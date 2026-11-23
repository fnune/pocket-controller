#!/usr/bin/env bash
set -euo pipefail

host="${POCKET_CONTROLLER_HOST:-root@192.168.1.92}"
key="${POCKET_CONTROLLER_SSH_KEY:-$HOME/.ssh/id_ed25519_brick}"
mode="${1:-xbox}"
seconds="${2:-4}"
ssh_options=(-i "$key" -o BatchMode=yes -o IdentitiesOnly=yes -o LogLevel=ERROR)
release="target/aarch64-unknown-linux-musl/release"

scp -q "${ssh_options[@]}" "$release/pocket-controller" "$release/pocket-controller-inject" "$host:/tmp/"

ssh "${ssh_options[@]}" "$host" sh -s -- "$mode" "$seconds" <<'REMOTE'
mode="$1"
seconds="$2"
gadgets=/sys/kernel/config/usb_gadget
describe() {
    for gadget in "$gadgets"/*; do
        echo "  $(basename "$gadget"): UDC=[$(cat "$gadget/UDC")] id=$(cat "$gadget/idVendor"):$(cat "$gadget/idProduct") product=[$(cat "$gadget/strings/0x409/product")] links=[$(ls "$gadget/configs/c.1" | grep '\.' | tr '\n' ' ')]"
    done
    echo "  udc state: $(cat /sys/class/udc/*/state)"
}
echo "before:"; describe
/tmp/pocket-controller --mode "$mode" >/tmp/pocket-controller.log 2>&1 &
pid=$!
sleep "$seconds"
echo "during:"; describe
echo "  endpoints: $(ls /tmp/pocket-controller-functionfs 2>/dev/null | tr '\n' ' ')"
kill -TERM "$pid"
sleep 2
if kill -0 "$pid" 2>/dev/null; then echo "STILL RUNNING"; kill -KILL "$pid"; fi
echo "after:"; describe
echo "  marker: $(test -e /tmp/pocket-controller.port && echo present || echo gone)"
echo "  functionfs mounts: $(grep functionfs /proc/mounts | cut -d' ' -f1-2 | tr '\n' ' ')"
echo "log:"; sed 's/^/  /' /tmp/pocket-controller.log
REMOTE
