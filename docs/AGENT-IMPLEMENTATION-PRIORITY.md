# Agent handoff: implement optimizer, not another monitor

Read `OPTIMIZATION-CONTRACT.md` first. Treat it as the intended product behavior, not claims of shipped code.

Priority 0: Implement real GNOME 50 Wayland focus/visibility event integration. Design permissions and data exposed carefully. Unit-test visible multi-window, split-screen, multi-monitor, minimized and off-workspace transitions.

Priority 1: Implement safe cgroup-v2/systemd user-scope mapping, reversible relative protection and CPU weighting. Never move or write PIDs to systemd-owned cgroups without supported ownership APIs. Keep global OOM/system behavior unchanged.

Priority 2: Trigger changes only under sustained PSI pressure. Add hysteresis, cooldown, errors that restore previous values, explicit opt-in for actions, event log and comparative benchmarks.

Priority 3: Add a single bundled browser protocol that can have browser-specific adapters (Chrome/Chromium and Firefox). A browser adapter must report lifecycle blockers and use browser-approved freeze/discard commands. No direct renderer kill/stop.

Priority 4: Expose UI only to change mode, see decisions and undo them.

Rust + GTK4/Libadwaita + user daemon as chosen. Do not add per-app optimizer binaries to run separately. Avoid unverified performance or memory-saving claims. Set tests and compile checks on a real Ubuntu host before packaging.
