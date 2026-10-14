# LAMM — Linux Adaptive Memory Manager (v0.2.0 OS Policy Prototype)

**Goal:** Optimize operating-system and application workloads based on what the user is actively viewing. GUI/dashboard is secondary.

## Operational component built first

- GNOME Shell 50 extension publishes focused/visible/background window PIDs.
- Rust background daemon maps PIDs to application systemd scopes (`app-*.scope`).
- Applies real `CPUWeight` and `MemoryLow` property changes through `systemctl --user set-property --runtime`.
- Supports split-screen: both visible windows are protected, focused window highest.
- Stale/missing desktop activity bridge => no new actions.
- No kill, freeze, tab discard, or reclaim until measured and safeguarded.

Read **`docs/REAL-OPTIMIZATION-STATUS.md`** for exact functionality, limitations, validation and next implementation work.

## Installation on Ubuntu (GNOME Wayland)

### Option A: Standard Ubuntu Package (.deb via apt) — Recommended
Build a single, clean `.deb` package that bundles all components:

```bash
# 1. Build the Debian package (developer / packaging)
./scripts/build-deb.sh

# 2. Install single package on any Ubuntu system via apt
sudo apt install ./target/lamm_0.2.1_amd64.deb

# 3. Clean uninstall anytime via apt
sudo apt remove lamm
```

### Option B: Local User Install (without root)
```bash
# Build and install for current user only
./install.sh

# Uninstall
./scripts/uninstall-user.sh
```

### Verify Service and Status
```bash
systemctl --user status lamm-daemon
journalctl --user -u lamm-daemon -f
lammctl
```

Initial settings are experimental and may not change properties on distributions that restrict systemd app scopes. For uninstall see `scripts/uninstall-user.sh`. Check changed unit property state and reset it manually if needed; runtime changes do not always disappear merely by stopping the daemon.

This is a **source prototype, not a production-ready system optimizer**. Not compiled/tested in the artifact generation environment. Application/tab optimization is **not implemented** yet. The dashboard is inherited from the original starter and is not a priority.
