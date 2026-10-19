use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub static RUNNING: AtomicBool = AtomicBool::new(true);

const POLL_INTERVAL: Duration = Duration::from_millis(1000);
const BRIDGE_MAX_AGE: Duration = Duration::from_secs(12);
const BACKGROUND_RECLAIM_COOLDOWN: Duration = Duration::from_secs(60);
const ACTIVE_RECLAIM_COOLDOWN: Duration = Duration::from_secs(300);

pub const MIN_PROCESS_THRESHOLD_MB: u64 = 150;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Visibility {
    Active,
    Visible,
    Background,
    Idle,
}

impl Visibility {
    pub fn rank(self) -> u8 {
        match self {
            Self::Active => 4,
            Self::Visible => 3,
            Self::Background => 2,
            Self::Idle => 1,
        }
    }

    pub fn cpu_weight(self) -> u16 {
        match self {
            Self::Active => 1000,
            Self::Visible => 400,
            Self::Background => 50,
            Self::Idle => 20,
        }
    }

    pub fn memory_low_mb(self, current_mb: u64) -> u64 {
        match self {
            Self::Active => {
                if current_mb < MIN_PROCESS_THRESHOLD_MB {
                    current_mb.min(64)
                } else {
                    (current_mb as f64 * 0.70) as u64
                }
            }
            Self::Visible => {
                if current_mb < MIN_PROCESS_THRESHOLD_MB {
                    0
                } else {
                    (current_mb as f64 * 0.40) as u64
                }
            }
            Self::Background | Self::Idle => 0,
        }
    }

    pub fn io_weight(self) -> u16 {
        match self {
            Self::Active => 1000,
            Self::Visible => 400,
            Self::Background => 50,
            Self::Idle => 20,
        }
    }

    pub fn percentage_reclaim_mb(self, current_mb: u64, peak_mb: u64, floor_pct: u8) -> u64 {
        if current_mb < MIN_PROCESS_THRESHOLD_MB {
            return 0;
        }

        match self {
            Self::Active | Self::Visible => 0,
            Self::Background => {
                let retention_floor = (peak_mb as f64 * (floor_pct as f64 / 100.0)) as u64;
                if current_mb > retention_floor {
                    let excess = current_mb - retention_floor;
                    ((excess as f64 * 0.25) as u64).max(50)
                } else {
                    0
                }
            }
            Self::Idle => {
                let retention_floor = (peak_mb as f64 * (floor_pct as f64 / 100.0)) as u64;
                if current_mb > retention_floor {
                    let excess = current_mb - retention_floor;
                    ((excess as f64 * 0.50) as u64).max(100)
                } else {
                    0
                }
            }
        }
    }

    pub fn memory_high_mb(self, current_mb: u64, peak_mb: u64, floor_pct: u8) -> Option<u64> {
        if current_mb < MIN_PROCESS_THRESHOLD_MB {
            return None;
        }

        match self {
            Self::Active | Self::Visible => None,
            Self::Background => {
                let ceiling = (peak_mb as f64 * 1.50) as u64;
                Some(ceiling.max(current_mb.saturating_add(1024)))
            }
            Self::Idle => {
                let retention_floor = (peak_mb as f64 * (floor_pct as f64 / 100.0)) as u64;
                Some(current_mb.saturating_add(512).max(retention_floor.saturating_add(512)))
            }
        }
    }

    pub fn cooldown(self) -> Duration {
        match self {
            Self::Active | Self::Visible => ACTIVE_RECLAIM_COOLDOWN,
            Self::Background => BACKGROUND_RECLAIM_COOLDOWN,
            Self::Idle => Duration::from_secs(120),
        }
    }
}

extern "C" fn handle_signal(_sig: i32) {
    RUNNING.store(false, Ordering::SeqCst);
}

pub fn setup_signals() {
    extern "C" {
        fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
    }
    unsafe {
        signal(2, handle_signal);  // SIGINT
        signal(15, handle_signal); // SIGTERM
    }
}

