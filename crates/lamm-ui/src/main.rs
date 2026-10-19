use adw::prelude::*;
use gtk::glib;
use std::{
    io::Read,
    os::unix::net::UnixStream,
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

fn query() -> Result<lamm_core::Snapshot, String> {
    let path = lamm_core::socket_path().map_err(|e| e.to_string())?;
    let mut stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
    let mut input = String::new();
    stream.read_to_string(&mut input).map_err(|e| e.to_string())?;
    serde_json::from_str(&input).map_err(|e| e.to_string())
}

fn apply_zram_settings(size: &str, swappiness: u32, algo: &str) -> Result<String, String> {
    let output = Command::new("pkexec")
        .args([
            "/usr/local/bin/lamm-zram-helper",
            "apply",
            size,
            &swappiness.to_string(),
            algo,
        ])
        .output()
        .map_err(|e| format!("Failed to execute pkexec: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn main() {
    let app = adw::Application::builder().application_id("org.lamm.Desktop").build();
    app.connect_activate(|app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("LAMM - Linux Adaptive Memory Manager")
            .default_width(620)
            .default_height(600)
            .build();

        let header = adw::HeaderBar::new();
        let view_stack = adw::ViewStack::new();

        // ==========================================
        // --- TAB 1: MONITOR ---
        // ==========================================
        let mon_page = adw::PreferencesPage::new();

        // Group 1: Memory Overview
        let sys_group = adw::PreferencesGroup::builder()
            .title("System Memory Overview")
            .description("Real-time physical RAM, ZRAM virtual memory, and kernel pressure")
            .build();

        const BAR_WIDTH: i32 = 140;

        let ram_row = adw::ActionRow::builder()
            .title("Physical RAM")
            .subtitle("Measuring memory...")
            .build();
        let ram_bar = gtk::ProgressBar::new();
        ram_bar.set_valign(gtk::Align::Center);
        ram_bar.set_size_request(BAR_WIDTH, 8);
        ram_row.add_suffix(&ram_bar);
        sys_group.add(&ram_row);

        let zram_row = adw::ActionRow::builder()
            .title("ZRAM Virtual Memory (Fast Paging)")
            .subtitle("Measuring ZRAM...")
            .build();
        let zram_badge = gtk::Label::new(Some("Active"));
        zram_badge.add_css_class("caption");
        zram_badge.add_css_class("dim-label");
        zram_badge.set_valign(gtk::Align::Center);
        zram_row.add_suffix(&zram_badge);
        sys_group.add(&zram_row);

        let swap_row = adw::ActionRow::builder()
            .title("Total Swap Space")
            .subtitle("Measuring swap...")
            .build();
        let swap_bar = gtk::ProgressBar::new();
        swap_bar.set_valign(gtk::Align::Center);
        swap_bar.set_size_request(BAR_WIDTH, 8);
        swap_row.add_suffix(&swap_bar);
        sys_group.add(&swap_row);

        let psi_row = adw::ActionRow::builder()
            .title("Kernel Memory Pressure (PSI)")
            .subtitle("Kernel stall indicator avg10")
            .build();
        let psi_label = gtk::Label::new(Some("0.0%"));
        psi_label.add_css_class("heading");
        psi_label.set_valign(gtk::Align::Center);
        psi_row.add_suffix(&psi_label);
        sys_group.add(&psi_row);

        mon_page.add(&sys_group);

        // Group 2: Top Memory Processes
        let proc_group = adw::PreferencesGroup::builder()
            .title("Top Memory Processes")
            .description("Processes monitored by LAMM with proportional working-set protection")
            .build();

        let proc_list_label = gtk::Label::new(None);
        proc_list_label.set_xalign(0.0);
        proc_list_label.set_selectable(true);
        proc_list_label.add_css_class("monospace");
        proc_list_label.set_margin_start(12);
        proc_list_label.set_margin_end(12);
        proc_list_label.set_margin_top(8);
        proc_list_label.set_margin_bottom(8);
        proc_group.add(&proc_list_label);

        mon_page.add(&proc_group);

        let mon_view_page = view_stack.add_titled(&mon_page, Some("monitor"), "Monitor");
        mon_view_page.set_icon_name(Some("utilities-system-monitor-symbolic"));

        // ==========================================
        // --- TAB 2: SETTINGS ---
        // ==========================================
        let set_page = adw::PreferencesPage::new();

        // Standardized equalized width for all dropdowns and action buttons
        const CTRL_WIDTH: i32 = 200;

        // Group: ZRAM Settings
        let zram_set_group = adw::PreferencesGroup::builder()
            .title("ZRAM Virtual Memory Settings")
            .description("Configure compressed virtual RAM partition size and kernel paging behavior")
            .build();

        let zram_sizes = ["Auto (Recommended)", "4 GB", "6 GB", "8 GB", "10 GB", "12 GB", "16 GB"];
        let size_dropdown = gtk::DropDown::from_strings(&zram_sizes);
        size_dropdown.set_size_request(CTRL_WIDTH, -1);
        size_dropdown.set_halign(gtk::Align::End);
        let size_row = adw::ActionRow::builder()
            .title("Partition Size")
            .subtitle("Maximum compressed swap space allocated in RAM")
            .build();
        size_row.add_suffix(&size_dropdown);
        zram_set_group.add(&size_row);

        let swap_options = ["120 (Optimal Fast Paging)", "150 (Aggressive Paging)", "100 (Standard)", "60 (Ubuntu Default)"];
        let swap_dropdown = gtk::DropDown::from_strings(&swap_options);
        swap_dropdown.set_size_request(CTRL_WIDTH, -1);
        swap_dropdown.set_halign(gtk::Align::End);
        let swap_row = adw::ActionRow::builder()
            .title("Kernel Swappiness")
            .subtitle("Paging aggressiveness to fast compressed ZRAM")
            .build();
        swap_row.add_suffix(&swap_dropdown);
        zram_set_group.add(&swap_row);

        let algo_options = ["lz4 (Fastest CPU)", "zstd (Highest Ratio)"];
        let algo_dropdown = gtk::DropDown::from_strings(&algo_options);
        algo_dropdown.set_size_request(CTRL_WIDTH, -1);
        algo_dropdown.set_halign(gtk::Align::End);
        let algo_row = adw::ActionRow::builder()
            .title("Compression Algorithm")
            .subtitle("lz4 has lowest CPU overhead; zstd achieves higher ratio")
            .build();
        algo_row.add_suffix(&algo_dropdown);
        zram_set_group.add(&algo_row);

        let apply_btn = gtk::Button::with_label("Apply ZRAM");
        apply_btn.add_css_class("suggested-action");
        apply_btn.set_valign(gtk::Align::Center);
        apply_btn.set_size_request(CTRL_WIDTH, -1);

        let zram_status_label = gtk::Label::new(None);
        zram_status_label.set_xalign(0.0);
        zram_status_label.add_css_class("dim-label");
        zram_status_label.add_css_class("caption");

        let apply_row = adw::ActionRow::builder()
            .title("Apply Configuration")
            .subtitle("Recreates /dev/zram0 partition with selected parameters")
            .build();
        apply_row.add_suffix(&apply_btn);
        zram_set_group.add(&apply_row);
        zram_set_group.add(&zram_status_label);

        set_page.add(&zram_set_group);

        // Group: Policy Settings
        let policy_group = adw::PreferencesGroup::builder()
            .title("Adaptive Memory Policy")
            .description("Per-process working-set protection and background relief")
            .build();

        let floor_options = ["50% (Recommended - Balanced)", "40% (Aggressive Relief)", "60% (Retention Floor)"];
        let floor_dropdown = gtk::DropDown::from_strings(&floor_options);
        floor_dropdown.set_size_request(CTRL_WIDTH, -1);
        floor_dropdown.set_halign(gtk::Align::End);
        let floor_row = adw::ActionRow::builder()
            .title("Retention Floor")
            .subtitle("Minimum active memory guaranteed for background apps")
            .build();
        floor_row.add_suffix(&floor_dropdown);
        policy_group.add(&floor_row);

        let relief_options = ["1500 MB (Recommended)", "1000 MB (Low RAM)", "2000 MB (Workloads)"];
        let relief_dropdown = gtk::DropDown::from_strings(&relief_options);
        relief_dropdown.set_size_request(CTRL_WIDTH, -1);
        relief_dropdown.set_halign(gtk::Align::End);
        let relief_row = adw::ActionRow::builder()
            .title("Relief Threshold")
            .subtitle("Reclaim cold cache when available RAM drops below")
            .build();
        relief_row.add_suffix(&relief_dropdown);
        policy_group.add(&relief_row);

        let idle_options = ["120s (Recommended)", "60s (Fast Idle)", "180s (Relaxed)"];
        let idle_dropdown = gtk::DropDown::from_strings(&idle_options);
        idle_dropdown.set_size_request(CTRL_WIDTH, -1);
        idle_dropdown.set_halign(gtk::Align::End);
        let idle_row = adw::ActionRow::builder()
            .title("Idle Timeout")
            .subtitle("Time before unfocused apps transition to idle")
            .build();
        idle_row.add_suffix(&idle_dropdown);
        policy_group.add(&idle_row);

        let save_policy_btn = gtk::Button::with_label("Save Policy");
        save_policy_btn.add_css_class("suggested-action");
        save_policy_btn.set_valign(gtk::Align::Center);
        save_policy_btn.set_size_request(CTRL_WIDTH, -1);

        let policy_status_label = gtk::Label::new(None);
        policy_status_label.set_xalign(0.0);
        policy_status_label.add_css_class("dim-label");
        policy_status_label.add_css_class("caption");

        let save_policy_row = adw::ActionRow::builder()
            .title("Save Policy")
            .subtitle("Saved to ~/.config/lamm/config.json (auto-reloaded)")
            .build();
        save_policy_row.add_suffix(&save_policy_btn);
        policy_group.add(&save_policy_row);
        policy_group.add(&policy_status_label);

        set_page.add(&policy_group);

        let set_view_page = view_stack.add_titled(&set_page, Some("settings"), "Settings");
        set_view_page.set_icon_name(Some("preferences-system-symbolic"));

        // Connect ViewSwitcher to Header
        let view_switcher = adw::ViewSwitcher::builder()
            .stack(&view_stack)
            .policy(adw::ViewSwitcherPolicy::Wide)
            .build();
        header.set_title_widget(Some(&view_switcher));

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&header);
        view_stack.set_vexpand(true);
        view_stack.set_hexpand(true);
        root.append(&view_stack);
        window.set_content(Some(&root));

        // Load existing policy config to populate dropdowns
        let initial_config = lamm_core::load_policy_config();
        match initial_config.retention_floor_pct {
            40 => floor_dropdown.set_selected(1),
            60 => floor_dropdown.set_selected(2),
            _ => floor_dropdown.set_selected(0),
        }
        match initial_config.critical_available_mb {
            1000 => relief_dropdown.set_selected(1),
            2000 => relief_dropdown.set_selected(2),
            _ => relief_dropdown.set_selected(0),
        }
        match initial_config.idle_timeout_secs {
            60 => idle_dropdown.set_selected(1),
            180 => idle_dropdown.set_selected(2),
            _ => idle_dropdown.set_selected(0),
        }

        // Apply ZRAM Click Handler
        let size_dd_clone = size_dropdown.clone();
        let swap_dd_clone = swap_dropdown.clone();
        let algo_dd_clone = algo_dropdown.clone();
        let zram_lbl_clone = zram_status_label.clone();
        let apply_btn_clone = apply_btn.clone();

        apply_btn.connect_clicked(move |_| {
            let size_idx = size_dd_clone.selected();
            let size_str = match size_idx {
                1 => "4G",
                2 => "6G",
                3 => "8G",
                4 => "10G",
                5 => "12G",
                6 => "16G",
                _ => "auto",
            };

            let swap_idx = swap_dd_clone.selected();
            let swap_val: u32 = match swap_idx {
                1 => 150,
                2 => 100,
                3 => 60,
                _ => 120,
            };

            let algo_idx = algo_dd_clone.selected();
            let algo_str = match algo_idx {
                1 => "zstd",
                _ => "lz4",
            };

            zram_lbl_clone.set_text("Applying ZRAM configuration (authenticating via pkexec)...");
            apply_btn_clone.set_sensitive(false);

            let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
            let size_owned = size_str.to_string();
            let algo_owned = algo_str.to_string();

            std::thread::spawn(move || {
                let result = apply_zram_settings(&size_owned, swap_val, &algo_owned);
                let _ = tx.send(result);
            });

            let btn_resp = apply_btn_clone.clone();
            let lbl_resp = zram_lbl_clone.clone();
            glib::timeout_add_local(Duration::from_millis(200), move || {
                if let Ok(res) = rx.try_recv() {
                    btn_resp.set_sensitive(true);
                    match res {
                        Ok(msg) => lbl_resp.set_text(&format!("Success: {msg}")),
                        Err(err) => lbl_resp.set_text(&format!("Error: {err}")),
                    }
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
        });

        // Save Policy Click Handler
        let floor_dd_clone = floor_dropdown.clone();
        let relief_dd_clone = relief_dropdown.clone();
        let idle_dd_clone = idle_dropdown.clone();
        let pol_lbl_clone = policy_status_label.clone();

        save_policy_btn.connect_clicked(move |_| {
            let floor_pct = match floor_dd_clone.selected() {
                1 => 40,
                2 => 60,
                _ => 50,
            };

            let relief_mb = match relief_dd_clone.selected() {
                1 => 1000,
                2 => 2000,
                _ => 1500,
            };

            let idle_secs = match idle_dd_clone.selected() {
                1 => 60,
                2 => 180,
                _ => 120,
            };

            let cfg = lamm_core::PolicyConfig {
                retention_floor_pct: floor_pct,
                critical_available_mb: relief_mb,
                idle_timeout_secs: idle_secs,
            };

            match lamm_core::save_policy_config(&cfg) {
                Ok(()) => {
                    pol_lbl_clone.set_text("Configuration saved! Daemon applies changes within 5s.");
                }
                Err(e) => {
                    pol_lbl_clone.set_text(&format!("Failed to save config: {e}"));
                }
            }
        });

        // Periodic Refresh
        let ram_row_clone = ram_row.clone();
        let ram_bar_clone = ram_bar.clone();
        let psi_lbl_clone = psi_label.clone();
        let zram_row_clone = zram_row.clone();
        let zram_badge_clone = zram_badge.clone();
        let swap_row_clone = swap_row.clone();
        let swap_bar_clone = swap_bar.clone();
        let proc_lbl_clone = proc_list_label.clone();
        let zram_dropdown_init = Arc::new(Mutex::new(false));

        let size_dd_init = size_dropdown.clone();
        let swap_dd_init = swap_dropdown.clone();
        let algo_dd_init = algo_dropdown.clone();

        let refresh = move || {
            match query() {
                Ok(s) => {
                    let m = s.memory;
                    let total_gib = m.total_kib as f64 / 1048576.0;
                    let avail_gib = m.available_kib as f64 / 1048576.0;
                    let used_gib = (m.total_kib.saturating_sub(m.available_kib)) as f64 / 1048576.0;
                    let ram_fraction = if total_gib > 0.0 { (used_gib / total_gib).clamp(0.0, 1.0) } else { 0.0 };

                    ram_row_clone.set_subtitle(&format!(
                        "Available: {:.2} GiB · In use: {:.2} GiB / {:.2} GiB ({:.0}%)",
                        avail_gib, used_gib, total_gib, ram_fraction * 100.0
                    ));
                    ram_bar_clone.set_fraction(ram_fraction);

                    let swap_used_gib = (m.swap_total_kib.saturating_sub(m.swap_free_kib)) as f64 / 1048576.0;
                    let swap_total_gib = m.swap_total_kib as f64 / 1048576.0;
                    let swap_fraction = if swap_total_gib > 0.0 { (swap_used_gib / swap_total_gib).clamp(0.0, 1.0) } else { 0.0 };

                    swap_row_clone.set_subtitle(&format!(
                        "Used: {:.2} GiB / Total: {:.2} GiB ({:.0}%) · Fast ZRAM + Disk Swap",
                        swap_used_gib, swap_total_gib, swap_fraction * 100.0
                    ));
                    swap_bar_clone.set_fraction(swap_fraction);

                    psi_lbl_clone.set_text(&format!("{:.1}%", s.pressure.avg10));

                    let zram_opt = s.zram.or_else(|| {
                        let info = lamm_core::read_zram_info();
                        if info.active { Some(info) } else { None }
                    });

                    if let Some(zram) = zram_opt {
                        if zram.active {
                            let disk_gib = zram.disksize_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                            let compr_gib = zram.compr_data_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                            let orig_gib = zram.orig_data_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                            let saved_gib = (orig_gib - compr_gib).max(0.0);
                            let ratio = if compr_gib > 0.0 { orig_gib / compr_gib } else { 1.0 };
                            let used_pct = if disk_gib > 0.0 { (orig_gib / disk_gib) * 100.0 } else { 0.0 };

                            zram_badge_clone.set_text(&format!("{} · swappiness: {}", zram.algorithm, zram.swappiness));
                            zram_row_clone.set_subtitle(&format!(
                                "Capacity: {:.0} GiB ({:.2} GiB used · {:.0}%) | In RAM: {:.2} GiB (+{:.2} GiB saved · {:.1}x)",
                                disk_gib, orig_gib, used_pct, compr_gib, saved_gib, ratio
                            ));

                            // Sync dropdown selection once on start
                            let mut init = zram_dropdown_init.lock().unwrap();
                            if !*init {
                                *init = true;
                                let rounded_gb = (disk_gib + 0.5) as u64;
                                match rounded_gb {
                                    4 => size_dd_init.set_selected(1),
                                    6 => size_dd_init.set_selected(2),
                                    8 => size_dd_init.set_selected(3),
                                    10 => size_dd_init.set_selected(4),
                                    12 => size_dd_init.set_selected(5),
                                    16 => size_dd_init.set_selected(6),
                                    _ => size_dd_init.set_selected(0),
                                }

                                match zram.swappiness {
                                    150 => swap_dd_init.set_selected(1),
                                    100 => swap_dd_init.set_selected(2),
                                    60 => swap_dd_init.set_selected(3),
                                    _ => swap_dd_init.set_selected(0),
                                }

                                if zram.algorithm.contains("zstd") {
                                    algo_dd_init.set_selected(1);
                                } else {
                                    algo_dd_init.set_selected(0);
                                }
                            }
                        } else {
                            zram_badge_clone.set_text("Inactive");
                            zram_row_clone.set_subtitle("ZRAM partition not active");
                        }
                    } else {
                        zram_badge_clone.set_text("Inactive");
                        zram_row_clone.set_subtitle("ZRAM partition not active");
                    }

                    let processes = s.top_processes.iter().take(10).map(|p| {
                        let mem_mb = if p.pss_kib > 0 { p.pss_kib as f64 / 1024.0 } else { p.rss_kib as f64 / 1024.0 };
                        format!("{:>7}   {:>8.1} MiB   {}", p.pid, mem_mb, p.name)
                    }).collect::<Vec<_>>().join("\n");

                    proc_lbl_clone.set_text(&format!("    PID       Memory   Process Name\n{processes}"));
                }
                Err(error) => {
                    ram_row_clone.set_subtitle("Daemon unavailable");
                    swap_row_clone.set_subtitle("Daemon unavailable");
                    proc_lbl_clone.set_text(&format!("{error}\n\nStart: systemctl --user start lamm-daemon"));
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
