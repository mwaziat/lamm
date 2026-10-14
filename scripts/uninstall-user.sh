#!/usr/bin/env bash
set -euo pipefail
systemctl --user disable --now lamm-daemon.service 2>/dev/null || true
rm -f "$HOME/.config/systemd/user/lamm-daemon.service" "$HOME/.local/share/applications/org.lamm.Desktop.desktop"
rm -f "$HOME/.local/bin/lamm-daemon" "$HOME/.local/bin/lammctl" "$HOME/.local/bin/lamm-ui"
gnome-extensions disable lamm-activity@lamm.local 2>/dev/null || true
rm -rf "$HOME/.local/share/gnome-shell/extensions/lamm-activity@lamm.local"
systemctl --user daemon-reload
printf 'LAMM user installation removed. Source repository left unchanged.\n'