#[derive(Clone, Debug)]
struct TargetScope {
    unit: String,
    cgroup_rel_path: String,
}

struct ScopeInfo {
    state: Visibility,
    cgroup_rel_path: String,
}

fn runtime_file() -> io::Result<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR missing"))?;
    Ok(PathBuf::from(dir).join("lamm-activity.csv"))
}

fn is_managed_scope(component: &str) -> bool {
    let valid_prefix = component.starts_with("app-")
        || component.starts_with("snap.")
        || component.starts_with("ptyxis-spawn-")
        || component.starts_with("vte-spawn-")
        || component.starts_with("flatpak-");

    valid_prefix
        && component.ends_with(".scope")
        && component.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'@' | b'-' | b'\\'))
}

fn resolve_target_scope(pid: u32) -> io::Result<Option<TargetScope>> {
    let cgroups = fs::read_to_string(format!("/proc/{pid}/cgroup"))?;
    for line in cgroups.lines() {
        if let Some(path) = line.strip_prefix("0::") {
            for component in path.split('/') {
                if is_managed_scope(component) {
                    return Ok(Some(TargetScope {
                        unit: component.to_owned(),
                        cgroup_rel_path: path.to_owned(),
                    }));
                }
            }
        }
    }
    Ok(None)
}

fn is_protected_unit(unit: &str) -> bool {
    unit.contains("pipewire")
        || unit.contains("wireplumber")
        || unit.contains("gnome-shell")
        || unit.contains("portal")
}

fn read_memory_current_mb(cgroup_rel_path: &str) -> u64 {
    let path = format!("/sys/fs/cgroup{cgroup_rel_path}/memory.current");
    fs::read_to_string(path)
        .ok()
        .and_then(|val| val.trim().parse::<u64>().ok())
        .map(|bytes| bytes / 1048576)
        .unwrap_or(0)
}

fn try_cgroup_reclaim(cgroup_rel_path: &str, amount_mb: u64) -> u64 {
    let path = format!("/sys/fs/cgroup{cgroup_rel_path}/memory.reclaim");
    if Path::new(&path).exists() == false || amount_mb == 0 {
        return 0;
    }
    let before = read_memory_current_mb(cgroup_rel_path);
    let _ = fs::write(&path, format!("{amount_mb}M\n"));
    let after = read_memory_current_mb(cgroup_rel_path);
    before.saturating_sub(after)
}

fn bridge_states(path: &Path) -> io::Result<HashMap<String, ScopeInfo>> {
    let data = fs::read_to_string(path)?;
    let mut states: HashMap<String, ScopeInfo> = HashMap::new();
    if data.starts_with("TIME,") == false {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid bridge header"));
    }
    for line in data.lines().skip(1) {
        let Some((pid_text, state_text)) = line.split_once(',') else { continue };
        let Ok(pid) = pid_text.parse::<u32>() else { continue };
        let state = match state_text {
            "ACTIVE" => Visibility::Active,
            "VISIBLE" => Visibility::Visible,
            "BACKGROUND" => Visibility::Background,
            _ => continue,
        };
        if let Ok(Some(target)) = resolve_target_scope(pid) {
            states
                .entry(target.unit)
                .and_modify(|info| {
                    if state.rank() > info.state.rank() {
                        info.state = state;
                    }
                })
                .or_insert(ScopeInfo {
                    state,
                    cgroup_rel_path: target.cgroup_rel_path,
                });
        }
    }
    Ok(states)
}

fn user_app_slice_path() -> (PathBuf, String) {
    extern "C" {
        fn getuid() -> u32;
    }
    let uid = unsafe { getuid() };
    let rel_prefix = format!("/user.slice/user-{uid}.slice/user@{uid}.service/app.slice");
    let full_path = PathBuf::from(format!("/sys/fs/cgroup{rel_prefix}"));
    (full_path, rel_prefix)
}

