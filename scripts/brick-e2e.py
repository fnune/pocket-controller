#!/usr/bin/env python3
import os
import re
import select
import struct
import subprocess
import sys
import time
from collections.abc import Callable

HOST = os.environ.get("POCKET_CONTROLLER_HOST", "root@192.168.1.92")
KEY = os.environ.get("POCKET_CONTROLLER_SSH_KEY", os.path.expanduser("~/.ssh/id_ed25519_brick"))
SSH = ["ssh", "-i", KEY, "-o", "BatchMode=yes", "-o", "IdentitiesOnly=yes", "-o", "LogLevel=ERROR", HOST]
RELEASE = "target/aarch64-unknown-linux-musl/release"
PAD = "/dev/input/event3"
EV_KEY, EV_ABS = 1, 3
BTN_A, BTN_SELECT, BTN_START, BTN_MODE = 0x130, 0x13A, 0x13B, 0x13C
BTN_TL = 0x136
PRINTED_A = 0x131
ABS_Z, ABS_HAT0X, ABS_HAT0Y = 0x02, 0x10, 0x11

Event = tuple[int, int, int]

failures: list[str] = []


def brick(command: str) -> str:
    return subprocess.run(SSH + [command], capture_output=True, text=True, check=False).stdout.strip()


def inject(*events: str) -> None:
    _ = brick(f"/tmp/pocket-controller-inject {PAD} " + " ".join(events))


def host_device(vendor: str, product: str, timeout: float = 12.0) -> str | None:
    deadline = time.monotonic() + timeout
    pattern = re.compile(rf"I: Bus=0003 Vendor={vendor} Product={product}.*?H: Handlers=([^\n]*)", re.S)
    while time.monotonic() < deadline:
        with open("/proc/bus/input/devices") as devices:
            match = pattern.search(devices.read())
        if match:
            node = next(handler for handler in match.group(1).split() if handler.startswith("event"))
            return f"/dev/input/{node}"
        time.sleep(0.25)
    return None


