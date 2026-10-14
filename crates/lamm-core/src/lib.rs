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
    pub ppid: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub epoch_seconds: u64,
    pub memory: MemorySnapshot,
    pub pressure: Pressure,
    pub top_processes: Vec<ProcessSample>,
    pub mode: String,
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

pub fn read_process(pid: u32) -> io::Result<ProcessSample> {
    let status = fs::read_to_string(format!("/proc/{pid}/status"))?;
    let name = status.lines().find_map(|line| line.strip_prefix("Name:\t"))
        .unwrap_or("unknown").to_owned();
    let value = |key: &str| -> u64 {
        status.lines().find_map(|line| line.strip_prefix(key))
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.parse().ok()).unwrap_or(0)
    };
    Ok(ProcessSample { pid, name, rss_kib: value("VmRSS:"), ppid: value("PPid:") as u32 })
}

pub fn collect_snapshot() -> io::Result<Snapshot> {
    let memory = parse_meminfo(&fs::read_to_string("/proc/meminfo")?)?;
    let pressure = parse_pressure(&fs::read_to_string("/proc/pressure/memory")?)?;
    let mut top_processes = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() {
            if let Ok(process) = read_process(pid) {
                top_processes.push(process);
            }
        }
    }
    top_processes.sort_unstable_by(|a, b| b.rss_kib.cmp(&a.rss_kib));
    top_processes.truncate(15);
    Ok(Snapshot {
        epoch_seconds: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        memory, pressure, top_processes, mode: "adaptive-os-policy".to_string(),
    })
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
