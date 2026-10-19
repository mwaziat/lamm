#!/usr/bin/env bash
set -euo pipefail

# LAMM ZRAM & Fast Paging Setup Script
# Configures high-speed compressed in-memory swap (ZRAM) and optimizes kernel VM paging.

if [ "$(id -u)" -ne 0 ]; then
    echo "Error: This script must be run as root or with sudo:"
    echo "  sudo $0"
    exit 1
fi

echo "=== LAMM ZRAM & Fast Paging Accelerator Setup ==="

# 1. Determine optimal ZRAM size based on total RAM
TOTAL_MEM_KB=$(grep MemTotal /proc/meminfo | awk '{print $2}')
TOTAL_MEM_GB=$(( TOTAL_MEM_KB / 1024 / 1024 ))

if [ "$TOTAL_MEM_GB" -ge 12 ]; then
    ZRAM_SIZE="10G"
elif [ "$TOTAL_MEM_GB" -ge 8 ]; then
    ZRAM_SIZE="6G"
else
    ZRAM_SIZE="4G"
fi

echo "--> Detected ${TOTAL_MEM_GB} GB RAM. Target ZRAM size: ${ZRAM_SIZE}"

# 2. Helper script for persistent boot service
HELPER_PATH="/usr/local/bin/lamm-zram-helper"
cat << 'EOF' > "$HELPER_PATH"
#!/usr/bin/env bash
set -euo pipefail

ACTION="${1:-start}"
ZRAM_DEV="/dev/zram0"
CONFIG_FILE="/etc/default/lamm-zram"

if [ -f "$CONFIG_FILE" ]; then
    # shellcheck disable=SC1090
    source "$CONFIG_FILE" 2>/dev/null || true
fi

get_auto_size() {
    TOTAL_MEM_KB=$(grep MemTotal /proc/meminfo | awk '{print $2}')
    TOTAL_MEM_GB=$(( TOTAL_MEM_KB / 1024 / 1024 ))
    if [ "$TOTAL_MEM_GB" -ge 12 ]; then
        echo "10G"
    elif [ "$TOTAL_MEM_GB" -ge 8 ]; then
        echo "6G"
    else
        echo "4G"
    fi
}

case "$ACTION" in
    start)
        if swapon --show | grep -q "$ZRAM_DEV"; then
            echo "ZRAM swap is already active on $ZRAM_DEV"
            exit 0
        fi

        modprobe zram num_devices=1 2>/dev/null || true

        if [ -f /sys/block/zram0/reset ]; then
            swapoff "$ZRAM_DEV" 2>/dev/null || true
            echo 1 > /sys/block/zram0/reset 2>/dev/null || true
        fi

        TARGET_ALGO="${COMP_ALGORITHM:-lz4}"
        if grep -q "$TARGET_ALGO" /sys/block/zram0/comp_algorithm 2>/dev/null; then
            echo "$TARGET_ALGO" > /sys/block/zram0/comp_algorithm
        elif grep -q "lz4" /sys/block/zram0/comp_algorithm 2>/dev/null; then
            echo lz4 > /sys/block/zram0/comp_algorithm
        fi

        SIZE="${ZRAM_SIZE:-auto}"
        if [ "$SIZE" = "auto" ] || [ -z "$SIZE" ]; then
            SIZE=$(get_auto_size)
        fi

        echo "$SIZE" > /sys/block/zram0/disksize
        mkswap -L lamm-zram "$ZRAM_DEV" >/dev/null
        swapon -p 100 "$ZRAM_DEV"

        SWAP_VAL="${SWAPPINESS:-120}"
        sysctl -w vm.swappiness="$SWAP_VAL" vm.page-cluster=0 vm.vfs_cache_pressure=50 >/dev/null 2>&1 || true
        echo "LAMM: ZRAM ($SIZE) initialized with priority 100 on $ZRAM_DEV"
        ;;
    stop)
        if swapon --show | grep -q "$ZRAM_DEV"; then
            swapoff "$ZRAM_DEV"
        fi
        if [ -f /sys/block/zram0/reset ]; then
            echo 1 > /sys/block/zram0/reset
        fi
        echo "LAMM: ZRAM stopped."
        ;;
    apply)
        NEW_SIZE="${2:-auto}"
        NEW_SWAPPINESS="${3:-120}"
        NEW_ALGO="${4:-lz4}"

        mkdir -p /etc/default
        cat << CONF > "$CONFIG_FILE"
ZRAM_SIZE="$NEW_SIZE"
SWAPPINESS=$NEW_SWAPPINESS
COMP_ALGORITHM="$NEW_ALGO"
CONF

        mkdir -p /etc/sysctl.d
        cat << SYSCTL > /etc/sysctl.d/99-lamm-vm.conf
