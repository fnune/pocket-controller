import time
from collections.abc import Callable

BTN_A, BTN_SELECT, BTN_START, BTN_MODE = 0x130, 0x13A, 0x13B, 0x13C
PRINTED_A = 0x131
ABS_Z, ABS_HAT0X, ABS_HAT0Y = 0x02, 0x10, 0x11
EV_KEY, EV_ABS = 1, 3
G1 = "/sys/kernel/config/usb_gadget/g1"
XBOX = "Microsoft X-Box 360 pad"
GAMEPAD = "Pocket Controller Gamepad"

machine.wait_for_unit("multi-user.target")
machine.succeed("modprobe -a dummy_hcd libcomposite usb_f_fs usb_f_acm uinput xpad")
machine.succeed("mountpoint -q /sys/kernel/config || mount -t configfs none /sys/kernel/config")
machine.succeed("test -d /sys/kernel/config/usb_gadget")
machine.succeed("systemd-run --unit fake-pad vm-helpers fake-pad")
machine.wait_until_succeeds("vm-helpers find 'TRIMUI Player1'")
pad = machine.succeed("vm-helpers find 'TRIMUI Player1'").strip()

machine.succeed(
    f"mkdir -p {G1}/strings/0x409 {G1}/configs/c.1 {G1}/functions/acm.usb0",
    f"echo 0x18d1 > {G1}/idVendor",
    f"echo 0xd002 > {G1}/idProduct",
    f"echo TRIMUI > {G1}/strings/0x409/manufacturer",
    f"echo 'TRIMUI ADB' > {G1}/strings/0x409/product",
    f"echo 2c000c81b5c28711c9d > {G1}/strings/0x409/serialnumber",
    f"ln -s {G1}/functions/acm.usb0 {G1}/configs/c.1/",
    f"echo dummy_udc.0 > {G1}/UDC",
)
machine.wait_until_succeeds("lsusb | grep -q 18d1:d002")
firmware_before = machine.succeed(f"cat {G1}/idVendor {G1}/idProduct {G1}/strings/0x409/product {G1}/UDC; ls {G1}/configs/c.1")


def inject(*events: str) -> None:
    machine.succeed(f"pocket-controller-inject {pad} " + " ".join(events))


def press(code: int) -> None:
    inject(f"key:{code:#x}:1")
    inject(f"key:{code:#x}:0")


Event = tuple[int, int, int]


def parse_event(line: str) -> Event:
    kind, code, value = (int(part) for part in line.split())
    return (kind, code, value)


def recorded(device: str, action: Callable[[], object], seconds: float = 1.5) -> list[Event]:
    machine.succeed(f"systemd-run --unit recorder --collect --setenv=PATH=/run/current-system/sw/bin sh -c \"vm-helpers record '{device}' {seconds} > /tmp/recorded\"")
    time.sleep(0.5)
    action()
    machine.wait_until_fails("systemctl is-active --quiet recorder")
    lines = machine.succeed("cat /tmp/recorded").split("\n")
    return [parse_event(line) for line in lines if line.strip()]


def start(*arguments: str) -> None:
    machine.succeed("rm -f /tmp/presenter.log /tmp/pocket-controller.log")
    machine.succeed(
        "systemd-run --unit pocket-controller --collect --setenv=PATH=/run/current-system/sw/bin sh -c "
        f"'pocket-controller {' '.join(arguments)} --presenter fake-presenter > /tmp/pocket-controller.log 2>&1'"
    )


def hold_exit_chord() -> None:
    inject(f"key:{BTN_SELECT:#x}:1", f"key:{BTN_START:#x}:1")
    time.sleep(2.5)
    inject(f"key:{BTN_SELECT:#x}:0", f"key:{BTN_START:#x}:0")


def quit_and_check_firmware_restored(device: str) -> None:
    events = recorded(device, hold_exit_chord, seconds=3.5)
    assert not [event for event in events if event[1] in (BTN_SELECT, BTN_START)], f"exit chord reached the host: {events}"
    machine.wait_until_fails("systemctl is-active --quiet pocket-controller")
    machine.wait_until_succeeds("lsusb | grep -q 18d1:d002")
    firmware_after = machine.succeed(f"cat {G1}/idVendor {G1}/idProduct {G1}/strings/0x409/product {G1}/UDC; ls {G1}/configs/c.1")
    assert firmware_after == firmware_before, f"g1 changed:\n{firmware_before}\n{firmware_after}"
    machine.succeed("test ! -e /tmp/pocket-controller.port")
    machine.fail("pgrep -fx 'sleep 100000'")


