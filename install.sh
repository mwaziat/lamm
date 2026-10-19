#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Installing LAMM (Linux Adaptive Memory Manager) ==="

# 1. Dependency checks
MISSING_PKGS=()
for cmd in pkg-config; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        MISSING_PKGS+=("$cmd")
    fi
done

if ! command -v cargo >/dev/null 2>&1; then
    echo "Error: 'cargo' is not installed."
    echo "Install Rust via rustup: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

if [ ${#MISSING_PKGS[@]} -gt 0 ]; then
    echo "Missing build tools: ${MISSING_PKGS[*]}"
    echo "Run: sudo apt update && sudo apt install -y build-essential pkg-config libgtk-4-dev libadwaita-1-dev"
    exit 1
fi

# 2. Build release binaries
echo "--> Compiling release binaries..."
cargo build --release --workspace

# 3. Install binaries to user local bin
echo "--> Installing executables to ~/.local/bin..."
install -d "$HOME/.local/bin"
for bin in lamm-daemon lammctl lamm-ui; do
    install -m 755 "target/release/$bin" "$HOME/.local/bin/$bin"
done

# 4. Install systemd user service
echo "--> Configuring systemd user service..."
install -d "$HOME/.config/systemd/user"
install -m 644 packaging/lamm-daemon.service "$HOME/.config/systemd/user/lamm-daemon.service"
systemctl --user daemon-reload
systemctl --user enable lamm-daemon.service
systemctl --user restart lamm-daemon.service

# 5. Install desktop application launcher
install -d "$HOME/.local/share/applications"
install -m 644 packaging/org.lamm.Desktop.desktop "$HOME/.local/share/applications/org.lamm.Desktop.desktop"

# 6. Install GNOME Shell extension bridge
UUID="lamm-activity@lamm.local"
EXT_DIR="$HOME/.local/share/gnome-shell/extensions/$UUID"
echo "--> Installing GNOME extension bridge to $EXT_DIR..."
install -d "$EXT_DIR"
install -m 644 "gnome-extension/$UUID/metadata.json" "$EXT_DIR/metadata.json"
install -m 644 "gnome-extension/$UUID/extension.js" "$EXT_DIR/extension.js"

if command -v gnome-extensions >/dev/null 2>&1; then
    gnome-extensions enable "$UUID" 2>/dev/null || true
fi

# 7. ZRAM Virtual Memory & Fast Paging setup
echo "--> Checking ZRAM Fast Paging & Kernel VM status..."
CURRENT_SWAPPINESS=$(cat /proc/sys/vm/swappiness 2>/dev/null || echo 0)
if swapon --show | grep -q "/dev/zram0" && [ "$CURRENT_SWAPPINESS" -ge 100 ] && grep -q "apply)" /usr/local/bin/lamm-zram-helper 2>/dev/null; then
    echo "    [OK] ZRAM Virtual RAM is active and optimized (swappiness: ${CURRENT_SWAPPINESS})."
else
    echo "    Configuring ZRAM Virtual RAM & Fast Paging (requires sudo)..."
    if [ -x ./scripts/setup-zram.sh ]; then
        sudo ./scripts/setup-zram.sh
    fi
fi

echo ""
echo "=== LAMM Installation Complete ==="
echo "- Status:    systemctl --user status lamm-daemon"
echo "- CLI:       ~/.local/bin/lammctl"
echo "- GUI:       ~/.local/bin/lamm-ui (or launch 'LAMM' from application menu)"
echo "- Uninstall: ./uninstall.sh (safely restores 100% of Ubuntu defaults)"
echo "- Note:      If on Wayland and extension wasn't active, log out and back in once."

