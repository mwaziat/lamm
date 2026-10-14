mod policy;

use std::{
    fs,
    io::{self, Write},
    os::unix::net::UnixListener,
    sync::atomic::Ordering,
    time::Duration,
};

fn run() -> io::Result<()> {
    policy::setup_signals();

    let path = lamm_core::socket_path()?;
    if path.exists() {
        match std::os::unix::net::UnixStream::connect(&path) {
            Ok(_) => return Err(io::Error::new(io::ErrorKind::AddrInUse, "LAMM daemon already running")),
            Err(_) => fs::remove_file(&path)?,
        }
    }

    let listener = UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;
    fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    eprintln!("LAMM daemon with adaptive OS policy engine listening at {}", path.display());

    let policy_thread = std::thread::spawn(policy::run_policy);

    while policy::RUNNING.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let result = lamm_core::collect_snapshot()
                    .and_then(|snapshot| serde_json::to_vec(&snapshot)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)));
                match result {
                    Ok(mut json) => {
                        json.push(b'\n');
                        let _ = stream.write_all(&json);
                    }
                    Err(error) => eprintln!("snapshot error: {error}"),
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(error) => eprintln!("connection error: {error}"),
        }
    }

    let _ = policy_thread.join();
    let _ = fs::remove_file(&path);
    eprintln!("LAMM daemon stopped cleanly.");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("lamm-daemon: {error}");
        std::process::exit(1);
    }
}
