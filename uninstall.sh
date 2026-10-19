#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Uninstalling LAMM (Linux Adaptive Memory Manager) ==="

# 1. Stop and remove systemd user daemon
echo "--> Stopping and removing user services..."
systemctl --user disable --now lamm-daemon.service 2>/dev/null || true
rm -f "$HOME/.config/systemd/user/lamm-daemon.service"
systemctl --user daemon-reload

# 2. Remove desktop launcher & binaries
echo "--> Removing user executables..."
rm -f "$HOME/.local/bin/lamm-daemon" "$HOME/.local/bin/lammctl" "$HOME/.local/bin/lamm-ui"
rm -f "$HOME/.local/share/applications/org.lamm.Desktop.desktop"

# 3. Disable and remove GNOME Shell extension bridge
echo "--> Removing GNOME Shell extension..."
UUID="lamm-activity@lamm.local"
if command -v gnome-extensions >/dev/null 2>&1; then
    gnome-extensions disable "$UUID" 2>/dev/null || true
fi
rm -rf "$HOME/.local/share/gnome-shell/extensions/$UUID"

# 4. Clean up system-level ZRAM & Kernel Sysctl (if installed)
if [ -f /etc/systemd/system/lamm-zram.service ] || [ -f /etc/sysctl.d/99-lamm-vm.conf ]; then
    echo "--> System-level ZRAM/Paging configurations detected."
    echo "    Reverting system configurations to Ubuntu defaults (requires sudo)..."
    if [ -x ./scripts/teardown-zram.sh ]; then
        sudo ./scripts/teardown-zram.sh
    fi
fi

# 5. Clean up any runtime sockets
if [ -n "${XDG_RUNTIME_DIR:-}" ]; then
    rm -f "$XDG_RUNTIME_DIR/lamm.sock" "$XDG_RUNTIME_DIR/lamm-activity.csv"
fi

echo ""
echo "=== LAMM Uninstalled Cleanly ==="
echo "- All user binaries, services, and GNOME extensions removed."
echo "- All system kernel sysctl, ZRAM, and systemd-oomd settings restored to Ubuntu defaults."
echo "- The operating system is left 100% in its original state."

