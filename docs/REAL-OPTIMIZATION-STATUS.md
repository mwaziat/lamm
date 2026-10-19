# LAMM v0.3.0 — Universal Proportional Fast Paging & Anti-OOM Policy

## Universal Proportional Architecture

LAMM v0.3.0 applies a **100% universal proportional memory policy** across **all applications equally** (VS Code, Chrome, DBeaver, Blender, VMs, Terminals). There are **no hardcoded process names or fixed magic megabyte numbers**:

1. **Universal Anti-OOM Protection**: Whichever window is currently `Active` or `Visible` is granted `ManagedOOMPreference=avoid` to ensure `systemd-oomd` never terminates it under heavy workload pressure.
2. **Dynamic 50% Working-Set Protection**: Whichever application is focused receives:
   - `CPUWeight = 1000` & `IOWeight = 1000` (full-throttle scheduling priority).
   - `MemoryLow = 50%` of its current actual footprint (protected from kernel page reclaimer).
   - `Reclaim = 0 MB` (zero memory trimming while actively in use).
3. **Personal 50% Retention Floor**: When an application enters `Background` or `Idle`, LAMM tracks its peak active memory and guarantees it will **never be trimmed below 50% of its working set**:
   - **VS Code (~12 GB peak)**: Safe floor is **6.0 GB**.
   - **Chrome (~2.0 GB peak)**: Safe floor is **1.0 GB**.
   - **DBeaver (~600 MB peak)**: Safe floor is **300 MB**.
   - **Blender / VM (~16 GB peak)**: Safe floor is **8.0 GB**.
   - **Small Apps (< 150 MB)**: Completely untouched to preserve stability.
4. **Proactive Background Memory Relief**: When system available RAM drops below 1.5 GB, LAMM gently requests idle background applications to page cold cache out into ZRAM, giving immediate headroom for the active window to scale freely.
5. **Windows-Style Virtual RAM (ZRAM) & Fast Paging**: Automated ZRAM setup (`scripts/setup-zram.sh`) creates high-speed compressed in-memory swap (10 GB) with `lz4` compression, `swappiness=120`, and `page-cluster=0` for instantaneous sub-millisecond paging.

---

## Policy Matrix

| State | Condition | CPUWeight | MemoryLow Protection | MemoryHigh (Burst Damper) | IOWeight | Reclaim Policy | OOM Protection |
|---|---|---:|---:|---:|---:|---|---|
| **ACTIVE** | Focused window | **1000** | **50% of current usage** | **max** (unlimited) | **1000** | **0% (Never reclaimed)** | `avoid` (Immune) |
| **VISIBLE** | Split-screen / visible | **400** | **25% of current usage** | **max** (unlimited) | **400** | **0% (Never reclaimed)** | `avoid` (Immune) |
| **BACKGROUND** | Minimized (< 2 min) | **50** | **0** | **Soft Ceiling** (Peak + 1.5x / +1GB) | **50** | **25% of excess** above 50% floor | `none` |
| **IDLE** | Inactive in background (>= 2 min) | **20** | **0** | **Tight Ceiling** (Current + 512M) | **20** | **50% of excess** towards 50% floor | `none` |

---

## Installation & Setup

```bash
# 1. Install & configure everything (daemon, UI, GNOME bridge, 10 GB ZRAM & Kernel VM)
./install.sh

# 2. Verify real-time acceleration & anti-OOM logs
journalctl --user -u lamm-daemon -f

# 3. Clean uninstall (reverts 100% back to Ubuntu defaults whenever needed)
./uninstall.sh
```