vm.swappiness = $NEW_SWAPPINESS
vm.page-cluster = 0
vm.vfs_cache_pressure = 50
SYSCTL

        if swapon --show | grep -q "$ZRAM_DEV"; then
            swapoff "$ZRAM_DEV" 2>/dev/null || true
        fi
        if [ -f /sys/block/zram0/reset ]; then
            echo 1 > /sys/block/zram0/reset 2>/dev/null || true
        fi

        modprobe zram num_devices=1 2>/dev/null || true

        if grep -q "$NEW_ALGO" /sys/block/zram0/comp_algorithm 2>/dev/null; then
            echo "$NEW_ALGO" > /sys/block/zram0/comp_algorithm
        fi

        ACTUAL_SIZE="$NEW_SIZE"
        if [ "$ACTUAL_SIZE" = "auto" ] || [ -z "$ACTUAL_SIZE" ]; then
            ACTUAL_SIZE=$(get_auto_size)
        fi

        echo "$ACTUAL_SIZE" > /sys/block/zram0/disksize
        mkswap -L lamm-zram "$ZRAM_DEV" >/dev/null
        swapon -p 100 "$ZRAM_DEV"
        sysctl -w vm.swappiness="$NEW_SWAPPINESS" vm.page-cluster=0 vm.vfs_cache_pressure=50 >/dev/null

        echo "LAMM: ZRAM ($ACTUAL_SIZE) active with swappiness $NEW_SWAPPINESS and algo $NEW_ALGO."
        ;;
    status)
        if swapon --show | grep -q "$ZRAM_DEV"; then
            DISKSIZE=$(cat /sys/block/zram0/disksize 2>/dev/null || echo 0)
            echo "active: true, disksize: $DISKSIZE"
        else
            echo "active: false"
        fi
        ;;
    *)
        echo "Usage: $0 {start|stop|apply [size] [swappiness] [algo]|status}"
        exit 1
        ;;
esac
EOF
chmod 755 "$HELPER_PATH"

# 3. Create persistent systemd unit for ZRAM
echo "--> Installing systemd unit: lamm-zram.service..."
cat << EOF > /etc/systemd/system/lamm-zram.service
[Unit]
Description=LAMM Compressed ZRAM & Fast Paging Accelerator
DefaultDependencies=no
After=local-fs.target
Before=swap.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=${HELPER_PATH} start
ExecStop=${HELPER_PATH} stop

[Install]
WantedBy=swap.target
EOF

systemctl daemon-reload
systemctl enable --now lamm-zram.service

# 4. Tune kernel virtual memory parameters (Sysctl)
echo "--> Optimizing kernel virtual memory paging (/etc/sysctl.d/99-lamm-vm.conf)..."
cat << 'EOF' > /etc/sysctl.d/99-lamm-vm.conf
# LAMM Virtual Memory & Paging Optimizations
# vm.swappiness: Eagerly page cold inactive background apps to compressed ZRAM (RAM stays free for active app)
vm.swappiness = 120
# vm.page-cluster = 0: Read 1 page per fault on ZRAM with 0 latency (prevents over-reading)
vm.page-cluster = 0
# vm.vfs_cache_pressure = 50: Protect file & directory metadata caches from thrashing
vm.vfs_cache_pressure = 50
EOF

# Disable any legacy vm.swappiness in /etc/sysctl.conf that might override /etc/sysctl.d/
if grep -q "^vm\.swappiness" /etc/sysctl.conf 2>/dev/null; then
    echo "--> Overriding legacy 'vm.swappiness' in /etc/sysctl.conf (backup saved to /etc/sysctl.conf.lamm.bak)..."
    [ ! -f /etc/sysctl.conf.lamm.bak ] && cp /etc/sysctl.conf /etc/sysctl.conf.lamm.bak
    sed -i 's/^vm\.swappiness/# legacy overridden by lamm: vm.swappiness/g' /etc/sysctl.conf
fi

sysctl --system -q || sysctl -p /etc/sysctl.d/99-lamm-vm.conf
# Explicitly force parameters into current running kernel
sysctl -w vm.swappiness=120 vm.page-cluster=0 vm.vfs_cache_pressure=50

# 5. Tame systemd-oomd so it doesn't prematurely force-close heavy apps
echo "--> Tuning systemd-oomd thresholds..."
mkdir -p /etc/systemd/oomd.conf.d
cat << 'EOF' > /etc/systemd/oomd.conf.d/99-lamm.conf
[OOM]
# Allow higher pressure buffer before killing developer apps (Turbopack, Next.js, VS Code)
DefaultMemoryPressureLimit=85%
DefaultMemoryPressureDurationSec=30s
EOF

if systemctl is-active --quiet systemd-oomd; then
    systemctl restart systemd-oomd 2>/dev/null || true
fi

echo ""
echo "=== LAMM ZRAM & Fast Paging Configured Successfully ==="
swapon --show
echo ""
echo "Current sysctl:"
sysctl vm.swappiness vm.page-cluster vm.vfs_cache_pressure

