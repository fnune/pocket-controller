# Controller features

## Summary

- pocket-controller forwards the built-in pad unchanged today. These options add what a handheld-as-controller needs and the computer cannot do for it.
- One control scheme, copied from popular pads: MENU is the Home button and the only modifier. A tap (under 300 ms) sends Guide. Holding it shows a cheat sheet of chords and sends nothing on release. MENU plus a button toggles turbo, MENU plus START opens the settings overlay, MENU plus volume sets brightness, MENU plus Up, Left or Right picks D-pad, left stick or right stick. Only the quit hold shows a progress bar.
- The FN switch is the hardware version of the D-pad/stick switch. POWER wakes the screen first, then quits.
- Persisted settings: face button layout, rumble, screen while playing, volume keys, LED status. Runtime state, reset on each start like 8BitDo: D-pad mode (FN switch sets the start value) and turbo buttons.
- Volume keys are handled by pocket-controller itself, since the grab keeps them from NextUI. MENU plus volume sets the screen brightness in every mode. Host volume (volume alone) is optional and deferred: generic gamepad mode only, through report IDs. Xbox mode gets none.
- Out of scope: remap, profiles, stick tuning, screenshot, gyro, L3/R3, anything needing host software.

## Conventions followed

Sources were searched on 2026-11-23. Anything not found is marked unverified.

