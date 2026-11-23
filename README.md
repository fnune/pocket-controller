# Pocket Controller

Pocket Controller turns a Linux handheld into a wired USB controller for a computer. Nothing to install on the computer.

> [!NOTE]
> So far Pocket Controller has only been tested on the TrimUI Brick running NextUI. Other devices are packaged but untested: see [Supported devices](#supported-devices).

| Connected | Shortcuts (hold MENU) |
| --- | --- |
| ![Connected as Xbox-style, with the live handheld showing pressed buttons and turbo on A and R1](docs/screenshots/connected.png) | ![The shortcuts view, labelling every button's MENU chord](docs/screenshots/shortcuts.png) |
| **D-pad as an analog stick** | **Mode picker** |
| ![The D-pad drawn as a stick well, its thumb following the emulated stick](docs/screenshots/analog.png) | ![The mode picker with Automatic, Xbox-style and Generic](docs/screenshots/picker.png) |

pocket-controller turns the handheld's USB port into a USB device through the kernel's gadget framework (configfs and FunctionFS) and forwards the built-in controller to it.

## Modes

- **Automatic** (default): starts as Xbox-style. If no Xbox driver answers within two seconds, it switches to Generic. Unplugging tries Xbox-style again for the next computer.
- **Xbox-style**: emulates an Xbox 360 controller, with Microsoft's IDs and protocol. Linux (`xpad`) and Steam recognise it without setup.
- **Generic**: a standard USB HID gamepad that most hosts accept without drivers, including Windows, macOS and Android. Games may need you to map its buttons once.

Change the mode on the handheld's screen with Y before connecting. Quit by holding SELECT + START, pressing POWER, or pressing B while no computer is connected.

## Shortcuts

Tap MENU to send Guide. Hold MENU to see the shortcuts on screen:

- MENU + A, B, X, Y, L1, L2, R1 or R2: turbo on that button.
- MENU + up, left or right: the D-pad acts as the D-pad, the left analog stick or the right analog stick.

The face buttons send what is printed on them: the button labelled A sends A.

## Supported devices

| Device | Firmware | Status |
| --- | --- | --- |
| TrimUI Brick | NextUI | Tested |
| TrimUI Smart Pro, Brick Pro | NextUI | Expected to work, untested |
| TrimUI Brick, Smart Pro | Stock | Expected to work, untested |
| TrimUI Smart Pro S | NextUI | Untested |
| Anbernic H700 devices | NextUI | Untested |

### TrimUI Brick

- Use the bottom USB-C port. The top port is host-only.
- Connect it to a USB-A port on the computer, or through a USB-C to USB-A adapter or hub. With a USB-C to USB-C cable, laptops make the Brick the power source and nothing connects.

## Install

- NextUI: copy `Pocket Controller.pak` into `Tools/<platform>/` on the SD card, or unzip `Pocket.Controller.pak.zip` there.
- Stock TrimUI firmware: copy `PocketController` into `Apps/` on the SD card.

While it runs, the handheld's ADB gadget is set aside and restored exactly when pocket-controller quits. If pocket-controller crashes, the next start restores it, and so does a reboot.

## Development

```
nix develop
just check
just pak
just deploy
just e2e
```

`just e2e` needs the handheld on its device port, reachable over SSH (`POCKET_CONTROLLER_HOST`, `POCKET_CONTROLLER_SSH_KEY`), and connected to this computer.

`pocket-controller-probe` reports which endpoint layouts a device's USB device controller can bind, for porting to new devices:

```
pocket-controller-probe int-in:32 int-out:32
```

## License

AGPL-3.0-or-later. The Xbox 360 and HID protocol details follow [JukaHub](https://github.com/jukaLang/JukaHubV2)'s controller mode.
