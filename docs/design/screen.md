# Screen design

What pocket-controller shows on the handheld's screen and how it draws it. Written against the TrimUI Brick (1024x768, 4:3). Items marked **Unverified** were not tested on hardware or could not be confirmed from the sources read. Controls, chords and settings rows are owned by `controller-features.md`; this doc owns what is drawn.

## Decisions

- Render with a small helper, `pocket-controller-screen`, written in Rust, linked against nothing and loading the device's own `libSDL2` and `libSDL2_ttf` with `dlopen`. It is a second binary built for `aarch64-unknown-linux-gnu`, because the static musl `pocket-controller` cannot load glibc libraries. pocket-controller feeds it full state snapshots on stdin and keeps all input.
- Keep `minui-presenter` as the fallback renderer when the helper is missing or fails, and for the final message after a fatal exit in `launch.sh`.
- Do not write to `/dev/fb0` directly.
- Navigation while disconnected: A settings, X debug, Y mode, B quit, and MENU + START also opens settings. While connected, MENU + START opens a captured settings overlay (with a Debug row). No other on-screen navigation exists while connected.
- Holding MENU shows a chord cheat sheet for as long as it is held, so chords can be learned on the device. It is not a menu.
- The timed hold (the quit chord only) shows a progress bar with what will happen, on the waiting and connected views and over a dimmed screen.
- While playing the screen dims after 30 s by default. `Off after 30 s` and `Always on` are options. The helper never draws black to save power; the backlight does it.
- A brightness bar shows for about 1 s on MENU plus volume, even with the overlay closed and on a dimmed screen.

## Facts from the sources

