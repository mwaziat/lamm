use adw::prelude::*;
use gtk::glib;
use std::{io::Read, os::unix::net::UnixStream, time::Duration};

fn query() -> Result<lamm_core::Snapshot, String> {
    let path = lamm_core::socket_path().map_err(|e| e.to_string())?;
    let mut stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
    let mut input = String::new();
    stream.read_to_string(&mut input).map_err(|e| e.to_string())?;
    serde_json::from_str(&input).map_err(|e| e.to_string())
}

fn main() {
    let app = adw::Application::builder().application_id("org.lamm.Desktop").build();
    app.connect_activate(|app| {
        let window = adw::ApplicationWindow::builder()
            .application(app).title("LAMM").default_width(550).default_height(450).build();
        let header = adw::HeaderBar::new();
        let title = gtk::Label::new(Some("Linux Adaptive Memory Manager"));
        title.add_css_class("title-2");
        let state = gtk::Label::new(Some("Connecting to daemon…"));
        state.add_css_class("dim-label");
        let details = gtk::Label::new(None);
        details.set_xalign(0.0);
        details.set_selectable(true);
        details.set_wrap(true);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
        content.set_margin_top(24);
        content.set_margin_bottom(24);
        content.set_margin_start(24);
        content.set_margin_end(24);
        content.append(&title);
        content.append(&state);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content.append(&details);
        let note = gtk::Label::new(Some("Adaptive OS Policy: Dynamically manages CPU/IO weights, memory protections, and reclaim without killing or freezing processes."));
        note.set_wrap(true);
        note.set_xalign(0.0);
        note.add_css_class("dim-label");
        content.append(&note);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&header);
        root.append(&content);
        window.set_content(Some(&root));
        let refresh = move || {
            match query() {
                Ok(s) => {
                    let m = s.memory;
                    state.set_text("Connected · Monitoring enabled");
                    let processes = s.top_processes.iter().take(8).map(|p|
                        format!("{:>6}  {:>7.0} MiB   {}", p.pid, p.rss_kib as f64 / 1024., p.name))
                        .collect::<Vec<_>>().join("\n");
                    details.set_text(&format!(
                        "Memory total:       {:.2} GiB\nMemory available: {:.2} GiB\nSwap used:             {:.2} GiB\nMemory PSI (10s): {:.2}%\n\nTop processes (RSS):\n{}",
                        m.total_kib as f64 / 1048576., m.available_kib as f64 / 1048576.,
                        (m.swap_total_kib - m.swap_free_kib) as f64 / 1048576., s.pressure.avg10, processes));
                }
                Err(error) => {
                    state.set_text("Daemon unavailable");
                    details.set_text(&format!("{error}\n\nStart: systemctl --user start lamm-daemon"));
                }
            }
            glib::ControlFlow::Continue
        };
        refresh();
        glib::timeout_add_seconds_local(3, refresh);
        window.present();
    });
    app.run();
}