- Mode choice on 8BitDo pads happens when powering on, by holding Start plus a face button (SN30 Pro: Start+Y Switch, Start+B Android, Start+X Windows, Start+A macOS): [SN30 Pro manual](https://www.manualslib.com/manual/3468333/8bitdo-Sn30-Pro.html). A wired handheld has no power-on moment, so pocket-controller keeps mode as an offline setting. Not copied.
- Turbo on 8BitDo: hold the button, then press Star (Lite and Ultimate: Star is the turbo key); not saved across power-off; D-pad, sticks, Home, Select and Start are excluded: [SN30 Pro manual](https://www.manualslib.com/manual/3468333/8bitdo-Sn30-Pro.html), [Lite FAQ](https://api.8bitdo.com/faq/lite.html), [Lite 2 manual](https://manuals.plus/8bitdo/lite2-8-bluetooth-gamepad-manual). Copied with MENU as the turbo key.
- D-pad as stick on 8BitDo's stickless Zero 2: Select plus Left sets left stick, Select plus Right sets right stick: [Zero 2 FAQ](https://api.8bitdo.com/faq/zero2.html). Select plus Up for plain D-pad is from memory, unverified. Copied in idea, with MENU instead of Select: SELECT is a game button that reaches the computer before a chord completes, while MENU is withheld until its role is decided. The Brick is a stickless SNES-style pad, the closest match.
- Home on 8BitDo is held 3 s to power off: [Lite 2 manual](https://manuals.plus/8bitdo/lite2-8-bluetooth-gamepad-manual), and Start 3 s powers off the SN30 Pro. Not copied, since POWER and the exit chord already exist.
- Screenshot: the SN30 Pro's Star button is the Switch screenshot button ([manual](https://www.manualslib.com/manual/3468333/8bitdo-Sn30-Pro.html)). The Brick has no spare button, so no screenshot.
- Nintendo's Switch Online SNES controller adds only small ZL and ZR to the original layout, has no sticks, and charges over USB-C ([Nintendo support](https://en-americas-support.nintendo.com/app/answers/detail/a_id/47357), [GameSpot via search](https://www.gamespot.com/articles/that-official-nintendo-switch-snes-controller-is-a/1100-6469882/)). Home and Capture on it were not confirmed. It supports no on-pad settings, which matches pocket-controller's small option set.
- Handheld PCs: holding the Steam button shows a list of shortcuts on Steam Deck ([overview](https://freedom251.com/how-to-use-steam-decks-hidden-shortcuts/)), and the Legion Go reaches Guide through a chord ([Lenovo forum](https://gaming.lenovo.com/legion-go/post/how-are-we-still-unable-to-map-xbox-guide-button-s00zotWltkPmZKd)). Hold-Home for a list of shortcuts and Home plus a button for the shortcut itself are the convention. Copied as the cheat sheet and the MENU chords.
- Opposite note: the Xbox 360 pad and 8BitDo pads have no host volume control. Handhelds that act as PCs do; a controller does not. Volume is therefore ours to justify (see below).
- NextUI on the Brick (from its guide and release notes, not its source): in menus, hold Start plus Volume Up or Down for brightness, hold Select plus Volume for colour temperature, Menu plus Select opens the game switcher ([setup guide](https://www.joeysretrohandhelds.com/guides/nextui-setup-guide/), [NextUI](https://loveretro.github.io/NextUI)). The FN switch can be bound globally to joystick emulation since 5.9.0 ([release](https://newreleases.io/project/github/LoveRetro/NextUI/release/v5.9.0)), which is the same idea as D-pad as stick, and 6.14.0 added user-assigned FN actions ([release](https://newreleases.io/project/github/LoveRetro/NextUI/release/v6.14.0)). I found no source naming keymon's exact bindings, so they are unverified. Copied: FN as D-pad/stick switch, and a modifier plus volume for brightness. Departure: brightness uses MENU, not Start, because START is a game button that reaches the computer before the chord completes, while MENU is withheld until its role is decided. Colour temperature is not copied (no backend known).
- Unverified: how 8BitDo's X-input mode places the face buttons (position or label); Logitech-style mode switches. Not relied on.

## Chord scheme

One table settles settings, debug, quit and Guide. All chords are withheld from the computer the way `ExitChord` withholds SELECT+START (`ChordOutcome::sent`).

| Input | Action |
| --- | --- |
| MENU tap (under 300 ms, nothing else pressed) | Guide pulse of 100 ms, sent on release |
| MENU held past 300 ms, or at the first chord | Cheat sheet, shown while MENU is held. Nothing reaches the computer. Releasing MENU with no chord sends nothing, so reading the sheet never sends Guide |
| MENU + START | Open the settings overlay. While the debug view is open, close it. A second press while the overlay is open closes it |
| MENU + A, B, X, Y, L1, R1, L2 or R2 | Toggle turbo for that button |
| MENU + Volume Up or Down | Brightness up or down one step, repeating while held. Wakes a dimmed screen |
| MENU + Up, Left or Right | D-pad mode, instant: Up is D-pad, Left is left stick, Right is right stick. Withheld, no Guide |
| MENU + SELECT alone | Passthrough: Guide goes down and SELECT passes, so Guide + Back still reaches the computer. This is the only passthrough. MENU + Down is unused and sends nothing. Guide + A, B, X, Y, the shoulders and the D-pad no longer reach the computer (agreed with `screen.md`) |
| Volume Up or Down alone | Host volume in generic gamepad mode if enabled. Otherwise nothing, never forwarded as a button |
| SELECT + START held 2 s (progress bar) | Quit, unchanged |
| POWER, screen on | Quit, unchanged |
| POWER, screen dimmed or off | Wake the screen, do not quit |
| FN switch changes | D-pad mode: left stick in one position, D-pad in the other |

Resolution of the conflicts with `screen.md`:

- Settings while connected: MENU + START opens the overlay. It captures all input, so its hint bar does not lie. The computer receives one neutral report on open. Holding MENU alone no longer opens anything, it shows the cheat sheet (`screen.md`).
- Debug view: a row in the overlay. Opening it leaves capture, so held buttons still reach the computer and the "Held" line shows what it receives. MENU + START closes it, and the next MENU + START opens the overlay. SELECT+MENU is not used.
- Waiting screen: keeps `screen.md` keys (A settings, X debug, Y mode, B quit). A or MENU + START opens settings there.
- Quit: unchanged.
- Auto screen-off stays paused while the debug view is open.

## Pipeline

Fixed stages between the evdev state and `Action::Report`.

1. Exit chord (`chord.rs`), on the raw state, always works.
2. Chord layer: MENU decision, overlay capture, MENU+direction, turbo toggle. A new `chords.rs` next to `chord.rs`, pure.
3. Face layout swap, on the report only.
4. Turbo.
5. D-pad to stick.
6. Report (xinput or hid encodes it), plus the consumer report for volume in HID mode.

Local controls (`LOCAL_QUIT`, `LOCAL_CHANGE_MODE`, overlay navigation) read the state before stage 3 and use printed labels, so the swap never changes their meaning. This confirms the `screen.md` request. Today those constants are named in Xbox terms (`xinput::A`, `xinput::X`); the evdev mapping is positional and correct, measured on hardware (printed B, south, 0x130, is Xbox A).

Timing: the MENU decision runs on `Tick` (100 ms resolution is enough). Turbo needs finer timing, see Turbo.

`control.rs` gains `Settings` (persisted), `RuntimeState` (D-pad mode, turbo mask, overlay), `Input::PadSwitch(bool)` for the FN switch, `Input::VolumeKey(...)`, `Action::SaveSettings`, `Action::Backlight`, `Action::Leds`, `Action::Consumer(...)`.

## Options

### Overlay (settings and debug)

- Behaviour: MENU + START opens it (offline, A or MENU + START). Up and down move, left and right change the value, A cycles two-value rows, B or MENU + START closes it. L1 and R1 are not used. Changes apply live and are saved on close. Held buttons at open are withheld until released after close.
- Rows, in order: D-pad mode, Face buttons, Rumble, Screen while playing, Brightness (value, same as the chord), Volume keys, LEDs, Clear turbo, Debug, Mode (read-only while connected, editable offline), Defaults.
- Wakes the screen to full brightness.
- Xbox and HID: same, except where a row is greyed (below).
- Offline: A opens settings (screen doc). The same list, without capture.
- State machine: `overlay: Option<Cursor>`, `menu_since: Option<Instant>` and `sheet: bool` (set after 300 ms or at the first chord) in `Controller`, and a `hold` value with start time and duration for the quit hold.

### Face button layout

- Behaviour: `Position` (default) keeps the measured positional mapping, the Brick's south button sends Xbox A. `Label` swaps A with B and X with Y so each printed letter sends the same Xbox letter.
- Default `Position`, because games show Xbox prompts by position. Whether 8BitDo's X-input mode does the same is unverified.
- Xbox and HID: same swap before encoding.
- Acts only on the report, stage 3. Local UI uses printed labels.
- Table-based permutation internally, so remap stays cheap later, not exposed.

### D-pad mode

- Behaviour: `D-pad` (default), `Left stick`, `Right stick`. In a stick mode the D-pad bits are removed and the stick carries the direction at full deflection. Diagonals scale to 0.7071 (23170 of 32767).
- Reached by MENU plus Up (D-pad), Left (left stick) or Right (right stick), instant, no hold and no progress bar, by the FN switch, or by the overlay row. Shown on the cheat sheet. Runtime only, not saved, like 8BitDo.
- FN switch (NextUI binds the same switch to joystick emulation, so users already expect this): an `EV_SW` code 1 latching switch (measured). One position means left stick, the other D-pad. The start value comes from the switch level at launch (needs `EVIOCGSW`, unverified), and later changes override whatever a chord set. Which physical position is which is unverified; the first release treats value 1 as left stick.
- Xbox: `left_x`, `left_y` (or right). HID: axis bytes, centre 127 or 128, hat neutral.
- Smart Pro: the real stick wins per axis when it is larger than the D-pad's contribution. Unverified.
- Hint: a one-line "D-pad: left stick" on the connected screen while not default. A short motor pulse confirms a chord change (one pulse for stick mode, two for D-pad) when rumble is On or Off alike, since it is local feedback.

### Turbo

- Behaviour: hold MENU, then press the button. Toggles turbo for that button; the setting stays when both are released. This reverses 8BitDo's order (button first, then Star) to fit the cheat sheet. Buttons: A, B, X, Y, L1, R1, L2, R2. About 12 Hz (40 ms down, 40 ms up). Not saved across starts, like 8BitDo.
- Feedback: one motor pulse for on, two for off, plus an "Options" line on the connected and debug views. Clear all: overlay row.
- Xbox and HID: same, before encoding.
- Needs a clock finer than the 100 ms `TICK` in `main.rs` (a 100 ms tick gives 5 Hz at best). Use a deadline-based `recv_timeout` while a turbo button is held. Report throughput at 25 reports per second is unverified.
- Ships after the first group.

### Triggers

- No option. L2 and R2 are digital (measured, 0 or 255 on `ABS_Z` and `ABS_RZ`). Xbox mode sends 0 or 255, HID sends buttons 9 and 10. No ramp: it would help only racing games.

### Rumble

- Behaviour: `On` (default) or `Off`. Off ignores `Input::Rumble` and keeps the motor stopped. The motor is on or off only.
- Xbox: applies. HID: no output report exists, so the row is greyed with "Not used in generic gamepad mode".
- State machine: gate in the `Input::Rumble` arm. `Strong only` is not decided and needs a game to test.

### Screen while playing

- Decision: default `Dim after 30 s`, the choice follows `screen.md`'s own argument that a black frame saves little while the backlight dominates, and a fully dark screen looks like a crash while every button goes to the computer. Other values: `Always on`, `Off after 30 s`. The dim level is a raw backlight value that the screen doc owns.
- Mechanism and restore are owned by `screen.md` (`/dev/disp` ioctl 0x102, restore on every exit path and in `pocket-controller recover`).
- Wake: overlay open, the cheat sheet, MENU + volume, POWER when dimmed, disconnect, error. Never ordinary buttons. A rumble burst does not wake it.
- Xbox and HID: same.

### LED status

- Behaviour: `Off` (default) or `Status`. Waiting is dim amber, connected as Xbox is green, connected as generic gamepad is blue, overlay open is white. Originals are restored on every exit path.
- Default off because a NextUI pak (LED'oh!) owns the LEDs. Useful mostly with the screen dimmed.
- Not verified: the format of `/sys/class/leds/sunxi_led*` and how LED'oh! stores state. Ship last, after reading the sysfs on the device.

### Volume keys and brightness

- Facts: `KEY_VOLUMEUP` and `KEY_VOLUMEDOWN` are on the grabbed pad device (measured), so they never reach NextUI's keymon while pocket-controller runs. The Brick's own volume is of no use while it is a controller, so nothing is lost by owning them.
- Brightness: MENU held, then Volume Up or Down, changes the backlight by one step (about ten steps over the raw 0 to 255 range, never fully dark, unverified how NextUI scales it). Works in both modes, offline, and with the overlay open or closed. The cheat sheet shows the value while MENU is held. Mirrors NextUI's modifier plus volume, with MENU instead of Start (see conventions). Brightness is a session value: pocket-controller restores the original backlight on exit, as `screen.md` already requires. Dim and off levels apply on top of it, and the chord wakes a dimmed screen.
- Host volume is optional and last in the queue. Skip it if the descriptor change is not clean.
    - Generic gamepad mode: add a Consumer Control collection to the same interface and endpoint with report IDs (ID 1 gamepad, ID 2 a one-byte consumer report: Volume Increment, Volume Decrement). The 7-byte gamepad report gains an ID byte, so it needs a re-run of `just e2e` on Linux, Windows, macOS and Android first. Volume alone sends it. Setting `Volume keys`: `Computer volume` (default when available) or `Off`.
    - Xbox mode: the 360 protocol has no volume, and a second interface needs an IN endpoint that the Brick does not have free (bulk at full speed and interrupt at high speed is untested). Left out; volume alone does nothing and the row is greyed with "Generic gamepad mode only".
    - Automatic mode: works only after the fallback to the generic gamepad.
- Rejected: releasing the grab (every button would reach NextUI), and mapping volume to game buttons.

### POWER button

- Behaviour: POWER (separate `axp2202-pek` device, read without a grab) quits when the screen is on, and only wakes the screen when it is dimmed or off. Matches what a handheld's power key does.
- No long press. Quitting by POWER and by SELECT+START stay.
- State machine: `Input::PowerPressed` handler checks screen state. New `Controller` field for the screen level.

### FN switch

- Covered under D-pad mode. No other use; a second consumer would need another setting.

### Remapping and profiles

- Out. Reason: no touch UI, and Steam Input does it better on the host. Settings stay one set, shared by both personalities.

## What the screen must support

- Overlay: a list with a cursor, label left and value right, a hint bar of `(A) CHANGE (B) CLOSE`, a "Input paused" line, drawn on top of the connected view. The row types are choice, toggle and action (Clear turbo, Defaults, Debug).
- Greyed rows need a reason in text: Rumble and Volume keys in HID or Xbox as above.
- Debug view reachable from the overlay row and from X while waiting. While connected it is non-captured. Closing hint: "Press MENU + START to close".
- Cheat sheet, shown while MENU is held (after 300 ms, or at the first chord), on connected, waiting and dimmed views. It lists the chords in the table above and a note line for the last chord run ("Turbo A: on", "Brightness: 70 %", "D-pad mode: left stick"). The quit chord is listed at the bottom.
- Hold progress bar for the quit chord only, SELECT + START (2 s). It wakes an off screen to dim.
- Connected screen hint lines: "Hold MENU for shortcuts" and "Hold SELECT + START for 2 s to quit". The old "SELECT + MENU for debug" line is gone.
- An "Options" line on the connected and debug views for non-default runtime state: D-pad mode, turbo buttons.
- Waiting screen keeps A settings, X debug, Y mode, B quit, and adds nothing.
- A transient brightness bar for the MENU plus volume chord, shown for about 1 s, drawn even when the overlay is closed, and visible on the dimmed screen (the chord wakes it). NextUI's own bar is a style to follow.
- The snapshot needs `overlay`, `cursor`, `rows`, `dpad_mode`, `turbo`, `screen_level`, `brightness`. Debug `layout` stays.
- The screen must wake on overlay open, the cheat sheet, the chords above and the quit hold, and not depend on being on.

## Out of scope

- Full remap, macros, per-game profiles.
- Stick dead zones, curves, calibration.
- Analog trigger emulation.
- Screenshot or capture button.
- L3 and R3 (the Brick has no spare button; `KEY_F1`/`KEY_F2` turned out to be the FN switch, not extra buttons).
- Gyro, touch, rumble strength.
- Wireless transports.
- Changing mode live while connected.
- Xbox mode volume (until the spike proves a second interface).
- Anything that needs host software.

## Open questions

- Which FN switch position should mean left stick, and does the grab still deliver `EV_SW` events and the initial state?
- Is `BTN_MODE` always MENU on NextUI devices other than the Brick, and what plays MENU on the Smart Pro?
- Does the 300 ms tap threshold feel right, and is Guide-on-release a problem for Steam Guide chords?
- Is Select plus Up really 8BitDo's D-pad convention? Unverified, and no longer load-bearing since the chord uses MENU.
- Will the consumer report break any host that accepts today's 7-byte HID report? Needs a four-OS check.
- Backlight dim level, LED sysfs format, and LED'oh! coexistence.
- Is `Strong only` rumble worth a row?
- Does a held MENU plus volume feel right with auto-repeat, and how does NextUI scale brightness steps (its levels and any saved setting pocket-controller must not overwrite)?
- Should Xbox mode keep Guide-on-release, given that Steam reads Guide while held for its overlay? Not verified.
