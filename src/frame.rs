use crate::control::{Diagnosis, Kind, LiveView, Mode, Screen, SheetView};
use crate::layer::{DpadMode, Turbo};
use crate::xinput;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub screen: Screen,
    pub hold: Option<u8>,
    pub busy: Option<u8>,
    pub picker: Option<Mode>,
    pub sheet: Option<SheetView>,
    pub live: Option<LiveView>,
}

const END: &str = "end";

fn diagnosis_name(diagnosis: Diagnosis) -> &'static str {
    match diagnosis {
        Diagnosis::None => "none",
        Diagnosis::NoComputer => "no-computer",
        Diagnosis::SetupStuck => "setup-stuck",
    }
}

fn parse_diagnosis(text: &str) -> Option<Diagnosis> {
    [
        Diagnosis::None,
        Diagnosis::NoComputer,
        Diagnosis::SetupStuck,
    ]
    .into_iter()
    .find(|diagnosis| diagnosis_name(*diagnosis) == text)
}

const TURBO_NAMES: [(u16, &str); 6] = [
    (xinput::A, "a"),
    (xinput::B, "b"),
    (xinput::X, "x"),
    (xinput::Y, "y"),
    (xinput::LEFT_SHOULDER, "lb"),
    (xinput::RIGHT_SHOULDER, "rb"),
];

fn turbo_names(turbo: Turbo) -> String {
    let mut names: Vec<&str> = TURBO_NAMES
        .iter()
        .filter(|(bit, _)| turbo.buttons & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    if turbo.left_trigger {
        names.push("lt");
    }
    if turbo.right_trigger {
        names.push("rt");
    }
    names.join(",")
}

fn parse_turbo(text: &str) -> Option<Turbo> {
    let mut turbo = Turbo::default();
    for name in text.split(',').filter(|name| !name.is_empty()) {
        match name {
            "lt" => turbo.left_trigger = true,
            "rt" => turbo.right_trigger = true,
            _ => turbo.buttons |= TURBO_NAMES.iter().find(|(_, known)| *known == name)?.0,
        }
    }
    Some(turbo)
}

fn parse_live(text: &str) -> Option<LiveView> {
    let mut parts = text.split(',');
    let buttons = parts.next()?.parse().ok()?;
    let flag = |part: Option<&str>| match part? {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    };
    let left_trigger = flag(parts.next())?;
    let right_trigger = flag(parts.next())?;
    let volume_up = flag(parts.next())?;
    let volume_down = flag(parts.next())?;
    let fn_switch = flag(parts.next())?;
    Some(LiveView {
        buttons,
        left_trigger,
        right_trigger,
        volume_up,
        volume_down,
        fn_switch,
        ..LiveView::default()
    })
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Xbox => "xbox",
        Kind::Gamepad => "gamepad",
    }
}

fn parse_kind(text: &str) -> Option<Kind> {
    [Kind::Xbox, Kind::Gamepad]
        .into_iter()
        .find(|kind| kind_name(*kind) == text)
}

impl Frame {
    pub fn serialize(&self) -> String {
        let mut lines = Vec::new();
        match &self.screen {
            Screen::Waiting(mode, diagnosis) => {
                lines.push("view=waiting".to_string());
                lines.push(format!("mode={}", mode.as_str()));
                lines.push(format!("diagnosis={}", diagnosis_name(*diagnosis)));
            }
            Screen::Connected(kind) => {
                lines.push("view=connected".to_string());
                lines.push(format!("kind={}", kind_name(*kind)));
            }
        }
        if let Some(progress) = self.hold {
            lines.push(format!("hold={progress}"));
        }
        if let Some(phase) = self.busy {
            lines.push(format!("busy={phase}"));
        }
        if let Some(selected) = self.picker {
            lines.push(format!("picker={}", selected.as_str()));
        }
        if let Some(live) = &self.live {
            lines.push(format!(
                "live={},{}",
                live.buttons,
                [
                    live.left_trigger,
                    live.right_trigger,
                    live.volume_up,
                    live.volume_down,
                    live.fn_switch
                ]
                .map(|on| if on { "1" } else { "0" })
                .join(",")
            ));
            lines.push(format!("live-turbo={}", turbo_names(live.turbo)));
            lines.push(format!("live-dpad={}", live.dpad.as_str()));
        }
        if let Some(sheet) = &self.sheet {
            lines.push("sheet=1".to_string());
            lines.push(format!("turbo={}", turbo_names(sheet.turbo)));
            lines.push(format!("dpad={}", sheet.dpad.as_str()));
            if let Some(note) = &sheet.note {
                lines.push(format!("note={}", note.replace('\n', " ")));
            }
        }
        lines.push(END.to_string());
        lines.join("\n") + "\n"
    }

