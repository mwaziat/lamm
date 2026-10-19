#!/usr/bin/env bash
set -euo pipefail

# LAMM ZRAM & System Tuning Teardown
# Safely removes all system configurations and restores Ubuntu defaults.

if [ "$(id -u)" -ne 0 ]; then
    echo "Error: This script must be run as root or with sudo:"
    echo "  sudo $0"
    exit 1
fi

echo "=== Removing LAMM ZRAM & System Paging Configurations ==="

# 1. Stop and disable lamm-zram service
if systemctl is-active --quiet lamm-zram.service 2>/dev/null; then
    echo "--> Stopping lamm-zram service..."
    systemctl stop lamm-zram.service || true
fi
if systemctl is-enabled --quiet lamm-zram.service 2>/dev/null; then
    echo "--> Disabling lamm-zram service..."
    systemctl disable lamm-zram.service || true
fi

# 2. Deactivate zram swap device
if swapon --show | grep -q "/dev/zram0"; then
    echo "--> Deactivating /dev/zram0 swap..."
    swapoff /dev/zram0 2>/dev/null || true
fi
if [ -f /sys/block/zram0/reset ]; then
    echo 1 > /sys/block/zram0/reset 2>/dev/null || true
fi

# 3. Remove systemd unit and helper
rm -f /etc/systemd/system/lamm-zram.service
rm -f /usr/local/bin/lamm-zram-helper
rm -f /etc/default/lamm-zram
systemctl daemon-reload

# 4. Remove sysctl and restore backup if present
echo "--> Restoring default kernel virtual memory parameters..."
rm -f /etc/sysctl.d/99-lamm-vm.conf
if [ -f /etc/sysctl.conf.lamm.bak ]; then
    mv -f /etc/sysctl.conf.lamm.bak /etc/sysctl.conf
elif grep -q "legacy overridden by lamm: vm.swappiness" /etc/sysctl.conf 2>/dev/null; then
    sed -i 's/# legacy overridden by lamm: //g' /etc/sysctl.conf
fi
sysctl --system -q || true

# 5. Remove systemd-oomd tuning and restore default
rm -f /etc/systemd/oomd.conf.d/99-lamm.conf
if systemctl is-active --quiet systemd-oomd 2>/dev/null; then
    systemctl restart systemd-oomd 2>/dev/null || true
fi

echo "=== System components cleanly removed. Ubuntu defaults restored. ==="

