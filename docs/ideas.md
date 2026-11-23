# Ideas

Future work and open ideas. Move an item into a design doc in `docs/design/` when it gets designed, and delete it here when it ships.

## Inputs

- **What the volume buttons do:** nothing for now. Brightness on the Brick was tried and dropped as useless while playing. Decide later.
- **Volume keys to the computer:** only in generic gamepad mode, through a consumer-control report sharing the gamepad's endpoint. Optional, skip if the descriptor change is not clean. In Xbox mode the Brick has no spare interrupt endpoint.
- **Rumble in Generic mode:** the HID gamepad has no output report, so the computer cannot ask for rumble. HID force feedback (PID) would add it: Linux handles it with `hid-pidff`, Windows through DirectInput. Needs a descriptor change and testing on each OS.
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

- **Release process and pak store:** copy kyaraben's release process (`~/Development/kyaraben`), which ships to the pak store, so tagged releases build the NextUI pak and the stock firmware app and publish them.
- **Repository and CI:** run `just check` and `nix build .#checks.x86_64-linux.vm` on every push once the repository exists.
- **Quirk emulation in the VM test:** the one-interrupt layout is covered. The settle delay could be checked by timing binds. The controller wedge needs real hardware.
- **Split `screen.md`** into a portable part (screens, flows, cheat sheet, hold progress) and a short NextUI rendering section.
- **USB product ID for Generic mode:** replace the stand-in `1d6b:0105` with a pid.codes ID under vendor `1209`. Intended PID: `9AD1` ("PAD1", free on 2026-11-23; recheck before submitting, since only an accepted pull request reserves it). Open a pull request to [pidcodes/pidcodes.github.com](https://github.com/pidcodes/pidcodes.github.com) adding `org/<owner>/index.md` and `1209/<PID>/index.md` (title, owner, licence, source URL), as described in the [how-to](https://pid.codes/howto/). Needs the public repository with its `LICENSE` first. Software-only projects may be asked why they need an ID: the Brick enumerates as its own device, so users cannot request their own. Xbox-style keeps `045e:028e`.
