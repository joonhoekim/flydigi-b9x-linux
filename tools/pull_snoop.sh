#!/bin/sh
# Copy the tablet's HCI snoop log into captures/ and print the ATT traffic.
# usage: tools/pull_snoop.sh [serial]   (default: first adb device)
set -e
cd "$(dirname "$0")/.."
s=${1:+-s $1}
out=captures/btsnoop_$(date +%Y%m%d_%H%M%S).log
adb $s shell 'su -c "cat /data/misc/bluetooth/logs/btsnoop_hci.log"' > "$out"
echo "saved $out"
python3 -I tools/btsnoop_att.py "$out"
