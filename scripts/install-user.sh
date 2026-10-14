#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
if ! command -v cargo >/dev/null 2>&1; then
  echo 'Missing cargo. Install rustup or Ubuntu rustc/cargo.' >&2
  exit 1
fi
cargo build --release --workspace
install -d "$HOME/.local/bin" "$HOME/.config/systemd/user" "$HOME/.local/share/applications"
for executable in lamm-daemon lammctl lamm-ui; do
  install -m 755 "target/release/$executable" "$HOME/.local/bin/$executable"
done
install -m 644 packaging/lamm-daemon.service "$HOME/.config/systemd/user/lamm-daemon.service"
install -m 644 packaging/org.lamm.Desktop.desktop "$HOME/.local/share/applications/org.lamm.Desktop.desktop"
systemctl --user daemon-reload
systemctl --user enable --now lamm-daemon.service
printf '\nInstalled. Use ~/.local/bin/lammctl or launch LAMM from applications.\n'