def adb_is_back(timeout: float = 12.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if "18d1:d002" in subprocess.run(["lsusb"], capture_output=True, text=True).stdout:
            return True
        time.sleep(0.5)
    return False


def read_events(path: str, action: Callable[[], object], settle: float = 0.6) -> list[Event]:
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    try:
        action()
        events: list[Event] = []
        deadline = time.monotonic() + settle
        while time.monotonic() < deadline:
            ready, _, _ = select.select([fd], [], [], 0.05)
            if ready:
                try:
                    data = os.read(fd, 24 * 64)
                except OSError:
                    break
                for offset in range(0, len(data) - 23, 24):
                    _, _, kind, code, value = struct.unpack("qqHHi", data[offset : offset + 24])
                    if kind in (EV_KEY, EV_ABS):
                        events.append((kind, code, value))
        return events
    finally:
        os.close(fd)


def expect(name: str, condition: bool, detail: str = "") -> None:
    print(f"  {'ok  ' if condition else 'FAIL'} {name}" + (f": {detail}" if detail and not condition else ""))
    if not condition:
        failures.append(name)


def press(code: int) -> Callable[[], object]:
    def tap() -> None:
        _ = brick(
            f"/tmp/pocket-controller-inject {PAD} key:{code:#x}:1 && sleep 0.1 && "
            f"/tmp/pocket-controller-inject {PAD} key:{code:#x}:0"
        )

    return tap


STATE = "/tmp/pocket-controller-e2e-state"


def start(mode: str, saved: dict[str, str] | None = None) -> None:
    seed = "".join(f" && printf {value} > {STATE}/{name}" for name, value in (saved or {}).items())
    _ = brick(f"rm -rf {STATE} && mkdir -p {STATE}{seed}")
    _ = brick(
        "rm -f /tmp/pocket-controller.log /tmp/presenter.log; "
        f"nohup /tmp/pocket-controller --mode {mode} --state-dir {STATE} --presenter /tmp/fake-presenter "
        ">/tmp/pocket-controller.log 2>&1 </dev/null &"
    )


def quit_with_chord() -> None:
    inject(f"key:{BTN_SELECT:#x}:1", f"key:{BTN_START:#x}:1")
    time.sleep(2.5)
    inject(f"key:{BTN_SELECT:#x}:0", f"key:{BTN_START:#x}:0")


def scenario(
    mode: str,
    vendor: str,
    product: str,
    checks: Callable[[str], None],
    label: str | None = None,
    saved: dict[str, str] | None = None,
) -> None:
    print(f"{label or mode}:")
    start(mode, saved)
    node = host_device(vendor, product)
    expect(f"laptop sees {vendor}:{product}", node is not None)
    if node:
        time.sleep(1.0)
        checks(node)
        events = read_events(node, quit_with_chord, settle=0.3)
        leaked = [event for event in events if event[1] in (BTN_SELECT, BTN_START)]
        expect("exit chord is not sent to the laptop", not leaked, str(leaked))
    else:
        _ = brick("kill -TERM $(pidof pocket-controller)")
    time.sleep(1.0)
    expect("pocket-controller quit", brick("pidof pocket-controller") == "", brick("tail -n 5 /tmp/pocket-controller.log"))
    expect("no presenter is left running", brick("pgrep -fx 'sleep 100000'") == "")
    expect("ADB is given back", adb_is_back())
    gadget = brick("cat /sys/kernel/config/usb_gadget/g1/idProduct; ls /sys/kernel/config/usb_gadget/g1/configs/c.1")
    expect("g1 is restored", "0xd002" in gadget and "ffs.adb" in gadget, gadget)


def screens() -> str:
    return brick("cat /tmp/presenter.log")


def xbox_checks(node: str) -> None:
    events = read_events(node, press(PRINTED_A))
    expect("A press and release", (EV_KEY, BTN_A, 1) in events and (EV_KEY, BTN_A, 0) in events, str(events))
    events = read_events(node, press(BTN_MODE))
    expect("guide button", (EV_KEY, BTN_MODE, 1) in events, str(events))
    events = read_events(node, lambda: (inject(f"abs:{ABS_HAT0Y:#x}:-1"), time.sleep(0.15), inject(f"abs:{ABS_HAT0Y:#x}:0")))
    expect("dpad up", (EV_ABS, ABS_HAT0Y, -1) in events, str(events))
    events = read_events(node, lambda: (inject(f"abs:{ABS_Z:#x}:255"), time.sleep(0.15), inject(f"abs:{ABS_Z:#x}:0")))
    expect("left trigger", (EV_ABS, ABS_Z, 255) in events, str(events))
    log = brick("cat /tmp/pocket-controller.log")
    expect("xbox driver detected", "XboxDriverSpoke" in log, log)
    expect("screen says connected as xbox", "Connected as an Xbox 360 controller" in screens(), screens())


def gamepad_checks(node: str) -> None:
    events = read_events(node, press(PRINTED_A))
    expect("A press and release", (EV_KEY, BTN_A, 1) in events and (EV_KEY, BTN_A, 0) in events, str(events))
    events = read_events(node, lambda: (inject(f"abs:{ABS_HAT0X:#x}:-1"), time.sleep(0.15), inject(f"abs:{ABS_HAT0X:#x}:0")))
    expect("dpad left", (EV_ABS, ABS_HAT0X, -1) in events, str(events))
    expect("screen says connected as gamepad", "Connected as a generic gamepad" in screens(), screens())


def swapped_shoulder_checks(node: str) -> None:
    events = read_events(node, press(BTN_TL))
    expect("L1 sends the left trigger", (EV_ABS, ABS_Z, 255) in events, str(events))
    expect("L1 does not send LB", not any(event[1] == BTN_TL for event in events), str(events))
    events = read_events(node, lambda: (inject(f"abs:{ABS_Z:#x}:255"), time.sleep(0.15), inject(f"abs:{ABS_Z:#x}:0")))
    expect("L2 sends LB", (EV_KEY, BTN_TL, 1) in events and (EV_KEY, BTN_TL, 0) in events, str(events))


def shoulders_scenario() -> None:
    scenario("xbox", "045e", "028e", swapped_shoulder_checks, label="swapped shoulders", saved={"shoulders": "swapped"})


def hat(axis: int, value: int) -> None:
    _ = brick(
        f"/tmp/pocket-controller-inject {PAD} abs:{axis:#x}:{value} && sleep 0.1 && "
        f"/tmp/pocket-controller-inject {PAD} abs:{axis:#x}:0"
    )
    time.sleep(0.1)


def open_settings() -> None:
    inject(f"key:{BTN_MODE:#x}:1", f"key:{BTN_START:#x}:1")
    time.sleep(0.1)
    inject(f"key:{BTN_START:#x}:0", f"key:{BTN_MODE:#x}:0")
    time.sleep(0.3)


def settings_scenario() -> None:
    print("settings:")
    start("xbox")
    node = host_device("045e", "028e")
    expect("laptop sees 045e:028e", node is not None)
    if node:
        time.sleep(1.0)
        events = read_events(node, lambda: (open_settings(), press(PRINTED_A)()))
        pressed = [event for event in events if event[0] == EV_KEY and event[2] == 1]
        expect("settings keep buttons from the laptop", not pressed, str(events))
        log = brick("cat /tmp/pocket-controller.log")
        expect("settings open on the face buttons row", "cursor: Face" in log, log[-400:])
        for _ in range(5):
            hat(ABS_HAT0Y, 1)
        hat(ABS_HAT0X, 1)
        hat(ABS_HAT0X, 1)
        log = brick("cat /tmp/pocket-controller.log")
        expect("the mode row shows Generic before it applies", "mode: Gamepad, current_mode: Xbox" in log, log[-400:])
        press(BTN_A)()
        gamepad = host_device("1d6b", "0105")
        expect("closing settings switches the laptop to Generic", gamepad is not None)
        saved = brick(f"cat {STATE}/mode")
        expect("the new mode is saved", saved.strip() == "gamepad", saved)
        if gamepad:
            time.sleep(1.0)
            _ = read_events(gamepad, quit_with_chord, settle=0.3)
    else:
        _ = brick("kill -TERM $(pidof pocket-controller)")
    time.sleep(1.0)
    expect("pocket-controller quit", brick("pidof pocket-controller") == "", brick("tail -n 5 /tmp/pocket-controller.log"))
    expect("ADB is given back", adb_is_back())
    _ = brick(f"rm -rf {STATE}")


def main() -> None:
    if brick("pidof pocket-controller"):
        sys.exit("pocket-controller is already running on the Brick: quit it there, or wait for it to quit, then run this again")
    if "configured" not in brick("cat /sys/class/udc/*/state"):
        sys.exit("the Brick is not connected to this computer as a USB device (use its bottom port and a USB-A port here)")
    _ = subprocess.run(
        ["scp", "-q", "-i", KEY, "-o", "BatchMode=yes", "-o", "IdentitiesOnly=yes", "-o", "LogLevel=ERROR",
         f"{RELEASE}/pocket-controller", f"{RELEASE}/pocket-controller-inject", "nix/fake-presenter.sh", f"{HOST}:/tmp/"],
        check=True,
    )
    _ = brick("mv /tmp/fake-presenter.sh /tmp/fake-presenter && chmod +x /tmp/fake-presenter")
    scenario("xbox", "045e", "028e", xbox_checks)
    scenario("gamepad", "1d6b", "0105", gamepad_checks)
    scenario("automatic", "045e", "028e", xbox_checks)
    shoulders_scenario()
    settings_scenario()
    print("all passed" if not failures else f"{len(failures)} failed: {', '.join(failures)}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
