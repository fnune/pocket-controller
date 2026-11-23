use std::time::{Duration, Instant};

use crate::xinput::{self, State};

pub const EXIT_CHORD: u16 = xinput::BACK | xinput::START;
pub const EXIT_HOLD: Duration = Duration::from_secs(2);
pub const BAR_FULL_AFTER: Duration = Duration::from_millis(1700);

#[derive(Debug, Default)]
pub struct ExitChord {
    held_since: Option<Instant>,
    withholding: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChordOutcome {
    pub sent: State,
    pub exit: bool,
    pub progress: Option<u8>,
}

const PROGRESS_STEP: u128 = 5;

impl ExitChord {
    pub fn update(&mut self, held: State, now: Instant) -> ChordOutcome {
        let chord_held = held.holds(EXIT_CHORD);
        if chord_held {
            self.withholding = true;
            self.held_since.get_or_insert(now);
        } else {
            self.held_since = None;
            if held.buttons & EXIT_CHORD == 0 {
                self.withholding = false;
            }
        }
        let mut sent = held;
        if self.withholding {
            sent.buttons &= !EXIT_CHORD;
        }
        let held_for = self.held_since.map(|since| now.duration_since(since));
        let exit = held_for.is_some_and(|held_for| held_for >= EXIT_HOLD);
        let progress = held_for.map(|held_for| {
            let percent = (held_for.as_millis() * 100 / BAR_FULL_AFTER.as_millis()).min(100);
            (percent / PROGRESS_STEP * PROGRESS_STEP) as u8
        });
        ChordOutcome {
            sent,
            exit,
            progress,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding(buttons: u16) -> State {
        State {
            buttons,
            ..State::default()
        }
    }

    #[test]
    fn select_alone_reaches_the_host() {
        let mut chord = ExitChord::default();
        let outcome = chord.update(holding(xinput::BACK), Instant::now());
        assert_eq!(outcome.sent.buttons, xinput::BACK);
        assert!(!outcome.exit);
    }

    #[test]
    fn chord_is_withheld_from_the_host_while_held() {
        let mut chord = ExitChord::default();
        let outcome = chord.update(holding(EXIT_CHORD | xinput::A), Instant::now());
        assert_eq!(outcome.sent.buttons, xinput::A);
    }

    #[test]
    fn holding_the_chord_for_two_seconds_exits() {
        let mut chord = ExitChord::default();
        let start = Instant::now();
        assert!(!chord.update(holding(EXIT_CHORD), start).exit);
        assert!(
            !chord
                .update(holding(EXIT_CHORD), start + Duration::from_millis(1999))
                .exit
        );
        assert!(chord.update(holding(EXIT_CHORD), start + EXIT_HOLD).exit);
    }

    #[test]
    fn progress_fills_in_five_percent_steps_while_the_chord_is_held() {
        let mut chord = ExitChord::default();
        let start = Instant::now();
        assert_eq!(chord.update(holding(xinput::A), start).progress, None);
        assert_eq!(chord.update(holding(EXIT_CHORD), start).progress, Some(0));
        let at = |millis| chord_progress(&mut ExitChord::default(), start, millis);
        assert_eq!(at(1020), Some(60));
        assert_eq!(at(1690), Some(95));
        assert_eq!(at(2500), Some(100));
    }

    #[test]
    fn the_bar_is_full_for_a_moment_before_the_app_quits() {
        let start = Instant::now();
        let mut chord = ExitChord::default();
        let full = chord.update(holding(EXIT_CHORD), start).progress;
        assert_eq!(full, Some(0));
        let before_exit = chord.update(holding(EXIT_CHORD), start + Duration::from_millis(1800));
        assert_eq!((before_exit.progress, before_exit.exit), (Some(100), false));
    }

    fn chord_progress(chord: &mut ExitChord, start: Instant, millis: u64) -> Option<u8> {
        chord.update(holding(EXIT_CHORD), start);
        chord
            .update(holding(EXIT_CHORD), start + Duration::from_millis(millis))
            .progress
    }

    #[test]
    fn letting_go_restarts_the_hold() {
        let mut chord = ExitChord::default();
        let start = Instant::now();
        chord.update(holding(EXIT_CHORD), start);
        chord.update(holding(xinput::START), start + Duration::from_secs(1));
        assert!(
            !chord
                .update(holding(EXIT_CHORD), start + Duration::from_millis(2500))
                .exit
        );
    }

    #[test]
    fn half_released_chord_stays_withheld_until_both_are_up() {
        let mut chord = ExitChord::default();
        let start = Instant::now();
        chord.update(holding(EXIT_CHORD), start);
        let half = chord.update(holding(xinput::START), start + Duration::from_millis(100));
        assert_eq!(half.sent.buttons, 0);
        chord.update(holding(0), start + Duration::from_millis(200));
        let again = chord.update(holding(xinput::START), start + Duration::from_millis(300));
        assert_eq!(again.sent.buttons, xinput::START);
    }
}