fn discover_active_app_scopes() -> HashMap<String, ScopeInfo> {
    let mut states = HashMap::new();
    let (app_slice, rel_prefix) = user_app_slice_path();
    if let Ok(entries) = fs::read_dir(app_slice) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if is_managed_scope(&name) {
                let rel_path = format!("{rel_prefix}/{name}");
                states.insert(
                    name,
                    ScopeInfo {
                        state: Visibility::Background,
                        cgroup_rel_path: rel_path,
                    },
                );
            }
        }
    }
    states
}

fn read_available_memory_mb() -> u64 {
    fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|content| {
            content.lines().find_map(|line| {
                let (key, val) = line.split_once(':')?;
                if key.trim() == "MemAvailable" {
                    val.split_whitespace().next()?.parse::<u64>().ok()
                } else {
                    None
                }
            })
        })
        .map(|kib| kib / 1024)
        .unwrap_or(u64::MAX)
}

fn systemd_set(unit: &str, state: Visibility, current_mb: u64, peak_mb: u64, floor_pct: u8, cgroup_rel_path: &str) -> io::Result<()> {
    let cpu_weight_arg = format!("CPUWeight={}", state.cpu_weight());
    let memory_low_val = state.memory_low_mb(current_mb);
    let memory_low_arg = format!("MemoryLow={}M", memory_low_val);
    let memory_high_arg = match state.memory_high_mb(current_mb, peak_mb, floor_pct) {
        Some(mb) => format!("MemoryHigh={}M", mb),
        None => "MemoryHigh=infinity".to_string(),
    };
    let io_weight_arg = format!("IOWeight={}", state.io_weight());

    let oom_pref_arg = match state {
        Visibility::Active | Visibility::Visible => "ManagedOOMPreference=avoid",
        Visibility::Background | Visibility::Idle => "ManagedOOMPreference=none",
    };

    // Instant direct cgroupfs writes for zero-latency enforcement
    let cgroup_high_file = format!("/sys/fs/cgroup{cgroup_rel_path}/memory.high");
    if Path::new(&cgroup_high_file).exists() {
        let val = match state.memory_high_mb(current_mb, peak_mb, floor_pct) {
            Some(mb) => format!("{}\n", mb * 1048576),
            None => "max\n".to_string(),
        };
        let _ = fs::write(&cgroup_high_file, val);
    }

    let cgroup_low_file = format!("/sys/fs/cgroup{cgroup_rel_path}/memory.low");
    if Path::new(&cgroup_low_file).exists() {
        let val = format!("{}\n", memory_low_val * 1048576);
        let _ = fs::write(&cgroup_low_file, val);
    }

    let result = Command::new("systemctl")
        .args([
            "--user",
            "set-property",
            "--runtime",
            unit,
            &cpu_weight_arg,
            &memory_low_arg,
            &memory_high_arg,
            &io_weight_arg,
            oom_pref_arg,
        ])
        .output()?;

    if result.status.success() == false {
        let fallback = Command::new("systemctl")
            .args([
                "--user",
                "set-property",
                "--runtime",
                unit,
                &cpu_weight_arg,
                &memory_low_arg,
                &memory_high_arg,
                &io_weight_arg,
            ])
            .output()?;

        if fallback.status.success() == false {
            return Err(io::Error::other(format!(
                "{unit}: {}",
                String::from_utf8_lossy(&fallback.stderr).trim()
            )));
        }
    }
    Ok(())
}

fn systemd_reset(unit: &str) -> io::Result<()> {
    let result = Command::new("systemctl")
        .args([
            "--user",
            "set-property",
            "--runtime",
            unit,
            "CPUWeight=100",
            "MemoryLow=0",
            "MemoryHigh=infinity",
            "IOWeight=100",
            "ManagedOOMPreference=none",
        ])
        .output()?;

    if result.status.success() == false {
        let fallback = Command::new("systemctl")
            .args([
                "--user",
                "set-property",
                "--runtime",
                unit,
                "CPUWeight=100",
                "MemoryLow=0",
                "MemoryHigh=infinity",
                "IOWeight=100",
            ])
            .output()?;

        if fallback.status.success() == false {
            return Err(io::Error::other(format!(
                "{unit}: {}",
                String::from_utf8_lossy(&fallback.stderr).trim()
            )));
        }
    }
    Ok(())
}

