# Ideas

Future work and open ideas. Move an item into a design doc in `docs/design/` when it gets designed, and delete it here when it ships.

## Next up

- **Screen helper in Rust:** `pocket-controller-screen` loads the device's SDL2 and SDL2_ttf at runtime. A spike drew text on the Brick through its Mali backend. See `docs/design/screen.md`.
- **MENU layer:** tap for Guide on release. Hold to show the chord cheat sheet (the Brick drawn with each button labelled by its MENU chord) for as long as MENU is held. MENU + START opens settings, MENU + button toggles turbo, MENU + volume sets brightness. This replaces the 600 ms hold-for-settings in `docs/design/controller-features.md` and `docs/design/screen.md`.
- **Hold progress:** a ring or bar that fills while a hold is in progress: quit (2 s), settings (600 ms), D-pad mode (1 s).
- **Connection diagnostics on screen:** the logic exists (`UsbLink`, `Diagnosis`). It needs the screens from `screen.md`.

## Inputs

- **Change mode while connected:** today Y changes the mode only while no computer is connected, because switching between Xbox and generic gamepad rebuilds the USB device. Offer it in the settings overlay with a note that the computer sees an unplug and replug.

- **FN switch:** D-pad as left stick, like NextUI's joystick emulation. It is `EV_SW` code 1. Which position means "on" is unverified.
- **What the volume buttons do:** nothing for now. Brightness on the Brick was tried and dropped as useless while playing. Decide later.
- **Volume keys to the computer:** only in generic gamepad mode, through a consumer-control report sharing the gamepad's endpoint. Optional, skip if the descriptor change is not clean. In Xbox mode the Brick has no spare interrupt endpoint.
- **Turbo:** about 12 Hz per button. Needs a faster timer than the 100 ms tick.
- **Rumble strength:** the Brick's motor is on or off. Strength would mean pulsing it.

## Transports

- **Bluetooth controller:** the Brick has BlueZ 5.78, classic and LE, and `UHID`. Start with a spike: pair with a laptop as a classic HID gamepad and send one button. Later identities: Xbox One S over classic Bluetooth, Switch Pro Controller. Open points: coexisting with NextUI's Bluetooth audio, XR829 reliability, and talking to BlueZ over D-Bus without breaking the std-and-libc rule.
- **Windows XInput in Xbox mode:** needs the XUSB class descriptor, which these kernels' FunctionFS rejects. Only possible with a patched `f_fs` module per kernel (handtop does this). Out of scope for stock kernels.

## Screen and device

- **LED status:** show the connection or the player number on the Brick's RGB LEDs. Restore them on every exit path.
- **Screen off option:** besides dimming, turn the backlight off after 30 s.

## Platforms

- **Stock TrimUI firmware:** the wrapper in `packaging/trimui-stock` is untested.
- **TrimUI Smart Pro and Brick Pro:** expected to work, untested. The Smart Pro has analog sticks, which already map through.
- **TrimUI Smart Pro S (tg5050) and Anbernic H700 under NextUI:** need a kernel check and a `pocket-controller-probe` run on the device.
- **Other firmwares:** ROCKNIX and Knulli on H700 and RK3326/RK3566 ship FunctionFS. Each needs its own packaging folder and a device profile.
- **Device profiles as data:** one table keyed by `$PLATFORM`: motor path, backlight method, port hint text, button names. No per-platform code.

## Project

- **Repository and CI:** run `just check` and `nix build .#checks.x86_64-linux.vm` on every push once the repository exists.
- **Quirk emulation in the VM test:** the one-interrupt layout is covered. The settle delay could be checked by timing binds. The controller wedge needs real hardware.
- **Split `screen.md`** into a portable part (screens, flows, cheat sheet, hold progress) and a short NextUI rendering section.
- **USB product ID for Generic mode:** replace the stand-in `1d6b:0105` with a pid.codes ID under vendor `1209`. Intended PID: `9AD1` ("PAD1", free on 2026-11-23; recheck before submitting, since only an accepted pull request reserves it). Open a pull request to [pidcodes/pidcodes.github.com](https://github.com/pidcodes/pidcodes.github.com) adding `org/<owner>/index.md` and `1209/<PID>/index.md` (title, owner, licence, source URL), as described in the [how-to](https://pid.codes/howto/). Needs the public repository with its `LICENSE` first. Software-only projects may be asked why they need an ID: the Brick enumerates as its own device, so users cannot request their own. Xbox-style keeps `045e:028e`.
- **Settings screen title:** the screens have no title row, to give the content room. Show "Pocket Controller" and the version on the settings screen once it exists.
