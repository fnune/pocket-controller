#!/usr/bin/env bash
set -euo pipefail
font=$1
out=docs/screenshots
helper=target/release/pocket-controller-screen
frames=$(mktemp -d)
trap 'rm -rf "$frames"' EXIT

names=()
script=""
shot() {
    names+=("$1")
    shift
    script+=$(printf '%s\n' "$@" end)$'\n'
}
shot connected view=connected kind=xbox live=8201,0,0,0,0,0 live-turbo=b,rb live-dpad=dpad
shot analog view=connected kind=xbox live=9,0,0,0,0,0 live-turbo=b live-dpad=left-stick
shot shortcuts view=connected kind=xbox live=0,0,0,0,0,0 live-turbo=b,rb live-dpad=left-stick sheet=1 turbo=b,rb dpad=left-stick "note=Turbo A: on"
shot settings view=connected kind=xbox settings=face,label,1,dim,gamepad,xbox,xbox,1
shot debug view=connected kind=xbox live=8193,0,0,0,0,0 live-turbo= live-dpad=dpad debug=configured,xbox,1,one-interrupt,1,128,4097,0,0,-32767,0,0,0
shot waiting view=waiting mode=automatic diagnosis=none

printf '%s' "$script" | SDL_VIDEODRIVER=offscreen LD_LIBRARY_PATH="$SDL_LIBRARY_PATH" "$helper" \
    --font "$font" --assets screen/assets --device trimui-brick --size 1024x768 --snapshots "$frames"
mkdir -p "$out"
for index in "${!names[@]}"; do
    magick "$frames/$((index + 1)).bmp" -strip "$out/${names[$index]}.png"
done
ls "$out"
