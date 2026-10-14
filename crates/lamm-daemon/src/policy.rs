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
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const BACKGROUND_RECLAIM_COOLDOWN: Duration = Duration::from_secs(60);
const ACTIVE_RECLAIM_COOLDOWN: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
            Self::Active => 400,
            Self::Visible => 250,
            Self::Background => 80,
            Self::Idle => 50,
        }
    }

    pub fn memory_low(self) -> &'static str {
        match self {
            Self::Active => "256M",
            Self::Visible => "128M",
            Self::Background | Self::Idle => "0",
        }
    }

    pub fn io_weight(self) -> u16 {
        match self {
            Self::Active => 400,
            Self::Visible => 250,
            Self::Background => 100,
            Self::Idle => 50,
        }
    }

    pub fn percentage_reclaim_mb(self, current_mb: u64) -> u64 {
        if current_mb < 80 {
            return 0;
        }

        let pct = match self {
            Self::Active | Self::Visible => 0.10,
            Self::Background => 0.20,
            Self::Idle => 0.35,
        };

        let calculated = (current_mb as f64 * pct) as u64;

        match self {
            Self::Active | Self::Visible => calculated.min(800),
            Self::Background => calculated.min(1000),
            Self::Idle => calculated.min(2500),
        }
    }

    pub fn cooldown(self) -> Duration {
        match self {
            Self::Active | Self::Visible => ACTIVE_RECLAIM_COOLDOWN,
            Self::Background | Self::Idle => BACKGROUND_RECLAIM_COOLDOWN,
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

fn resolve_target_scope(pid: u32) -> io::Result<Option<TargetScope>> {
    let cgroups = fs::read_to_string(format!("/proc/{pid}/cgroup"))?;
    for line in cgroups.lines() {
        if let Some(path) = line.strip_prefix("0::") {
            for component in path.split('/') {
                let is_app_scope = (component.starts_with("app-") || component.starts_with("snap."))
                    && component.ends_with(".scope")
                    && component.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'@' | b'-' | b'\\'));
                if is_app_scope {
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

fn discover_active_app_scopes() -> HashMap<String, ScopeInfo> {
    let mut states = HashMap::new();
    let app_slice = "/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/app.slice";
    if let Ok(entries) = fs::read_dir(app_slice) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if (name.starts_with("app-") || name.starts_with("snap.")) && name.ends_with(".scope") {
                let rel_path = format!("/user.slice/user-1000.slice/user@1000.service/app.slice/{name}");
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

fn systemd_set(unit: &str, state: Visibility) -> io::Result<()> {
    let cpu_weight_arg = format!("CPUWeight={}", state.cpu_weight());
    let memory_low_arg = format!("MemoryLow={}", state.memory_low());
    let io_weight_arg = format!("IOWeight={}", state.io_weight());

    let result = Command::new("systemctl")
        .args([
            "--user",
            "set-property",
            "--runtime",
            unit,
            &cpu_weight_arg,
            &memory_low_arg,
            &io_weight_arg,
        ])
        .output()?;

    if result.status.success() == false {
        return Err(io::Error::other(format!("{unit}: {}", String::from_utf8_lossy(&result.stderr).trim())));
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
            "IOWeight=100",
        ])
        .output()?;

    if result.status.success() == false {
        return Err(io::Error::other(format!("{unit}: {}", String::from_utf8_lossy(&result.stderr).trim())));
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
    let mut last_seen: HashMap<String, Instant> = HashMap::new();
    let mut last_reclaim: HashMap<String, Instant> = HashMap::new();
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

        let current = if fresh {
            bridge_states(&path).unwrap_or_else(|_| discover_active_app_scopes())
        } else {
            discover_active_app_scopes()
        };

        let now = Instant::now();

        for (unit, info) in &current {
            if is_protected_unit(unit) {
                continue;
            }

            let mut state = info.state;

            if state == Visibility::Active || state == Visibility::Visible {
                last_seen.insert(unit.clone(), now);
            } else if state == Visibility::Background {
                let background_duration = last_seen
                    .get(unit)
                    .map(|seen| now.duration_since(*seen))
                    .unwrap_or(Duration::ZERO);

                if background_duration >= IDLE_TIMEOUT {
                    state = Visibility::Idle;
                }
            }

            let state_changed = applied.get(unit) != Some(&state);

            if state_changed {
                match systemd_set(unit, state) {
                    Ok(()) => {
                        applied.insert(unit.clone(), state);
                    }
                    Err(error) => {
                        if last_error.elapsed() > Duration::from_secs(30) {
                            eprintln!("LAMM policy: {error}");
                            last_error = Instant::now();
                        }
                    }
                }
            }

            // Execute percentage-based memory reclaim if cooldown allows
            let can_reclaim = last_reclaim
                .get(unit)
                .map(|t| now.duration_since(*t) >= state.cooldown())
                .unwrap_or(true);

            if can_reclaim {
                let current_mb = read_memory_current_mb(&info.cgroup_rel_path);
                let target_mb = state.percentage_reclaim_mb(current_mb);

                if target_mb > 0 {
                    let freed_mb = try_cgroup_reclaim(&info.cgroup_rel_path, target_mb);
                    let after_mb = read_memory_current_mb(&info.cgroup_rel_path);
                    last_reclaim.insert(unit.clone(), now);

                    if freed_mb > 0 {
                        eprintln!(
                            "LAMM: {unit} [{state:?}] -> Freed {freed_mb} MB (Target: {target_mb} MB | {current_mb} MB -> {after_mb} MB)"
                        );
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
            last_seen.remove(&unit);
            last_reclaim.remove(&unit);
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

        assert_eq!(Visibility::Background.memory_low(), "0");
        assert_eq!(Visibility::Active.percentage_reclaim_mb(50), 0);
        // 10% of 2000MB => 200MB
        assert_eq!(Visibility::Active.percentage_reclaim_mb(2000), 200);
        // 20% of 2000MB => 400MB
        assert_eq!(Visibility::Background.percentage_reclaim_mb(2000), 400);
        // 35% of 2000MB => 700MB
        assert_eq!(Visibility::Idle.percentage_reclaim_mb(2000), 700);
    }

    #[test]
    fn test_protected_units() {
        assert!(is_protected_unit("pipewire.service"));
        assert!(is_protected_unit("wireplumber.service"));
        assert!(is_protected_unit("gnome-shell.scope"));
        assert_eq!(is_protected_unit("app-gnome-firefox-123.scope"), false);
    }
}