    pub fn parse<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let (mut view, mut mode, mut diagnosis, mut kind, mut hold, mut busy) =
            (None, None, None, None, None, None);
        let mut picker = None;
        let mut sheet: Option<SheetView> = None;
        let mut live: Option<LiveView> = None;
        for line in lines {
            let (key, value) = line.split_once('=')?;
            match key {
                "view" => view = Some(value),
                "mode" => mode = Some(Mode::parse(value)?),
                "diagnosis" => diagnosis = Some(parse_diagnosis(value)?),
                "kind" => kind = Some(parse_kind(value)?),
                "hold" => {
                    hold = Some(
                        value
                            .parse::<u8>()
                            .ok()
                            .filter(|progress| *progress <= 100)?,
                    )
                }
                "busy" => busy = Some(value.parse::<u8>().ok()?),
                "picker" => picker = Some(Mode::parse(value)?),
                "live" => live = Some(parse_live(value)?),
                "live-turbo" => live.as_mut()?.turbo = parse_turbo(value)?,
                "live-dpad" => live.as_mut()?.dpad = DpadMode::parse(value)?,
                "sheet" => sheet = (value == "1").then(SheetView::default),
                "turbo" => sheet.as_mut()?.turbo = parse_turbo(value)?,
                "dpad" => sheet.as_mut()?.dpad = DpadMode::parse(value)?,
                "note" => sheet.as_mut()?.note = Some(value.to_string()),
                _ => return None,
            }
        }
        let screen = match view? {
            "waiting" => Screen::Waiting(mode?, diagnosis?),
            "connected" => Screen::Connected(kind?),
            _ => return None,
        };
        Some(Self {
            screen,
            hold,
            busy,
            picker,
            sheet,
            live,
        })
    }
}

pub struct FrameReader {
    pending: Vec<String>,
}

impl Default for FrameReader {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameReader {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    pub fn push_line(&mut self, line: &str) -> Option<Option<Frame>> {
        let line = line.trim_end();
        if line != END {
            self.pending.push(line.to_string());
            return None;
        }
        let frame = Frame::parse(self.pending.iter().map(String::as_str));
        self.pending.clear();
        Some(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(text: &str) -> Vec<Option<Frame>> {
        let mut reader = FrameReader::new();
        text.lines()
            .filter_map(|line| reader.push_line(line))
            .collect()
    }

    #[test]
    fn every_screen_survives_the_trip_to_the_helper() {
        let frames = [
            Frame {
                screen: Screen::Waiting(Mode::Automatic, Diagnosis::NoComputer),
                hold: None,
                busy: None,
                picker: None,
                sheet: None,
                live: None,
            },
            Frame {
                screen: Screen::Connected(Kind::Xbox),
                hold: Some(35),
                busy: None,
                picker: None,
                sheet: None,
                live: None,
            },
            Frame {
                screen: Screen::Connected(Kind::Gamepad),
                hold: Some(100),
                busy: None,
                picker: None,
                sheet: None,
                live: None,
            },
        ];
        for frame in frames {
            assert_eq!(read_all(&frame.serialize()), vec![Some(frame.clone())]);
        }
    }

    #[test]
    fn frames_arrive_one_per_end_line() {
        let text = Frame {
            screen: Screen::Connected(Kind::Xbox),
            hold: None,
            busy: None,
            picker: None,
            sheet: None,
            live: None,
        }
        .serialize()
            + &Frame {
                screen: Screen::Connected(Kind::Xbox),
                hold: None,
                busy: None,
                picker: None,
                sheet: None,
                live: None,
            }
            .serialize();
        assert_eq!(read_all(&text).len(), 2);
    }

    #[test]
    fn a_garbled_frame_is_reported_and_the_next_one_still_reads() {
        let text = "view=connected\nkind=toaster\nend\nview=connected\nkind=xbox\nend\n";
        assert_eq!(
            read_all(text),
            vec![
                None,
                Some(Frame {
                    screen: Screen::Connected(Kind::Xbox),
                    hold: None,
                    busy: None,
                    picker: None,
                    sheet: None,
                    live: None,
                })
            ]
        );
    }

    #[test]
    fn the_loading_phase_survives_the_trip() {
        let frame = Frame {
            screen: Screen::Waiting(Mode::Xbox, Diagnosis::None),
            hold: None,
            busy: Some(2),
            picker: None,
            sheet: None,
            live: None,
        };
        assert_eq!(read_all(&frame.serialize()), vec![Some(frame.clone())]);
    }

    #[test]
    fn the_picker_selection_survives_the_trip() {
        let frame = Frame {
            screen: Screen::Waiting(Mode::Automatic, Diagnosis::None),
            hold: None,
            busy: None,
            picker: Some(Mode::Gamepad),
            sheet: None,
            live: None,
        };
        assert_eq!(read_all(&frame.serialize()), vec![Some(frame.clone())]);
    }

    #[test]
    fn the_cheat_sheet_survives_the_trip() {
        let frame = Frame {
            screen: Screen::Connected(Kind::Xbox),
            hold: None,
            busy: None,
            picker: None,
            sheet: Some(SheetView {
                note: Some("Turbo A: on".to_string()),
                turbo: Turbo {
                    buttons: xinput::B | xinput::RIGHT_SHOULDER,
                    left_trigger: true,
                    right_trigger: false,
                },
                dpad: DpadMode::LeftStick,
            }),
            live: Some(LiveView {
                buttons: xinput::A | xinput::DPAD_UP,
                left_trigger: false,
                right_trigger: true,
                volume_up: false,
                volume_down: true,
                fn_switch: true,
                turbo: Turbo {
                    buttons: xinput::B,
                    ..Turbo::default()
                },
                dpad: DpadMode::RightStick,
            }),
        };
        assert_eq!(read_all(&frame.serialize()), vec![Some(frame.clone())]);
    }

    #[test]
    fn hold_above_one_hundred_percent_is_rejected() {
        assert_eq!(
            Frame::parse(["view=connected", "kind=xbox", "hold=101"]),
            None
        );
    }
}
