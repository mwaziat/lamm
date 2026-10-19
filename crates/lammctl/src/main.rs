use std::{io::{self, Read}, os::unix::net::UnixStream, time::Duration};

fn run() -> io::Result<()> {
    let mut stream = UnixStream::connect(lamm_core::socket_path()?)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut body = String::new();
    stream.read_to_string(&mut body)?;
    let snapshot: lamm_core::Snapshot = serde_json::from_str(&body)?;
    let json = std::env::args().any(|arg| arg == "--json");
    if json {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        let mem = snapshot.memory;
        println!("LAMM | {} | adaptive-os-policy", snapshot.epoch_seconds);
        println!("RAM  total {:.2} GiB | available {:.2} GiB", mem.total_kib as f64 / 1048576., mem.available_kib as f64 / 1048576.);
        if let Some(zram) = &snapshot.zram {
            if zram.active {
                let disk_gib = zram.disksize_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let compr_gib = zram.compr_data_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let orig_gib = zram.orig_data_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let saved_gib = (orig_gib - compr_gib).max(0.0);
                let ratio = if compr_gib > 0.0 { orig_gib / compr_gib } else { 1.0 };
                println!("ZRAM size {:.2} GiB | used {:.2} GiB (from {:.2} GiB, +{:.2} GiB saved, ratio {:.2}x) | {} | swappiness {}",
                    disk_gib, compr_gib, orig_gib, saved_gib, ratio, zram.algorithm, zram.swappiness);
            }
        }
        println!("Swap used {:.2} GiB | PSI some avg10 {:.2}%", (mem.swap_total_kib - mem.swap_free_kib) as f64 / 1048576., snapshot.pressure.avg10);
        println!("Top processes (PSS proportional memory attribution):");
        for p in snapshot.top_processes.iter().take(10) {
            let mem_kib = if p.pss_kib > 0 { p.pss_kib } else { p.rss_kib };
            println!("{:>7} {:>8.1} MiB  {}", p.pid, mem_kib as f64 / 1024., p.name);
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("lammctl: {e}. Is lamm-daemon running?");
        std::process::exit(1);
    }
}
