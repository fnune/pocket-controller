use std::io::Write;
use std::process::ExitCode;

use pocket_controller::evdev::{EV_ABS, EV_KEY, Event};

const USAGE: &str =
    "usage: pocket-controller-inject /dev/input/eventN key:CODE:VALUE|abs:CODE:VALUE ...";

fn parse_event(text: &str) -> Result<Event, String> {
    let parts: Vec<&str> = text.split(':').collect();
    let [kind, code, value] = parts.as_slice() else {
        return Err(format!("expected kind:code:value, got {text:?}"));
    };
    let kind = match *kind {
        "key" => EV_KEY,
        "abs" => EV_ABS,
        other => return Err(format!("unknown event kind {other:?}")),
    };
    let parse_number = |text: &str| -> Result<i64, String> {
        let parsed = match text.strip_prefix("0x") {
            Some(hex) => i64::from_str_radix(hex, 16),
            None => text.parse(),
        };
        parsed.map_err(|error| format!("bad number {text:?}: {error}"))
    };
    Ok(Event {
        kind,
        code: u16::try_from(parse_number(code)?).map_err(|error| error.to_string())?,
        value: i32::try_from(parse_number(value)?).map_err(|error| error.to_string())?,
    })
}

fn run() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let device = arguments.next().ok_or(USAGE)?;
    let mut bytes = Vec::new();
    for argument in arguments {
        bytes.extend_from_slice(&parse_event(&argument)?.encode());
    }
    bytes.extend_from_slice(&Event::sync().encode());
    std::fs::OpenOptions::new()
        .write(true)
        .open(&device)
        .and_then(|mut file| file.write_all(&bytes))
        .map_err(|error| format!("could not write to {device}: {error}"))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
