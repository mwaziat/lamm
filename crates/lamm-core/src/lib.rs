use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub total_kib: u64,
    pub available_kib: u64,
    pub swap_total_kib: u64,
    pub swap_free_kib: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pressure {
    pub avg10: f64,
    pub avg60: f64,
    pub avg300: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessSample {
    pub pid: u32,
    pub name: String,
    pub rss_kib: u64,
    #[serde(default)]
    pub pss_kib: u64,
    pub ppid: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZramInfo {
    pub active: bool,
    pub disksize_bytes: u64,
    pub orig_data_bytes: u64,
    pub compr_data_bytes: u64,
    pub mem_used_bytes: u64,
    pub algorithm: String,
    pub swappiness: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PolicyConfig {
    pub retention_floor_pct: u8,
    pub idle_timeout_secs: u64,
    pub critical_available_mb: u64,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            retention_floor_pct: 50,
            idle_timeout_secs: 120,
            critical_available_mb: 1500,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub epoch_seconds: u64,
    pub memory: MemorySnapshot,
    pub pressure: Pressure,
    pub top_processes: Vec<ProcessSample>,
    pub mode: String,
    #[serde(default)]
    pub zram: Option<ZramInfo>,
    #[serde(default)]
    pub policy_config: Option<PolicyConfig>,
}

pub fn parse_meminfo(input: &str) -> io::Result<MemorySnapshot> {
    let lookup = |field: &str| -> io::Result<u64> {
        input.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key == field).then(|| value.split_whitespace().next()?.parse().ok()).flatten()
        }).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("missing {field}")))
    };
    Ok(MemorySnapshot {
        total_kib: lookup("MemTotal")?,
        available_kib: lookup("MemAvailable")?,
        swap_total_kib: lookup("SwapTotal")?,
        swap_free_kib: lookup("SwapFree")?,
    })
}

pub fn parse_pressure(input: &str) -> io::Result<Pressure> {
    let line = input.lines().find(|line| line.starts_with("some "))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing PSI some"))?;
    let field = |key: &str| -> io::Result<f64> {
        line.split_whitespace().find_map(|part| {
            let (name, value) = part.split_once('=')?;
            (name == key).then(|| value.parse().ok()).flatten()
        }).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("missing {key}")))
    };
    Ok(Pressure { avg10: field("avg10")?, avg60: field("avg60")?, avg300: field("avg300")? })
}

pub fn read_smaps_rollup(pid: u32) -> Option<(u64, u64)> {
    let content = fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
    let mut rss = None;
    let mut pss = None;
    for line in content.lines().skip(1) {
        if let Some(val) = line.strip_prefix("Rss:") {
            rss = val.split_whitespace().next().and_then(|v| v.parse().ok());
        } else if let Some(val) = line.strip_prefix("Pss:") {
            pss = val.split_whitespace().next().and_then(|v| v.parse().ok());
        }
        if rss.is_some() && pss.is_some() {
            break;
        }
    }
    Some((rss.unwrap_or(0), pss.unwrap_or(0)))
}

pub fn read_process(pid: u32) -> io::Result<ProcessSample> {
    let status = fs::read_to_string(format!("/proc/{pid}/status"))?;
    let name = status.lines().find_map(|line| line.strip_prefix("Name:\t"))
        .unwrap_or("unknown").to_owned();
    let value = |key: &str| -> u64 {
        status.lines().find_map(|line| line.strip_prefix(key))
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.parse().ok()).unwrap_or(0)
    };
    let default_rss = value("VmRSS:");
    let ppid = value("PPid:") as u32;
    let (rss_kib, pss_kib) = if let Some((rollup_rss, rollup_pss)) = read_smaps_rollup(pid) {
        (if rollup_rss > 0 { rollup_rss } else { default_rss }, rollup_pss)
    } else {
        (default_rss, default_rss)
    };

    Ok(ProcessSample { pid, name, rss_kib, pss_kib, ppid })
}

