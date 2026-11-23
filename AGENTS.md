# Engineering principles

## Calm tool

- Never leave the handheld worse off. The USB port always goes back to the firmware's gadget, exactly as it was, on every exit path: normal quit, error, signal and crash (through the marker file and `pocket-controller recover`).
- Screen text is plain and short, says what to do next, and never blames the user.
- A feature earns its place or it does not ship.

## Testability through injection

- Volatile dependencies sit behind traits owned by the code that uses them: configfs through `GadgetFs`, time as an `Instant` or `Duration` passed in, the settle delay in `Layout`.
- Decisions are pure and unit-tested: `control::Controller` takes inputs and time and returns actions, `chord`, `evdev`, `xinput`, `hid` and `gadget` take values and return values.
- `main.rs` is the composition root. Nothing else knows about `RealFs`, threads or signals.
- Tests use hand-written fakes (`gadget::fake::FakeFs`), never mocking libraries.

## Strict types and boundaries

- `cargo clippy --all-targets -- -D warnings` passes. No `unwrap` or `expect` outside tests.
- Everything read from the system is parsed into types where it enters: `/proc/bus/input/devices`, evdev events, FunctionFS events, setup requests, the gadget marker and the state directory.
- Errors carry context: what was attempted, on which path, and what failed.

## Code comments

- Code is self-evident through naming and structure. A test named after the reason replaces a comment.

## Dependencies

- `std` and `libc` only. Under 50 lines, write it. Every new dependency needs a reason in the commit message.
- Downloaded binaries (`minui-presenter`) are pinned by version and SHA-256 in the `justfile`.
- `Cargo.lock` is committed.

## Hardware facts we rely on

- The TrimUI Brick's kernel allows one configfs gadget, so pocket-controller borrows `g1` instead of creating its own.
- Its USB device controller has one interrupt endpoint, so the Xbox 360 function declares its OUT endpoint as bulk at full speed and interrupt at high speed.
- Its controller wedges if a gadget is unbound and bound again within milliseconds while a host enumerates, so every bind waits `Layout::settle` after the last unbind.
- Only its bottom USB-C port can be a device, and only with a USB-A host or a host that supplies power.

## Checks

- Before committing: `just check` (format, clippy, unit tests).
- With a Brick on the bottom port and a USB-A port here: `just e2e`.
- Development environment: `nix develop`. The toolchain is pinned in `rust-toolchain.toml`.

## Commits and writing

- Conventional Commits, imperative mood, subject under 72 characters, body explains why.
- No value-judgement words ("improve", "better", "robust"), no vague qualifiers, no AI attribution.
- Sentence-case headings, no em-dashes.
