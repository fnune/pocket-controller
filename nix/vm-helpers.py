import sys
import time

import evdev
from evdev import AbsInfo, UInput, ecodes


def fake_pad() -> None:
    capabilities = {
        ecodes.EV_KEY: [
            ecodes.BTN_SOUTH,
            ecodes.BTN_EAST,
            ecodes.BTN_NORTH,
            ecodes.BTN_WEST,
            ecodes.BTN_TL,
            ecodes.BTN_TR,
            ecodes.BTN_SELECT,
            ecodes.BTN_START,
            ecodes.BTN_MODE,
            ecodes.KEY_VOLUMEUP,
            ecodes.KEY_VOLUMEDOWN,
        ],
        ecodes.EV_ABS: [
            (ecodes.ABS_Z, AbsInfo(0, 0, 255, 0, 0, 0)),
            (ecodes.ABS_RZ, AbsInfo(0, 0, 255, 0, 0, 0)),
            (ecodes.ABS_HAT0X, AbsInfo(0, -1, 1, 0, 0, 0)),
            (ecodes.ABS_HAT0Y, AbsInfo(0, -1, 1, 0, 0, 0)),
        ],
    }
    with UInput(capabilities, name="TRIMUI Player1", vendor=0x045E, product=0x028E):
        while True:
            time.sleep(3600)


def find_device(name: str) -> str:
    for path in evdev.list_devices():
        if evdev.InputDevice(path).name == name:
            return path
    raise SystemExit(f"no input device named {name!r}")


def record(name: str, seconds: str) -> None:
    device = evdev.InputDevice(find_device(name))
    deadline = time.monotonic() + float(seconds)
    while time.monotonic() < deadline:
        try:
            event = device.read_one()
        except OSError:
            break
        if event is None:
            time.sleep(0.01)
        elif event.type in (ecodes.EV_KEY, ecodes.EV_ABS):
            print(f"{event.type} {event.code} {event.value}", flush=True)


if __name__ == "__main__":
    command, *arguments = sys.argv[1:]
    {"fake-pad": fake_pad, "find": lambda name: print(find_device(name)), "record": record}[command](*arguments)