pub fn collect_snapshot() -> io::Result<Snapshot> {
    let memory = parse_meminfo(&fs::read_to_string("/proc/meminfo")?)?;
    let pressure = parse_pressure(&fs::read_to_string("/proc/pressure/memory")?)?;
    let mut top_processes = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() {
            if let Ok(process) = read_process(pid) {
                if process.rss_kib > 0 && process.ppid != 2 {
                    top_processes.push(process);
                }
            }
        }
    }
    top_processes.sort_unstable_by(|a, b| {
        let mem_b = if b.pss_kib > 0 { b.pss_kib } else { b.rss_kib };
        let mem_a = if a.pss_kib > 0 { a.pss_kib } else { a.rss_kib };
        mem_b.cmp(&mem_a)
    });
    top_processes.truncate(15);
    Ok(Snapshot {
        epoch_seconds: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        memory,
        pressure,
        top_processes,
        mode: "adaptive-os-policy".to_string(),
        zram: Some(read_zram_info()),
        policy_config: Some(load_policy_config()),
    })
}

pub fn read_zram_info() -> ZramInfo {
    let disksize_bytes = fs::read_to_string("/sys/block/zram0/disksize")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let active = disksize_bytes > 0;

    let algorithm = fs::read_to_string("/sys/block/zram0/comp_algorithm")
        .ok()
        .and_then(|s| {
            s.split_whitespace().find_map(|word| {
                if word.starts_with('[') && word.ends_with(']') {
                    Some(word[1..word.len() - 1].to_string())
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| "unknown".to_string());

    let (orig_data_bytes, compr_data_bytes, mem_used_bytes) = fs::read_to_string("/sys/block/zram0/mm_stat")
        .ok()
        .map(|s| {
            let mut parts = s.split_whitespace();
            let orig = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let compr = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let used = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            (orig, compr, used)
        })
        .unwrap_or((0, 0, 0));

    let swappiness = fs::read_to_string("/proc/sys/vm/swappiness")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(60);

    ZramInfo {
        active,
        disksize_bytes,
        orig_data_bytes,
        compr_data_bytes,
        mem_used_bytes,
        algorithm,
        swappiness,
    }
}

pub fn load_policy_config() -> PolicyConfig {
    let Some(config_dir) = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
    else {
        return PolicyConfig::default();
    };
    let file = config_dir.join("lamm").join("config.json");
    if let Ok(data) = fs::read_to_string(&file) {
        if let Ok(cfg) = serde_json::from_str(&data) {
            return cfg;
        }
    }
    PolicyConfig::default()
}

pub fn save_policy_config(config: &PolicyConfig) -> io::Result<()> {
    let Some(config_dir) = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
    else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "config dir missing"));
    };
    let dir = config_dir.join("lamm");
    fs::create_dir_all(&dir)?;
    let data = serde_json::to_string_pretty(config).map_err(io::Error::other)?;
    fs::write(dir.join("config.json"), data)
}

pub fn socket_path() -> io::Result<std::path::PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR missing; run in user session"))?;
    let path = Path::new(&runtime);
    if !path.is_dir() { return Err(io::Error::new(io::ErrorKind::NotFound, "runtime dir does not exist")); }
    Ok(path.join("lamm.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_memory() {
        let sample = "MemTotal: 16384 kB\nMemAvailable: 4096 kB\nSwapTotal: 8192 kB\nSwapFree: 5120 kB\n";
        let parsed = parse_meminfo(sample).expect("valid meminfo");
        assert_eq!(parsed.total_kib, 16384);
        assert_eq!(parsed.swap_total_kib - parsed.swap_free_kib, 3072);
    }
    #[test]
    fn parses_pressure() {
        let parsed = parse_pressure("some avg10=0.25 avg60=1.50 avg300=0.80 total=100\nfull avg10=0.00").expect("valid pressure");
        assert_eq!(parsed.avg60, 1.5);
    }
}
