set -eu
PAK="/mnt/SDCARD/Tools/tg5040/Pocket Controller.pak"
OUT=/tmp/pocket-controller-screenshots
rm -rf "$OUT"; mkdir -p "$OUT"
FIFO=/tmp/pocket-controller-screenshots.fifo
rm -f "$FIFO"; mkfifo "$FIFO"
"$PAK/bin/arm64-gnu/pocket-controller-screen" --font /mnt/SDCARD/.system/res/font1.ttf --assets "$PAK/assets" --device trimui-brick < "$FIFO" > "$OUT/helper.log" 2>&1 &
HELPER=$!
exec 3> "$FIFO"
shot() {
    name=$1; shift
    printf '%s\n' "$@" end >&3
    sleep 1.2
    cat /sys/class/graphics/fb0/pan > "$OUT/$name.pan"
    dd if=/dev/fb0 of="$OUT/$name.raw" bs=4096 count=$((768 * 3)) 2>/dev/null
}
shot connected view=connected kind=xbox live=8201,0,0,0,0,0 live-turbo=b,rb live-dpad=dpad
shot analog view=connected kind=xbox live=9,0,0,0,0,0 live-turbo=b live-dpad=left-stick
shot shortcuts view=connected kind=xbox live=0,0,0,0,0,0 live-turbo=b,rb live-dpad=left-stick sheet=1 turbo=b,rb dpad=left-stick "note=Turbo A: on"
shot picker view=waiting mode=automatic diagnosis=none picker=xbox
shot waiting view=waiting mode=automatic diagnosis=none
exec 3>&-
sleep 1
kill "$HELPER" 2>/dev/null || true
rm -f "$FIFO"
ls "$OUT"
