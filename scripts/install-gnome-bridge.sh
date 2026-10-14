#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
UUID=lamm-activity@lamm.local
DEST="$HOME/.local/share/gnome-shell/extensions/$UUID"
install -d "$DEST"
install -m 644 "gnome-extension/$UUID/metadata.json" "$DEST/metadata.json"
install -m 644 "gnome-extension/$UUID/extension.js" "$DEST/extension.js"
echo "GNOME bridge files installed at $DEST"
echo "Under Wayland, log out and back in, then: gnome-extensions enable $UUID"