def wait_for_device(name: str) -> None:
    try:
        machine.wait_until_succeeds(f"vm-helpers find '{name}'", timeout=30)
    except Exception:
        print(machine.execute("cat /tmp/pocket-controller.log; echo ---; lsusb; echo ---; dmesg | tail -n 40")[1])
        raise
    machine.sleep(1)


with subtest("xbox mode reaches xpad and gives the port back"):
    start("--mode xbox", "--state-dir /tmp/state-xbox")
    wait_for_device(XBOX)
    events = recorded(XBOX, lambda: press(PRINTED_A))
    assert (EV_KEY, BTN_A, 1) in events and (EV_KEY, BTN_A, 0) in events, events
    events = recorded(XBOX, lambda: (inject(f"abs:{ABS_Z:#x}:255"), inject(f"abs:{ABS_Z:#x}:0")))
    assert (EV_ABS, ABS_Z, 255) in events, events
    events = recorded(XBOX, lambda: press(BTN_MODE))
    assert (EV_KEY, BTN_MODE, 1) in events, events
    machine.wait_until_succeeds("grep -q XboxDriverSpoke /tmp/pocket-controller.log")
    machine.succeed("grep -q 'Connected as an Xbox 360 controller' /tmp/presenter.log")
    quit_and_check_firmware_restored(XBOX)

with subtest("gamepad mode reaches hid-generic"):
    start("--mode gamepad", "--state-dir /tmp/state-gamepad")
    wait_for_device(GAMEPAD)
    events = recorded(GAMEPAD, lambda: (inject(f"abs:{ABS_HAT0X:#x}:-1"), inject(f"abs:{ABS_HAT0X:#x}:0")))
    assert (EV_ABS, ABS_HAT0X, -1) in events, events
    quit_and_check_firmware_restored(GAMEPAD)

with subtest("automatic mode stays xbox when xpad answers"):
    start("--mode automatic", "--state-dir /tmp/state-automatic")
    wait_for_device(XBOX)
    machine.sleep(3)
    machine.succeed(f"vm-helpers find '{XBOX}'")
    machine.succeed("grep -q 'Connected(Xbox)' /tmp/pocket-controller.log")
    quit_and_check_firmware_restored(XBOX)

with subtest("automatic mode falls back to a gamepad without an xbox driver"):
    machine.succeed("rmmod xpad", "echo 'blacklist xpad' > /etc/modprobe.d/no-xpad.conf")
    start("--mode automatic", "--state-dir /tmp/state-fallback")
    wait_for_device(GAMEPAD)
    machine.succeed("grep -q 'Connected(Gamepad)' /tmp/pocket-controller.log")
    quit_and_check_firmware_restored(GAMEPAD)
    machine.succeed("rm /etc/modprobe.d/no-xpad.conf", "modprobe xpad")

with subtest("the brick's one-interrupt layout still reaches xpad"):
    machine.succeed("mkdir -p /tmp/state-brick", "printf one-interrupt > /tmp/state-brick/endpoints")
    start("--mode xbox", "--state-dir /tmp/state-brick")
    wait_for_device(XBOX)
    events = recorded(XBOX, lambda: press(PRINTED_A))
    assert (EV_KEY, BTN_A, 1) in events, events
    machine.fail("grep -q TwoInterrupt /tmp/pocket-controller.log")
    quit_and_check_firmware_restored(XBOX)

with subtest("a crash is repaired by pocket-controller recover"):
    start("--mode xbox", "--state-dir /tmp/state-crash")
    wait_for_device(XBOX)
    machine.succeed("systemctl kill --signal=KILL pocket-controller")
    machine.wait_until_fails("systemctl is-active --quiet pocket-controller")
    machine.succeed("pkill -fx 'sleep 100000' || true")
    machine.succeed("test -e /tmp/pocket-controller.port")
    machine.succeed("pocket-controller recover")
    machine.wait_until_succeeds("lsusb | grep -q 18d1:d002")
    firmware_after = machine.succeed(f"cat {G1}/idVendor {G1}/idProduct {G1}/strings/0x409/product {G1}/UDC; ls {G1}/configs/c.1")
    assert firmware_after == firmware_before, f"g1 changed:\n{firmware_before}\n{firmware_after}"
