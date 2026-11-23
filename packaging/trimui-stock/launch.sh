#!/bin/sh
APP_DIR="$(dirname "$0")"
cd "$APP_DIR" || exit 1
exec >"$APP_DIR/pocket-controller.log" 2>&1

set -- --state-dir "$APP_DIR/state" --dim
if [ -x "$APP_DIR/minui-presenter" ]; then
    set -- "$@" --presenter "$APP_DIR/minui-presenter"
fi
if [ -w /sys/class/gpio/gpio227/value ]; then
    set -- "$@" --motor /sys/class/gpio/gpio227/value
fi

"$APP_DIR/pocket-controller" "$@"
status=$?
"$APP_DIR/pocket-controller" recover
exit "$status"