- `Screen` is `Waiting(Mode, Diagnosis)`, `Checking`, `Connected(Kind)`. This design removes `Checking` as a view (see "Settling indicator"). `Screen::text()` builds one `\n`-escaped string and `Presenter::show` kills and respawns `minui-presenter --message ... --timeout -1` on every `Action::Show`.
- `UsbLink` (`Unknown`, `NoPower`, `Powered`, `Enumerating`, `Configured`) is classified from the USB power_supply `online` value and the UDC state. `Diagnosis` is `None`, `NoPower`, `NoComputer` (powered, no host after 3 s), `SetupStuck` (enumerating, stuck after 3 s). `Input::Link(UsbLink)` feeds it. Verified in `src/control.rs`.
- "The Brick is the power source" cannot be detected (the AXP2202's Type-C logic is disabled on the Brick), so it looks like `NoPower`. The Brick also cannot tell "no cable" from "a cable with no power" (both read as zero volts), so both show the neutral waiting state.
- `src/system.rs` has `Backlight`, `read_brightness` (ioctl 0x103 on `/dev/disp`) and `write_brightness` (ioctl 0x102, `param[1]` 0 to 255), with the original level saved in a file for recovery. Verified on the Brick, which reads 255.
- Brick buttons, measured: three circles is MENU (`BTN_MODE`), minus is SELECT, play is START. Volume keys are on the grabbed pad device. The FN switch is `EV_SW` code 1, latching.
- This doc names buttons by their printed Nintendo label. `LOCAL_QUIT = xinput::A` is the printed B and `LOCAL_CHANGE_MODE = xinput::X` is the printed Y. The face layout swap acts only on the report.
- NextUI draws with SDL2 and an OpenGL renderer (`PLAT_initVideo`, `workspace/all/common/generic_video.c`). `show2` (`workspace/all/show2/show2.cpp`) opens a 0x0 SDL window, draws on `SDL_GetWindowSurface`, links `-lSDL2 -lSDL2_image -lSDL2_ttf`, and takes live `TEXT:`, `PROGRESS:`, `BGCOLOR:` commands on a FIFO.
- On the Brick (read-only over SSH, aarch64): glibc 2.33 (`/lib/libc-2.33.so`, `ld-2.33.so`), `libSDL2-2.0.so.0.3000.8` (SDL 2.30.8), `libSDL2_ttf-2.0.so.0.14.1` and `libfreetype.so.6` in `/usr/lib`. NextUI's own libraries (`libmsettings.so` and others) are in `/mnt/SDCARD/.system/tg5040/lib`. Which firmware generated this rootfs listing (stock or NextUI) does not matter for the libraries, they are in the base system.
- Screen size is fixed per device in `platform.h`: Brick 1024x768, Smart Pro 1280x720. H700 sizes are **Unverified**.
- The theme is `minuisettings.txt` in the shared userdata folder (`font=`, `color1=` to `color7=` as `0xRRGGBB`). Defaults in `config.h`: color1 `ffffff`, color2 `9b2257`, color3 `1e2329`, color4 `ffffff`, color5 `000000`, color6 `ffffff`, color7 `000000`. Colour roles are **Unverified**; read `config.c` first.
- `minui-presenter` (README): `--message` or `--file` JSON `items`, `--timeout -1` ignores buttons, SIGUSR1 advances items, no live update. `-nextui` builds use theme colours and font. `minui-list` reads input itself and writes the selection to stdout.

## Rendering options

| | minui-presenter respawn | minui-list | SDL2 helper (recommended) | Raw `/dev/fb0` |
| --- | --- | --- | --- | --- |
| Looks like NextUI | Yes, `-nextui` builds | Yes | Yes if it reads `minuisettings.txt` | Only with our own fonts and shapes |
| Live data (debug view, brightness bar) | No | No | Yes | Yes |
| Flicker | Yes, process restart | Yes | No, one window for the run | Tearing likely, no vsync |
| Input | Reads input itself, clashes with our grab | Same | None, pocket-controller keeps input | None |
| Effort | Done | Needs grab handover | Medium: about 600 lines plus a protocol | Medium plus font rendering |
| Binary size | Not measured | Not measured | Under 300 KB stripped, **Unverified** | Smallest |

Raw framebuffer: NextUI presents through an SDL2 OpenGL context, so `/dev/fb0` may not be the scanout on these devices. NextUI clears it at poweroff (`cat /dev/zero > /dev/fb0`), which shows it exists, not that writes show. **Unverified.** Rejected.

Settings through `minui-list` is rejected: it reads buttons itself, so pocket-controller would release and re-grab evdev around it, and it cannot draw the captured overlay on top of the connected view.

Interim with no new binary: `show2 --mode=daemon` and `TEXT:` over its FIFO stops the flicker for static text. Its presence on every NextUI version is **Unverified**.

## Helper language: Rust with dlopen against C++

The helper needs about 15 SDL calls: `SDL_Init`, `SDL_CreateWindow`, `SDL_GetWindowSize`, `SDL_CreateRenderer` (software), `SDL_SetRenderDrawColor`, `SDL_RenderClear`, `SDL_RenderFillRect`, `SDL_RenderPresent`, `TTF_Init`, `TTF_OpenFont`, `TTF_SizeUTF8`, `TTF_RenderUTF8_Blended`, `SDL_CreateTextureFromSurface`, `SDL_RenderCopy`, `SDL_FreeSurface`, `SDL_DestroyTexture`. Only `SDL_Rect` and `SDL_Color` need a known layout, everything else is an opaque pointer, so the hand-written FFI stays small and does not depend on the `SDL_Surface` layout.

| | Rust, `dlopen` | C++ against the union toolchain |
| --- | --- | --- |
| Build | `cargo` for `aarch64-unknown-linux-gnu` with a glibc baseline, see below | NextUI's Docker image, one build per platform |
| Dependencies | `libc` only, loads `libSDL2-2.0.so.0` and `libSDL2_ttf-2.0.so.0` at run time | Headers and link libraries from the toolchain sysroot |
| Pure logic and tests | Layout, hints and snapshot parsing are plain Rust with `cargo test`, same as the rest | Needs its own test setup |
| Unsafe code | One module of `extern "C"` types and `dlsym` lookups, wrapped once | None, but no type checks across the protocol |
| ABI risk | SDL2 and SDL2_ttf function signatures are stable across 2.x, and the Brick's 2.30.8 and 2.0.14 are old enough to be a floor | Linked against the sysroot's SDL2, which matches the device by construction |
| Same repository, same `just check` | Yes | Docker step outside it |
| Where it runs | Anywhere with glibc at or above the baseline and SDL2 on the library path | Same |

Why not the static musl `pocket-controller` binary itself: musl cannot `dlopen` glibc shared objects, and the device libraries are glibc 2.33. So the helper is a separate dynamically linked glibc binary, and the SDL libraries stay the device's own (no copies shipped, no version skew with NextUI).

Recommendation: Rust with `dlopen`. It keeps the toolchain, tests, lints and review rules of the project (`just check`, `clippy -D warnings`), and it does not make the build depend on NextUI's Docker image. C++ remains the fallback if the Rust build story below fails on a target.

Build story:

- Target `aarch64-unknown-linux-gnu` against a glibc baseline no higher than 2.33 (Brick). The Rust standard library needs glibc 2.17 or newer, so a lower baseline such as 2.28 is possible and widens the range of devices. Two ways to link against an old glibc, neither tried here: `cargo zigbuild --target aarch64-unknown-linux-gnu.2.28` (needs `zig` and `cargo-zigbuild` in the dev shell), or a Nix cross sysroot with an old glibc. **Unverified**: that either works with `rust-toolchain.toml` and `nix develop`.
- Ship it in the pak as `bin/arm64-gnu/pocket-controller-screen` next to the static `bin/arm64/pocket-controller`. `launch.sh` passes `--screen PATH` when it exists and is executable, as it does for `--presenter` today.
- Same binary for tg5040 and tg5050 (both aarch64) if their glibc is at or above the baseline. H700 devices on NextUI are aarch64 too but their glibc and SDL2 versions are **Unverified**. Stock TrimUI firmware: the Brick's rootfs listing above shows SDL2 and SDL2_ttf in the base system, so the same binary likely works; **Unverified** on a stock install.
- Fallback when `dlopen` fails, the window cannot open or fonts are missing: the helper exits non-zero and pocket-controller falls back to `minui-presenter`.
- Fonts and colours: read `font=` and `color1` to `color7` from `minuisettings.txt`, find the TTF from the NextUI font id. The font location is **Unverified**. Without it, use the presenter's default font path or a bundled TTF (adds size).
- Window: open with `SDL_WINDOW_SHOWN` and size 0x0 as `show2` does, then take the real size from `SDL_GetWindowSize`. Whether a software renderer or `SDL_GetWindowSurface` is the better path on the PowerVR devices is **Unverified**; try the renderer first because it needs no surface layout.

## Protocol between pocket-controller and the helper

Snapshots, not commands. One line per snapshot, `key=value` pairs separated by tabs, newline terminated. Absent keys mean not shown.

- `view=waiting|connected|debug|error|off`
- `busy=0|1|2` (the highlighted dot of the settling indicator, absent when settled)
- `diag=none|no_computer|setup_stuck`
- `mode=automatic|xbox|gamepad`, `kind=xbox|gamepad`
- `banner=no_xbox_driver` (transient, set by pocket-controller for 3 s)
- `hints=B:Quit,Y:Mode,...` (button and label pairs)
- `sheet=0|1`, `sheet_note=Turbo A: on` (the cheat sheet, and the last chord it marked)
- `hold=quit`, `hold_pct=0..100` (a timed hold in progress, absent otherwise)
- `overlay=0|1`, `cursor=N`, `rows=label:type:value:grey_reason;...` where type is `choice`, `toggle` or `action`
- `dpad_mode=dpad|left|right`, `turbo=A,R1,...`
- `screen_level=on|dim|off`, `brightness=0..255`, `brightness_bar=1` (while the 1 s bar is due)
- Debug: `udc`, `layout`, `held`, `report`, `rumble`, `led`, `errors`
- `error=startup|stuck` with `detail=...`

Helper behaviour: redraw when a line differs from the last, at most 30 times a second. Exit when stdin closes. If it cannot open the display it exits non-zero. It never reads buttons. `screen_level` tells it what the backlight is doing so it does not draw a bright full screen while dimmed beyond what is needed; pocket-controller drives the backlight itself through `Backlight`.

## Visual language

- Theme background, title top left in the theme font at NextUI's large size, content centred, hints in a bottom row.
- Hint pills follow NextUI: a filled circle with the button letter and a short uppercase label (`(B) QUIT`). A hint shows only if the button does something on that view.
- Sizes derive from the screen height, not fixed pixels. NextUI's `FIXED_SCALE` is 3 on the Brick and 2 on the Smart Pro.
- One accent colour from the theme for the current state. Errors use the same layout with a stronger accent. Meaning is never carried by colour alone.
- No animation. The helper draws only filled rectangles, filled circles and text; see "Drawing primitives".
- Copy is short, says what to do next, never blames the user.

## Screens

Mockups are 64 columns by 24 rows, 4:3 with a 1:2 character cell.

### Waiting for a USB connection

Shown when not connected. Three states, from `Diagnosis`. The graphic carries the state on its own, with no small labels. Reading order: graphic, headline, one instruction, then a secondary mode area.

```
+--------------------------------------------------------------+
| Pocket Controller                                               |
|                                                              |
|    +-------+                                                 |
|    | Brick |                                                 |
|    +--+----+                              +-----------+      |
|       |                                   |  [ ]      |      |
|       +-------------+  [plug]  <gap>      | USB-A     |      |
|                                           +-----------+      |
|                                                              |
|              (o) Not connected                               |
|                                                              |
|     Plug the bottom USB-C port into a USB-A port             |
|                                                              |
|                                                              |
|                                                              |
|     Mode: Automatic                                          |
|     Xbox 360 when supported, otherwise a generic gamepad     |
|                                                              |
|  (A) SETTINGS   (X) DEBUG   (Y) MODE                (B) QUIT |
+--------------------------------------------------------------+
```

| `Diagnosis` | Graphic | Icon | Headline | Instruction |
| --- | --- | --- | --- | --- |
| `None` | Cable plug stops short of the host socket, cable grey | Neutral circle with three dots | Not connected | Plug the bottom USB-C port into a USB-A port |
| `NoComputer` | Plug inside the socket, cable white, an amber circle with "?" on the host | Amber circle with "!" | Connected, but not recognised | Try a cable that carries data, or another USB-A port |
| `SetupStuck` | Plug inside the socket, cable white, three amber dots above the cable | Amber circle with "!" | USB setup did not finish | Try another USB-A port |

- The Brick cannot tell "no cable" from "a cable with no power": both read as zero volts. There is no separate "No USB power" screen. `Diagnosis::None` covers both, and the instruction names the bottom port and USB-A, which rules out the USB-C to USB-C case where the Brick ends up as the power source. `UsbLink::NoPower` maps to the neutral state.
- Wording is set 4 of the options considered. It names the other end by its port ("USB-A port") and never by a device type, since it can be a computer, console, phone, tablet or TV box. The alternatives were "computer", "host" and "the other device". They were rejected because they exclude phones and consoles, are jargon, or clash with the Brick being "the device" in the README.
- The bottom tip is dropped. Its content is folded into the instruction.
- Mode area, secondary: a muted two-line block above the hints, smaller than the instruction. Line one is "Mode: <name>", line two is the explanation:
    - Automatic: Xbox 360 when supported, otherwise a generic gamepad
    - Xbox 360: For Steam and Linux
    - Generic gamepad: For Windows, macOS and other devices
- A banner "No Xbox driver found, using the generic gamepad" is shown for 3 s on the connected view after the automatic fallback. It is a banner, not a diagnosis.
- MENU + START also opens settings here.
- Idle exit after 10 minutes stays and needs no screen.

#### Drawing primitives

The helper draws only filled rectangles, filled circles and text, so every graphic is built from them. No curves, gradients or animation. Solid colours only, all from the theme: text white, bar dark (`1e2329`), a mid grey, the hint pink, and one amber for warnings (green and red for the connected and error icons, **Unverified** whether the theme has them).

- Brick: a white rounded rectangle (a rectangle plus four circles at the corners, or two overlapping rectangles and four circles) with a black rectangle inset for the screen and a small dark rectangle for the bottom port.
- Cable: a vertical rectangle from the port, then a horizontal rectangle. Plug: a rectangle with two small dark rectangles for contacts. Loose and plugged states differ only by the plug's x position and the cable colour (grey or white).
- Host: a rectangle in a lighter dark grey with a black rectangle for the socket slot.
- Not recognised marker: an amber filled circle with the text "?".
- Setup stuck marker: three small amber filled circles in a row, static. They advance one step per second by redrawing, not by animation (optional).
- Status icons: a filled circle with a glyph. Waiting: a grey ring (two circles) with three small circles. Warning: an amber circle with "!" (a rectangle and a small circle). Connected: a green circle with a check made from a chain of small circles. Error: a red circle with a text "x".
- Hint pills: a filled circle with a letter, as in NextUI.

### Settling indicator

A state screen changes only once the new state is settled, meaning it will stay unless the user does something. While the link is in flux the previous screen stays and a small loading indicator is added. No intermediate state is shown.

- In flux: power present but no host yet, USB setup in progress, and the 2 s wait for an Xbox driver after the host enables the function.
- Settled, so immediate: unplugging, a confirmed connection, a diagnosis after its 3 s.
- The separate "checking what the computer supports" screen is removed. That wait is the indicator over the previous screen, then Connected.
- Indicator: three small filled circles in a row, one highlighted in turn (the highlight is a brighter or accent circle, the other two grey). pocket-controller advances the highlight every 100 ms and sends it as `busy=0|1|2`. Absent when nothing is in flux. It sits right of the headline, next to the status icon, on the waiting view, and in the same place on the connected view while the Xbox driver wait runs.
- Decomposition: three circles of equal size on one row. The highlight is a colour change, not a shape change. No animation in the helper: each snapshot is one frame.
- Redraw cost: 10 snapshots a second while it is active, fewer than the 30 a second cap. No indicator while dimmed or off (the screen change itself is the wake).

### Connected

```
+--------------------------------------------------------------+
| Pocket Controller                                      Player 1 |
|                                                              |
|                                                              |
|                      .----------.                            |
|                     /  (  )  (  )\                           |
|                    |    +      o  |                           |
|                     \____________/                           |
|                                                              |
|                Connected as an Xbox 360 controller           |
|                                                              |
|                Options  D-pad as left stick, turbo A B       |
|                Rumble  off                                   |
|                                                              |
|                                                              |
|                                                              |
|                                                              |
|                Hold MENU for shortcuts                       |
|                Hold SELECT + START for 2 s to quit           |
|                                                              |
|                                                              |
+--------------------------------------------------------------+
```

- Variant: "Connected as a generic gamepad". The Options line is hidden when D-pad mode and turbo are default. The Rumble line is hidden in generic gamepad mode.
- No hint bar, because every button goes to the computer. The two lines take its place.
- "Player N" comes from the Xbox LED report the computer sends. It is not the Brick's LED setting.
- A green check icon sits before the connected line. Status icons are drawn shapes (a circle with a check, a circle with an exclamation mark, a circle with a cross), built from circles, rectangles and text as listed under "Drawing primitives".

### Overlay

Opened by MENU + START, over the connected view. It captures all input, so the hint bar is true. The computer receives one neutral report on open. Closing: B, or MENU + START again. A MENU tap while it is open closes it and does not send Guide.

```
+--------------------------------------------------------------+
| Settings                                   Input paused      |
|                                                              |
|  > D-pad mode                                    D-pad       |
|    Face buttons                                  Position    |
|    Rumble                                        On          |
|    Screen while playing                          Dim 30 s    |
|    Brightness                                    [#######  ] |
|    Volume keys                       Xbox mode: not available|
|    LEDs                                          Off         |
|    Clear turbo                                               |
|    Debug                                                     |
|    Mode                           Automatic (offline only)   |
|    Defaults                                                  |
|                                                              |
|                                                              |
|                                                              |
|  Connected as an Xbox 360 controller                         |
|                                                              |
|  (A) CHANGE                                       (B) CLOSE  |
+--------------------------------------------------------------+
```

- Row types: choice (left and right change), toggle (A flips), action (A runs). A greyed row shows its reason in place of the value and the cursor skips it or A does nothing.
- Grey reasons: Rumble in generic gamepad mode ("Not used in generic gamepad mode"), Volume keys in Xbox mode ("Xbox mode: not available"), Mode while connected.
- The row list and order are owned by `controller-features.md`. The helper renders whatever `rows=` holds.
- Offline, A opens the same list as a full view without the "Input paused" line.
- Opening the overlay wakes a dimmed or off screen.

### Chord cheat sheet

Shown for as long as MENU is held, on the connected, waiting and dimmed views. Releasing MENU closes it. It does not capture a menu: it explains what the held MENU does to the next button.

- Timing: a MENU press released in under 300 ms is a tap and sends Guide on release, without flashing the sheet. After 300 ms of holding, or at the first chord, the sheet appears. Releasing MENU after the sheet appeared and no chord ran sends nothing, so reading the sheet never sends Guide.
- While it is up nothing reaches the computer.
- A chord pressed while the sheet is up runs and is marked on the sheet in a note line, for example "Turbo A: on", "Turbo A: off", "Brightness: 70 %", "D-pad mode: left stick". The note stays until MENU is released.
- Wakes a dimmed or off screen. The brightness bar is not drawn over it, the sheet shows the brightness value instead.
- The quit chord is listed separately at the bottom, because it does not use MENU.
- D-pad mode is decided: MENU + up, left or right, instant, no SELECT and no hold. The cheat sheet draws the three labels on the D-pad directions themselves (up "D-pad", left "Left stick", right "Right stick") beside the cross, not over it. Down is unused and has no label.

```
+--------------------------------------------------------------+
| Hold MENU for shortcuts                                      |
|                                                              |
|       L2 turbo   L1 turbo            R1 turbo   R2 turbo     |
|   .------------------------------------------------------.  |
|   |                                          X turbo      |  |
|   |        UP                                             |  |
|   |   LEFT    RIGHT                    Y turbo  A turbo   |  |
|   |       DOWN                                            |  |
|   |    (MENU + direction sets D-pad mode)     B turbo      |  |
|   |                                                       |  |
|   |   Volume + / -   SELECT        START       MENU       |  |
|   |   brightness     + direction   settings    (held)     |  |
|   '------------------------------------------------------'  |
|                                                              |
|                      Turbo A: on                             |
|                                                              |
|     Quit: hold SELECT + START for 2 s                        |
+--------------------------------------------------------------+
```

Chords on the sheet:

| Button | With MENU held |
| --- | --- |
| A, B, X, Y, L1, R1, L2, R2 | Toggle turbo for that button |
| Volume up, down | Brightness step, repeats while held |
| START | Open the settings overlay |
| Up | D-pad mode: D-pad |
| Left | D-pad mode: left stick |
| Right | D-pad mode: right stick |

No Guide passthrough: a button pressed while MENU is held never reaches the computer, whether or not it has a shortcut. D-pad up, left and right set the D-pad mode; SELECT, D-pad down, L3 and R3 under MENU do nothing. An earlier MENU + SELECT passthrough (Guide + Back) was dropped: no common host software needs it, and a quick MENU tap followed by SELECT covers it. `controller-features.md` has the same rule.

### Hold progress

A bar with a label fills while the quit hold is in progress, so a long press never feels dead and a mistaken one can be released. It is the only timed hold, since the D-pad mode chord is instant.

| Hold | Duration | Label |
| --- | --- | --- |
| SELECT + START | 2 s | Keep holding to quit |

```
|                                                              |
|                  Keep holding to quit                        |
|                  [#############..........]                   |
|                                                              |
```

- Drawn near the bottom centre over the connected view, the waiting view and a dimmed screen. It disappears when the hold ends before completion. On completion the action runs and the bar is not drawn.
- Over a dimmed screen the bar is drawn at the current backlight. On an off screen a hold in progress wakes it to dim so the bar can be seen.
- State: `hold` and `hold_pct` from the chord layer, the quit chord's `held_since` (`ExitChord`). The bar redraws at the snapshot rate while a hold is active.

### Brightness bar

Shown for about 1 s after each MENU plus volume step, with the overlay open or closed, and on a dimmed or off screen (the chord wakes it). It sits near the bottom centre over any view.

```
|                                                              |
|                  Brightness  [##########       ]             |
|                                                              |
```

Style follows NextUI's own bar (**Unverified**: its exact look). The Brightness row in the overlay shows the same bar.

### Screen dim and off

Default `Dim after 30 s` while connected. Other values: `Off after 30 s` and `Always on`.

- Brightness is a session value. On start `Backlight` records the original level and writes it to the recovery file. The chord and the overlay change the session value. Dim and off apply on top of it: dim is `min(session, DIM_RAW)` (a starting value of 16 raw, feel **Unverified**) and off is 0.
- Restore the recorded original on every exit path and in `pocket-controller recover`, which is already the rule for `Backlight`.
- Wake on: overlay open, the cheat sheet, MENU plus volume, POWER while dimmed or off (POWER then does not quit), disconnect, error. The quit hold in progress wakes an off screen to dim. Never on ordinary buttons or on a rumble burst from the computer.
- `controller-features.md` lists wake on overlay, POWER, disconnect and error, and separately says the brightness chord wakes. Both are kept here, plus the cheat sheet and hold progress.
- Auto screen-off is paused while the debug view is open.
- Dim is the default because a fully dark screen looks like a crash while every button goes to the computer.

### Debug view

Opened from the overlay's Debug row, or with X while waiting. It is not captured, so every button still reaches the computer and the Held line shows what the computer receives. Updates at up to 10 Hz, held buttons immediately.

```
+--------------------------------------------------------------+
| Debug                                         connected 04:12|
|                                                              |
| Mode      automatic -> xbox        USB   (v) configured      |
| Layout    int-in:32 int-out:32     Host  enabled, awake      |
|                                                              |
| Held      A  R1  DOWN        (what the computer receives)    |
| Sticks    LX 0   LY 0   RX 0   RY 0     Triggers  0   0      |
| Options   D-pad as left stick, turbo A B                     |
|                                                              |
| Report    00 14 20 00 00 00 00 00 00 00 00 00 00 00 ...      |
| Sent      12 reports/s                                       |
|                                                              |
| Rumble    on  level 128 / 64     (from computer, 2 s ago)    |
| LED       01  Player 1                                       |
|                                                              |
| Errors    none                                               |
|                                                              |
|                                                              |
|  Press MENU + START to close                                 |
+--------------------------------------------------------------+
```

- The USB line starts with a status icon: green check when configured, amber warning for powered, enumerating and no host, red cross for a stuck port. Offline, B closes it and `(B) BACK` shows. Connected, MENU + START closes it (it does not open the overlay then; a second MENU + START does). A MENU tap still sends Guide. Holding MENU shows the cheat sheet over it.
- The Held line shows the state after the face layout, turbo and D-pad mode stages. That is the report. The raw pad is not shown.

### Errors

- Startup failure (pocket-controller exits non-zero): keep the `launch.sh` path, `minui-presenter --message ... --timeout 4`, since the helper may be what failed. Text: "Pocket Controller stopped" and the log location, plus the first line of the error when short.
- Controller stuck (pocket-controller running, the USB device controller no longer binds): in-app screen, so the user is not dropped to the launcher unexplained.

```
+--------------------------------------------------------------+
| Pocket Controller                                               |
|                                                              |
|                                                              |
|                                                              |
|                 (x) The USB port is stuck                    |
|                                                              |
|                 Turn the Brick off and on again              |
|                 to fix it, then try once more.               |
|                                                              |
|                 Your settings are unchanged.                 |
|                                                              |
|                                                              |
|                                                              |
|                                                              |
|                                                              |
|                                                              |
|                                                              |
|                                                              |
|  (X) DETAILS                                        (B) QUIT |
+--------------------------------------------------------------+
```

How to detect "stuck" is **Unverified**. Candidates: a bind or UDC write that fails or times out after `Layout::settle`, or the UDC state never leaving `not attached` while VBUS is present after a rebind. The state machine needs `Input::UsbStuck` and an error screen state. The screen wakes on an error. A red cross icon sits before the title. "Your settings are unchanged." appears once, on this screen only, and not on the debug view.

## Controls

| Context | Buttons (printed labels) |
| --- | --- |
| Connected | Everything goes to the computer except the chords in `controller-features.md`: MENU tap (Guide), MENU held (cheat sheet), MENU + START (settings overlay), MENU plus volume (brightness), MENU plus A, B, X, Y, L1, R1, L2 or R2 (turbo), MENU plus up, left or right (D-pad mode, instant), SELECT + START 2 s (quit). POWER quits, or wakes a dimmed screen. The quit hold shows a progress bar |
| Cheat sheet | Shown while MENU is held. A chord pressed with it up runs and is marked. Release MENU to close |
| Overlay | Up and down move, left and right change, A cycles or runs, B, MENU or MENU + START closes |
| Waiting | A or MENU + START settings, X debug, Y mode, B quit. Holding MENU shows the cheat sheet |
| Debug, offline | B back |
| Debug, connected | MENU + START closes it. Not captured |
| Settings, offline | Same as the overlay without "Input paused" |
| Error, stuck | X details, B quit |

Local controls read the state before the face layout stage and use printed labels, as `controller-features.md` requires.

## Navigation map

```
                         start
                           |
                           v
   +---------+ A or MENU+START  +----------+
   | Waiting |----------------->| Settings |
   |         |<-----------------|          |
   +---------+        B         +----------+
     |  ^  |  X
     |  |  +-----------> Debug (B back)
 host|  |unplug
 on  v  |
   +-----------+  MENU+START   +---------------------+
   | Connected |<------------->| Overlay (captured)  |
   +-----------+  B or MENU    |  row: Debug --------+--> Debug (not captured,
     |    ^  |                 +---------------------+     MENU+START closes)
     |    |  +-- MENU held: cheat sheet (release closes)
 timer|    | MENU+vol, MENU held, POWER, overlay, disconnect, error
     v    |
   +--------------------+
   | Dimmed or off      |
   +--------------------+

 Anywhere: SELECT + START 2 s quits, with a progress bar. POWER quits unless the screen is dimmed or off.
 Error (stuck) replaces Waiting.
```

## Bindings to the state machine

Existing, verified in `src/control.rs` and `src/system.rs`:

| Element | Source |
| --- | --- |
| View and diagnosis | `Controller::current_screen()`, `Screen::{Waiting(Mode, Diagnosis), Checking, Connected(Kind)}`. The helper view never shows `Checking`: pocket-controller keeps the previous view and sets `busy` |
| Link and timers | `Input::Link(UsbLink)`, `UsbLink::classify`, the 3 s thresholds behind `Diagnosis` |
| Mode, kind | `Controller.mode`, `Controller.kind` |
| Backlight | `Backlight`, `read_brightness`, `write_brightness`, saved level file |
| Rumble level | `Input::Rumble(u8)`, `Action::Motor` |

To add (the snapshot is pure and owned by `Controller`, `main.rs` forwards it):

| Element | Needed |
| --- | --- |
| Hint bar | Pure `hints(view, connected) -> Vec<Hint>` |
| No Xbox driver banner | A banner deadline set where `switch_to(Gamepad)` runs on `Tick` |
| Overlay, cursor, rows | `overlay: Option<Cursor>`, a pure `rows(settings, mode, kind) -> Vec<Row>` with grey reasons |
| Cheat sheet and note | `menu_since: Option<Instant>`, `sheet: bool` (set after 300 ms or at the first chord), `sheet_note: Option<String>` set by each chord outcome |
| Hold progress | `hold: Option<(Hold, Instant, Duration)>` from the quit chord, percent computed in the pure snapshot from `now` |
| `dpad_mode`, `turbo` | `RuntimeState` from `controller-features.md` |
| `screen_level`, `brightness` | Screen state in `Controller` (on, dim, off), session brightness, `Action::Backlight(level)` handled beside `Motor` |
| Brightness bar | A deadline of about 1 s set by each brightness step |
| Held, Report | Last `ChordOutcome::sent` state after all stages, plus a report counter |
| Rumble on and level | Store the last level and its `Instant` |
| LED report, Player N | New `Input::Led(u8)` from `functionfs.rs` (**Unverified** where the report is read today) |
| Endpoint layout | Chosen layout string passed at startup |
| Errors | `Input::Error(...)`, `Input::UsbStuck`, a bounded error list |

Rendering sits behind a trait owned by the caller, like `Motor`: `show(&Snapshot)`. Implementations: `HelperScreen` (spawn, write lines, one restart on failure), `PresenterScreen` (today's `Presenter`, fed plain text from the snapshot), and a recording fake for tests. Snapshot to text and to protocol line are pure and tested.

## Rollout

1. Extract the render trait around `Presenter` and add the snapshot, hints and banner to `control.rs` with tests. The only visible change is the new diagnosis wording already in place.
2. Add `pocket-controller-screen` with waiting, connected and error views, theme, font and scaling. Prove the build story on the Brick.
3. Add the overlay, cheat sheet, hold progress, brightness bar and dim with the chord layer.
4. Add the debug view.
5. Measure on device: first frame time, redraw cost, power with the screen dimmed and off, the stock firmware, then Smart Pro and H700 glibc and SDL2 versions.

## Not verified

- Raw `/dev/fb0` writes, and whether a software renderer or window surface is the right path under SDL 2.30.8 on this device.
- Whether 300 ms is the right tap versus hold threshold for MENU, and whether losing Guide + A, B, X, Y and the shoulders hurts Steam Guide use.
- That `cargo zigbuild` or a Nix cross sysroot produces a working glibc 2.28 to 2.33 binary inside this repository's dev shell.
- glibc and SDL2 versions on tg5050, H700 and stock TrimUI.
- Colour roles in `config.c`, font location, NextUI's brightness bar look.
- Dim level feel (starting value 16 raw) and binary sizes.
- How to detect a wedged controller.
- Where the Xbox LED report is read today.
- H700 screen sizes.
