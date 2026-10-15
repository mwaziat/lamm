# LAMM v0.2.4 — Dynamic Percentage-Based Memory Policy

## What is implemented

The GNOME 50 bridge runs inside GNOME Shell, publishing window PID and activity (`ACTIVE`, `VISIBLE`, `BACKGROUND`) to `$XDG_RUNTIME_DIR/lamm-activity.csv`.

The Rust daemon monitors window activity and dynamically scales memory reclaim proportional to each application's footprint:

| State | Condition | CPUWeight | MemoryLow | IOWeight | Dynamic Cache Reclaim | Cooldown |
|---|---|---:|---:|---:|---:|---|
| **ACTIVE** | Focused window | 400 | 256M | 400 | **~6%** (max 300 MB) | 5 minutes |
| **VISIBLE** | Split-screen / visible | 250 | 128M | 250 | **~6%** (max 300 MB) | 5 minutes |
| **BACKGROUND** | Obscured / minimized (< 2 min) | 80 | 0 | 100 | **~20%** (max 600 MB) | 60 seconds |
| **IDLE** | Inactive in background (>= 2 min) | 50 | 0 | 50 | **~35% - 50%** (max 2500 MB) | 60 seconds |

### Proportional Impact Examples (from real usage)
- **Chrome (~1.86 GB)**:
  - Background: 20% = **~370 MB** reclaimed
  - Idle: 35% = **~650 MB** reclaimed (exceeds the 500 MB target safely)
- **VS Code (~10.04 GB)**:
  - Background: 20% (capped at 600 MB) = **~600 MB** cold file cache reclaimed
  - Idle: 35% = **~2.5 GB** cold file cache reclaimed safely without starving language servers
- **Firefox (~600 MB)**:
  - Background: 20% = **~120 MB** reclaimed
  - Idle: 35% = **~210 MB** reclaimed
- **Terminal Dev Servers (Next.js / Turbopack / Vite in Ptyxis or GNOME Terminal)**:
  - Managed via `ptyxis-spawn-*.scope` & `vte-spawn-*.scope`
  - Reclaims **~200 MB to 1.2 GB** of cold inactive page cache without touching Node.js V8 JS heap or disrupting Hot Module Replacement (HMR)
- **Small Apps (< 80 MB)**: Skipped to preserve stability

## Verification & Testing

```bash
# 1. Update and reinstall
./install.sh

# 2. Monitor real-time reclaim metrics
journalctl --user -u lamm-daemon -f
```
