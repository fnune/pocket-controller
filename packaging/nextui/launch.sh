#!/bin/sh
PAK_DIR="$(dirname "$0")"
PAK_NAME="$(basename "$PAK_DIR")"
PAK_NAME="${PAK_NAME%.*}"

mkdir -p "$LOGS_PATH"
exec >"$LOGS_PATH/$PAK_NAME.txt" 2>&1

if [ "$PLATFORM" = "tg3040" ] && [ -z "$DEVICE" ]; then
    export DEVICE="brick"
    export PLATFORM="tg5040"
fi

presenter="$PAK_DIR/bin/$PLATFORM/minui-presenter"
screen_helper="$PAK_DIR/bin/arm64-gnu/pocket-controller-screen"
font="$SDCARD_PATH/.system/res/font1.ttf"
state_dir="$USERDATA_PATH/$PAK_NAME"

set -- --state-dir "$state_dir" --dim
if [ -x "$screen_helper" ] && [ -f "$font" ]; then
    set -- "$@" --screen "$screen_helper" --font "$font" --assets "$PAK_DIR/assets"
fi
if [ -x "$presenter" ]; then
    set -- "$@" --presenter "$presenter"
fi

echo 1 >/tmp/stay_awake
"$PAK_DIR/bin/arm64/pocket-controller" "$@"
status=$?
"$PAK_DIR/bin/arm64/pocket-controller" recover
rm -f /tmp/stay_awake

if [ "$status" -ne 0 ] && [ -x "$presenter" ]; then
    "$presenter" --message "Pocket Controller stopped\n\nThe log is in $PAK_NAME.txt in the logs folder" --timeout 4
fi
exit "$status"
