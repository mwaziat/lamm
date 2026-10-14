# LAMM v0.2.1 — Adaptive OS Resource Policy

## What is implemented

The GNOME 50 bridge runs inside GNOME Shell, publishes window PID and activity (`ACTIVE`, `VISIBLE`, `BACKGROUND`) to `$XDG_RUNTIME_DIR/lamm-activity.csv` every 2 seconds.

The Rust daemon monitors the bridge, tracks duration of inactivity, reads memory pressure (PSI), and maps PIDs to cgroups v2 `app-*.scope` and `snap.*.scope` units. Using `systemctl --user set-property --runtime`, it applies:

| State | Inactivity Duration | CPUWeight | MemoryLow | IOWeight |
|---|---|---:|---:|---:|
| ACTIVE | Receiving input | 400 | 256M | 400 |
| VISIBLE | On-screen (including split-screen) | 250 | 128M | 250 |
| BACKGROUND | Obscured / minimized (< 15 min) | 80 | 0 | 80 |
| IDLE | Obscured / minimized (>= 15 min) | 40 | 0 | 40 |

### Dynamic Features
1. **Snap Scope Resolution**: Resolves Ubuntu snaps (`snap.<app>.<app>-*.scope`) in addition to traditional desktop app scopes (`app-*.scope`).
2. **PSI-Gated Memory Reclaim**: Under elevated memory pressure (`/proc/pressure/memory` `some avg10 >= 10.0%`), triggers measured kernel page reclaim (`64M`) via cgroup v2 `memory.reclaim` for `IDLE` units with a 60-second per-unit cooldown.
3. **Critical Unit Protection**: Automatically protects core desktop components (PipeWire, WirePlumber, GNOME Shell, portals) from deprioritization.
4. **Signal Handling & Graceful Rollback**: Captures `SIGINT` and `SIGTERM`. When stopped, automatically resets all modified units to defaults (`CPUWeight=100`, `MemoryLow=0`, `IOWeight=100`) and removes the socket file.
5. **Closed Window Cleanup**: Scopes of closed windows are reset and removed from tracking immediately.

## Operational Constraints

- Pure OS-level cgroup scheduling and memory protection. No internal application/tab tampering.
- No automatic `SIGKILL` or `SIGSTOP`.
- Fail-open: If the GNOME bridge is stale (> 12s) or absent, no new modifications are performed.
- Non-root user session operations only via systemd user manager.

## Validation & Testing

1. `cargo check --workspace && cargo test --workspace`
2. `./scripts/install-user.sh`
3. `./scripts/install-gnome-bridge.sh`, then enable with `gnome-extensions enable lamm-activity@lamm.local`
4. Inspect running service and logs:
   - `systemctl --user status lamm-daemon`
   - `journalctl --user -u lamm-daemon -f`
   - `lammctl`
