#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

echo "=== Building LAMM Debian (.deb) Package ==="

# 1. Check tools
if ! command -v dpkg-deb >/dev/null 2>&1; then
    echo "Error: 'dpkg-deb' is required to build Debian packages." >&2
    echo "Run: sudo apt install -y dpkg" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "Error: 'cargo' is required to compile release binaries." >&2
    exit 1
fi

# 2. Determine version and architecture
VERSION="$(grep '^Version:' packaging/debian/control | awk '{print $2}')"
ARCH="$(dpkg --print-architecture 2>/dev/null || echo "amd64")"
PACKAGE_NAME="lamm_${VERSION}_${ARCH}"
STAGING_DIR="target/deb-staging"

echo "Package:      lamm"
echo "Version:      $VERSION"
echo "Architecture: $ARCH"

# 3. Build release binaries
echo "--> Compiling release binaries with cargo..."
cargo build --release --workspace

# 4. Prepare staging directory
echo "--> Assembling package filesystem..."
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/DEBIAN"
mkdir -p "$STAGING_DIR/usr/bin"
mkdir -p "$STAGING_DIR/usr/lib/systemd/user"
mkdir -p "$STAGING_DIR/usr/share/applications"
mkdir -p "$STAGING_DIR/usr/share/gnome-shell/extensions/lamm-activity@lamm.local"

# Copy debian control & maintainer scripts
sed "s/^Architecture:.*/Architecture: $ARCH/" packaging/debian/control > "$STAGING_DIR/DEBIAN/control"
install -m 755 packaging/debian/postinst "$STAGING_DIR/DEBIAN/postinst"
install -m 755 packaging/debian/prerm "$STAGING_DIR/DEBIAN/prerm"
install -m 755 packaging/debian/postrm "$STAGING_DIR/DEBIAN/postrm"

# Copy binaries
install -m 755 target/release/lamm-daemon "$STAGING_DIR/usr/bin/lamm-daemon"
install -m 755 target/release/lammctl "$STAGING_DIR/usr/bin/lammctl"
install -m 755 target/release/lamm-ui "$STAGING_DIR/usr/bin/lamm-ui"

# Copy systemd user service (configured for /usr/bin)
sed 's|%h/.local/bin/lamm-daemon|/usr/bin/lamm-daemon|' packaging/lamm-daemon.service > "$STAGING_DIR/usr/lib/systemd/user/lamm-daemon.service"
chmod 644 "$STAGING_DIR/usr/lib/systemd/user/lamm-daemon.service"

# Copy desktop launcher
install -m 644 packaging/org.lamm.Desktop.desktop "$STAGING_DIR/usr/share/applications/org.lamm.Desktop.desktop"

# Copy GNOME extension files (system-wide)
install -m 644 gnome-extension/lamm-activity@lamm.local/metadata.json "$STAGING_DIR/usr/share/gnome-shell/extensions/lamm-activity@lamm.local/metadata.json"
install -m 644 gnome-extension/lamm-activity@lamm.local/extension.js "$STAGING_DIR/usr/share/gnome-shell/extensions/lamm-activity@lamm.local/extension.js"

# 5. Build .deb package
OUTPUT_DEB="target/${PACKAGE_NAME}.deb"
echo "--> Creating Debian package: $OUTPUT_DEB..."
dpkg-deb --build --root-owner-group "$STAGING_DIR" "$OUTPUT_DEB"

echo ""
echo "=== Build Succeeded! ==="
echo "Package file created at:"
echo "  $ROOT_DIR/$OUTPUT_DEB"
echo ""
echo "To test/install on Ubuntu via apt:"
echo "  sudo apt install ./$OUTPUT_DEB"
echo ""
echo "To uninstall cleanly:"
echo "  sudo apt remove lamm"