pub fn run_policy() {
    let path = match runtime_file() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("LAMM policy disabled: {e}");
            return;
        }
    };

    let mut applied: HashMap<String, Visibility> = HashMap::new();
    let mut applied_mem: HashMap<String, u64> = HashMap::new();
    let mut last_seen: HashMap<String, Instant> = HashMap::new();
    let mut last_reclaim: HashMap<String, Instant> = HashMap::new();
    let mut reclaimed_in_state: HashMap<String, Visibility> = HashMap::new();
    let mut peak_memory: HashMap<String, u64> = HashMap::new();
    let mut last_error = Instant::now() - Duration::from_secs(60);
    let mut last_file_mtime = None;
    let mut last_periodic_check = Instant::now() - Duration::from_secs(10);

    while RUNNING.load(Ordering::SeqCst) {
        std::thread::sleep(POLL_INTERVAL);
        if RUNNING.load(Ordering::SeqCst) == false {
            break;
        }

        let mtime = fs::metadata(&path).and_then(|m| m.modified()).ok();
        let fresh = mtime.and_then(|t| t.elapsed().ok()).is_some_and(|age| age < BRIDGE_MAX_AGE);

        let file_changed = mtime.is_some() && mtime != last_file_mtime;
        let interval_elapsed = last_periodic_check.elapsed() >= Duration::from_secs(5);

        if file_changed == false && interval_elapsed == false {
            continue;
        }

        last_file_mtime = mtime;
        last_periodic_check = Instant::now();
        let config = lamm_core::load_policy_config();
        let idle_timeout = Duration::from_secs(config.idle_timeout_secs);
        let floor_pct = config.retention_floor_pct;
        let critical_avail = config.critical_available_mb;

        let mut current = discover_active_app_scopes();
        if fresh {
            if let Ok(bridge) = bridge_states(&path) {
                for (unit, info) in bridge {
                    current.insert(unit, info);
                }
            }
        } else {
            // When extension bridge is inactive/missing, we cannot determine window focus.
            // DO NOT demote apps to Idle or reclaim memory! Treat them as Visible so user
            // apps (VS Code, Chrome) have full unthrottled memory (MemoryHigh=infinity) and are never chopped.
            for info in current.values_mut() {
                info.state = Visibility::Visible;
            }
        }

        let now = Instant::now();

        for (unit, info) in &current {
            if is_protected_unit(unit) {
                continue;
            }

            let mut state = info.state;
            let current_mb = read_memory_current_mb(&info.cgroup_rel_path);

            if state == Visibility::Active || state == Visibility::Visible {
                last_seen.insert(unit.clone(), now);
                let peak = peak_memory.entry(unit.clone()).or_insert(current_mb);
                if current_mb > *peak {
                    *peak = current_mb;
                }
            } else if state == Visibility::Background {
                let seen = last_seen.entry(unit.clone()).or_insert(now);
                let background_duration = now.duration_since(*seen);

                if fresh && background_duration >= idle_timeout {
                    state = Visibility::Idle;
                }
            }

            let peak_mb = *peak_memory.get(unit).unwrap_or(&current_mb);
            let state_changed = applied.get(unit) != Some(&state);
            let mem_grown = state == Visibility::Background
                && current_mb > applied_mem.get(unit).copied().unwrap_or(0).saturating_add(500);

            if state_changed || mem_grown {
                match systemd_set(unit, state, current_mb, peak_mb, floor_pct, &info.cgroup_rel_path) {
                    Ok(()) => {
                        applied.insert(unit.clone(), state);
                        applied_mem.insert(unit.clone(), current_mb);
                    }
                    Err(error) => {
                        if last_error.elapsed() > Duration::from_secs(30) {
                            eprintln!("LAMM policy: {error}");
                            last_error = Instant::now();
                        }
                        applied.insert(unit.clone(), state);
                        applied_mem.insert(unit.clone(), current_mb);
                    }
                }

                if state == Visibility::Active || state == Visibility::Visible {
                    reclaimed_in_state.remove(unit);
                    eprintln!(
                        "LAMM: {unit} -> [{state:?}] accelerated (CPU: {}, IO: {}, MemoryLow: {}M, MemoryHigh: infinity)",
                        state.cpu_weight(),
                        state.io_weight(),
                        state.memory_low_mb(current_mb)
                    );
                } else if state_changed {
                    let high_desc = match state.memory_high_mb(current_mb, peak_mb, floor_pct) {
                        Some(h) => format!("{h}M"),
                        None => "infinity".to_string(),
                    };
                    eprintln!(
                        "LAMM: {unit} -> [{state:?}] throttle (CPU: {}, IO: {}, MemoryHigh: {high_desc})",
                        state.cpu_weight(),
                        state.io_weight(),
                    );
                }
            }

            let target_mb = state.percentage_reclaim_mb(current_mb, peak_mb, floor_pct);
            let retention_floor = (peak_mb as f64 * (floor_pct as f64 / 100.0)) as u64;

            let needs_idle_trim = state == Visibility::Idle && current_mb > (retention_floor + 100);
            let already_reclaimed = reclaimed_in_state.get(unit) == Some(&state);
            let cooldown_ok = last_reclaim
                .get(unit)
                .map(|t| now.duration_since(*t) >= state.cooldown())
                .unwrap_or(true);

            let can_reclaim = fresh;
            if (!already_reclaimed || needs_idle_trim) && cooldown_ok && target_mb > 0 && can_reclaim {
                let freed_mb = try_cgroup_reclaim(&info.cgroup_rel_path, target_mb);
                let after_mb = read_memory_current_mb(&info.cgroup_rel_path);
                last_reclaim.insert(unit.clone(), now);

                if current_mb <= (retention_floor + 100) {
                    reclaimed_in_state.insert(unit.clone(), state);
                }

                if freed_mb > 0 {
                    eprintln!(
                        "LAMM: {unit} [{state:?}] -> Freed {freed_mb} MB (Target: {target_mb} MB | {current_mb} MB -> {after_mb} MB | {floor_pct}% Floor: {retention_floor} MB)",
                    );
                }
            }
        }

        let available_mb = read_available_memory_mb();
        if available_mb < critical_avail {
            for (unit, info) in &current {
                if is_protected_unit(unit) {
                    continue;
                }
                if info.state == Visibility::Idle || (available_mb < 1000 && info.state == Visibility::Background) {
                    let can_relief = last_reclaim
                        .get(unit)
                        .map(|t| now.duration_since(*t) >= Duration::from_secs(30))
                        .unwrap_or(true);
                    if can_relief {
                        let cur_mb = read_memory_current_mb(&info.cgroup_rel_path);
                        let peak = *peak_memory.get(unit).unwrap_or(&cur_mb);
                        let floor = (peak as f64 * (floor_pct as f64 / 100.0)) as u64;
                        if cur_mb > floor + 100 {
                            let excess = cur_mb - floor;
                            let relief_mb = ((excess as f64 * 0.25) as u64).max(50);
                            if relief_mb > 0 {
                                let freed = try_cgroup_reclaim(&info.cgroup_rel_path, relief_mb);
                                last_reclaim.insert(unit.clone(), now);
                                if freed > 0 {
                                    eprintln!(
                                        "LAMM: Proactive relief on {unit} [{:?}] -> Freed {freed} MB (System Available: {available_mb} MB)",
                                        info.state
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        let missing: HashSet<_> = applied
            .keys()
            .filter(|unit| current.contains_key(*unit) == false)
            .cloned()
            .collect();

        for unit in missing {
            let _ = systemd_reset(&unit);
            applied.remove(&unit);
            applied_mem.remove(&unit);
            last_seen.remove(&unit);
            last_reclaim.remove(&unit);
            reclaimed_in_state.remove(&unit);
            peak_memory.remove(&unit);
        }
    }

    for unit in applied.keys() {
        let _ = systemd_reset(unit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visibility_levels() {
        assert!(Visibility::Active.rank() > Visibility::Visible.rank());
        assert!(Visibility::Visible.rank() > Visibility::Background.rank());
        assert!(Visibility::Background.rank() > Visibility::Idle.rank());

        assert!(Visibility::Active.cpu_weight() > Visibility::Visible.cpu_weight());
        assert!(Visibility::Visible.cpu_weight() > Visibility::Background.cpu_weight());
        assert!(Visibility::Background.cpu_weight() > Visibility::Idle.cpu_weight());

        assert_eq!(Visibility::Background.memory_low_mb(2000), 0);
        assert_eq!(Visibility::Idle.memory_low_mb(2000), 0);
        assert_eq!(Visibility::Active.memory_low_mb(400), 280);
        assert_eq!(Visibility::Active.memory_low_mb(2000), 1400);
        assert_eq!(Visibility::Active.memory_low_mb(12000), 8400);

        assert_eq!(Visibility::Visible.memory_low_mb(400), 160);
        assert_eq!(Visibility::Visible.memory_low_mb(2000), 800);

        assert_eq!(Visibility::Active.memory_high_mb(2000, 2000, 50), None);
        assert_eq!(Visibility::Visible.memory_high_mb(2000, 2000, 50), None);
        assert_eq!(Visibility::Background.memory_high_mb(2000, 2000, 50), Some(3024));
        assert_eq!(Visibility::Idle.memory_high_mb(2000, 2000, 50), Some(2512));
        assert_eq!(Visibility::Background.memory_high_mb(100, 100, 50), None);

        assert_eq!(Visibility::Active.percentage_reclaim_mb(50, 50, 50), 0);
        assert_eq!(Visibility::Active.percentage_reclaim_mb(2000, 2000, 50), 0);
        assert_eq!(Visibility::Visible.percentage_reclaim_mb(2000, 2000, 50), 0);

        assert_eq!(Visibility::Background.percentage_reclaim_mb(2000, 2000, 50), 250);
        assert_eq!(Visibility::Idle.percentage_reclaim_mb(2000, 2000, 50), 500);

        assert_eq!(Visibility::Background.percentage_reclaim_mb(12000, 12000, 50), 1500);
        assert_eq!(Visibility::Idle.percentage_reclaim_mb(12000, 12000, 50), 3000);

        assert_eq!(Visibility::Background.percentage_reclaim_mb(1000, 2000, 50), 0);
        assert_eq!(Visibility::Idle.percentage_reclaim_mb(1000, 2000, 50), 0);
        assert_eq!(Visibility::Idle.percentage_reclaim_mb(6000, 12000, 50), 0);

        assert_eq!(Visibility::Background.percentage_reclaim_mb(100, 100, 50), 0);
        assert_eq!(Visibility::Idle.percentage_reclaim_mb(100, 100, 50), 0);
    }

    #[test]
    fn test_protected_units() {
        assert!(is_protected_unit("pipewire.service"));
        assert!(is_protected_unit("wireplumber.service"));
        assert!(is_protected_unit("gnome-shell.scope"));
        assert_eq!(is_protected_unit("app-gnome-firefox-123.scope"), false);
    }

    #[test]
    fn test_managed_scopes() {
        assert!(is_managed_scope("app-gnome-google-chrome-123.scope"));
        assert!(is_managed_scope("snap.code.code-uuid.scope"));
        assert!(is_managed_scope("ptyxis-spawn-uuid.scope"));
        assert!(is_managed_scope("vte-spawn-uuid.scope"));
        assert!(is_managed_scope("flatpak-app.scope"));
        assert_eq!(is_managed_scope("pipewire.service"), false);
        assert_eq!(is_managed_scope("dbus.socket"), false);
    }
}
