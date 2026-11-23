use super::*;

fn brick() -> Device {
    Device::parse(include_str!("../../assets/devices/trimui-brick/manifest")).unwrap()
}

fn device_layers() -> Vec<String> {
    let mut layers: Vec<String> = [
        "body",
        "leaders",
        "choice",
        "press-volume-up",
        "press-volume-down",
        "fn-on",
        "stick-well",
        "stick-well-l",
        "stick-well-r",
        "stick-thumb",
    ]
    .map(String::from)
    .to_vec();
    layers.extend(PLAIN_CONTROLS.map(|(_, name)| name.to_string()));
    for shortcut in turbo_shortcuts() {
        let key = shortcut.printed.to_lowercase();
        layers.extend(["press", "turbo", "turbo-press"].map(|state| format!("{state}-{key}")));
    }
    layers
}
use pocket_controller::control::{DebugView, SheetView, UsbLink};
use pocket_controller::settings::{
    MODE_CHANGE_WARNING, RUMBLE_UNAVAILABLE, Row, Settings, SettingsView, mode_detail,
};
use pocket_controller::xinput::{EndpointBudget, State};

const WIDTH: i32 = 1024;
const HEIGHT: i32 = 768;

fn fake_measure(role: Role, text: &str) -> i32 {
    let size = match role {
        Role::Headline => 42,
        Role::Body => 30,
        Role::Small | Role::Hint => 24,
        Role::Detail => 20,
        Role::Value => 18,
        Role::Tag => 16,
        Role::SheetLabel => 22,
        Role::Caption => 18,
    };
    text.chars().count() as i32 * size * 55 / 100
}

fn draw(screen: Screen, hold: Option<u8>, busy: Option<u8>) -> Vec<Op> {
    let frame = Frame {
        screen,
        hold,
        busy,
        settings: None,
        debug: None,
        sheet: None,
        live: None,
    };
    layout(&frame, WIDTH, HEIGHT, &fake_measure, &brick())
}

fn draw_overlay(
    settings: Option<SettingsView>,
    debug: Option<DebugView>,
    hold: Option<u8>,
) -> Vec<Op> {
    let frame = Frame {
        screen: Screen::Connected(Kind::Xbox),
        hold,
        busy: None,
        settings,
        debug,
        sheet: None,
        live: Some(LiveView {
            buttons: xinput::B,
            ..LiveView::default()
        }),
    };
    layout(&frame, WIDTH, HEIGHT, &fake_measure, &brick())
}

fn settings_view(cursor: Row) -> SettingsView {
    SettingsView {
        cursor,
        settings: Settings::default(),
        mode: Mode::Gamepad,
        current_mode: Mode::Xbox,
        kind: Kind::Xbox,
        connected: true,
    }
}

fn debug_view() -> DebugView {
    DebugView {
        link: UsbLink::Configured,
        kind: Kind::Xbox,
        connected: true,
        budget: Some(EndpointBudget::OneInterrupt),
        xbox_driver: true,
        rumble: 128,
        sent: State {
            buttons: xinput::A | xinput::DPAD_UP,
            left_x: -32767,
            ..State::default()
        },
    }
}

fn texts(ops: &[Op]) -> Vec<String> {
    ops.iter()
        .filter_map(|op| match op {
            Op::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn circles_painted(ops: &[Op], paint: Paint) -> usize {
    ops.iter()
        .filter(|op| matches!(op, Op::Circle { paint: p, .. } if *p == paint))
        .count()
}

const DIAGNOSES: [Diagnosis; 3] = [
    Diagnosis::None,
    Diagnosis::NoComputer,
    Diagnosis::SetupStuck,
];

#[test]
fn every_diagnosis_gets_its_own_headline_and_instruction() {
    for diagnosis in DIAGNOSES {
        let all = texts(&draw(
            Screen::Waiting(Mode::Automatic, diagnosis),
            None,
            None,
        ));
        let (headline, instruction) = waiting_lines(diagnosis);
        assert!(all.contains(&headline.to_string()) && all.contains(&instruction.to_string()));
    }
}

fn images(ops: &[Op]) -> Vec<&str> {
    ops.iter()
        .filter_map(|op| match op {
            Op::Image { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn each_waiting_state_has_its_own_illustration_and_problems_show_a_warning() {
    let expected = [
        (Diagnosis::None, "link-unplugged", "icon-waiting"),
        (Diagnosis::NoComputer, "link-unrecognised", "icon-warning"),
        (Diagnosis::SetupStuck, "link-stuck", "icon-warning"),
    ];
    for (diagnosis, link, icon) in expected {
        let ops = draw(Screen::Waiting(Mode::Xbox, diagnosis), None, None);
        let shown = images(&ops);
        assert!(
            shown.contains(&link) && shown.contains(&icon),
            "{diagnosis:?}: {shown:?}"
        );
    }
}

#[test]
fn every_image_the_layout_uses_is_loaded_by_the_helper() {
    let screens = [
        Screen::Waiting(Mode::Automatic, Diagnosis::None),
        Screen::Waiting(Mode::Xbox, Diagnosis::NoComputer),
        Screen::Waiting(Mode::Gamepad, Diagnosis::SetupStuck),
        Screen::Connected(Kind::Xbox),
    ];
    for screen in screens {
        for name in images(&draw(screen, None, None)) {
            assert!(
                IMAGES.contains(&name) || brick().image(name).is_some(),
                "{name} is in neither IMAGES nor the device manifest"
            );
        }
    }
}

#[test]
fn every_listed_image_ships_in_the_assets_folder() {
    for name in IMAGES {
        let path = format!("{}/assets/{name}.png", env!("CARGO_MANIFEST_DIR"));
        assert!(std::path::Path::new(&path).exists(), "missing {path}");
    }
}

#[test]
fn the_mode_chip_names_the_mode_with_one_word_and_its_glyph() {
    for mode in [Mode::Automatic, Mode::Xbox, Mode::Gamepad] {
        let ops = draw(Screen::Waiting(mode, Diagnosis::None), None, None);
        let (glyph, label) = mode_chip(mode);
        assert!(texts(&ops).contains(&label.to_string()), "{mode:?}");
        assert!(
            ops.iter()
                .any(|op| matches!(op, Op::Image { name, .. } if *name == glyph))
        );
        assert!(!texts(&ops).contains(&mode_detail(mode).to_string()));
    }
}

fn draw_sheet(view: SheetView) -> Vec<Op> {
    let frame = Frame {
        screen: Screen::Connected(Kind::Xbox),
        hold: None,
        busy: None,
        settings: None,
        debug: None,
        sheet: Some(view),
        live: None,
    };
    layout(&frame, WIDTH, HEIGHT, &fake_measure, &brick())
}

#[test]
fn holding_menu_labels_the_same_brick_and_colours_every_turbo_button() {
    use pocket_controller::layer::Turbo;
    let ops = draw_sheet(SheetView {
        note: Some("Turbo A: on".to_string()),
        turbo: Turbo {
            buttons: xinput::B | xinput::RIGHT_SHOULDER,
            left_trigger: true,
            right_trigger: false,
        },
        dpad: DpadMode::LeftStick,
    });
    let all = texts(&ops);
    assert!(all.contains(&"Turbo A: on".to_string()));
    assert!(
        !all.contains(&"Connected".to_string()),
        "the sheet replaces the headline row"
    );
    let rings: Vec<(i32, i32)> = ops
        .iter()
        .filter_map(|op| match op {
            Op::Image { name, x, y } if name.starts_with("turbo") => Some((*x, *y)),
            _ => None,
        })
        .collect();
    assert_eq!(
        rings,
        vec![(626, 396), (416, 68), (616, 68)],
        "printed A, L2 and R1"
    );
    assert!(images(&ops).contains(&"stick-well-l"));
    assert!(images(&ops).contains(&"choice"));
    assert!(images(&ops).contains(&"leaders"));
    assert!(all.contains(&"Left analog stick".to_string()));
}

fn draw_connected(live: LiveView) -> Vec<Op> {
    let frame = Frame {
        screen: Screen::Connected(Kind::Gamepad),
        hold: None,
        busy: None,
        settings: None,
        debug: None,
        sheet: None,
        live: Some(live),
    };
    layout(&frame, WIDTH, HEIGHT, &fake_measure, &brick())
}

fn placed<'o>(ops: &'o [Op], prefix: &str) -> Vec<(&'o str, i32, i32)> {
    ops.iter()
        .filter_map(|op| match op {
            Op::Image { name, x, y } if name.starts_with(prefix) => Some((name.as_str(), *x, *y)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_connected_screen_names_the_mode_and_lights_the_pressed_buttons() {
    let ops = draw_connected(LiveView {
        buttons: xinput::B | xinput::DPAD_UP | xinput::GUIDE,
        left_trigger: true,
        ..LiveView::default()
    });
    let all = texts(&ops);
    let headline: Vec<&String> = all
        .iter()
        .filter(|text| ["Connected", "as", "Generic"].contains(&text.as_str()))
        .collect();
    assert_eq!(
        headline,
        ["Connected", "as", "Generic"],
        "Connected as Generic, in that order"
    );
    assert!(images(&ops).contains(&"mode-gamepad-48-off"));
    assert_eq!(
        placed(&ops, "press"),
        vec![
            ("press-dpad-up", 403, 377),
            ("press-menu", 442, 473),
            ("press-a", 626, 396),
            ("press-l2", 416, 68),
        ],
        "printed A, D-pad up, MENU and L2"
    );
}

#[test]
fn the_side_controls_show_volume_presses_and_the_fn_position() {
    let ops = draw_connected(LiveView {
        volume_down: true,
        fn_switch: true,
        ..LiveView::default()
    });
    assert!(images(&ops).contains(&"press-volume-down"));
    assert!(images(&ops).contains(&"fn-on"));
    assert!(!images(&ops).contains(&"press-volume-up"));
}

#[test]
fn the_stick_thumb_moves_where_the_emulated_stick_points() {
    assert_eq!(thumb_offset(0, 30), (0, 0), "centred at rest");
    assert_eq!(thumb_offset(xinput::DPAD_UP, 30), (0, -30));
    assert_eq!(thumb_offset(xinput::DPAD_LEFT, 30), (-30, 0));
    assert_eq!(
        thumb_offset(xinput::DPAD_UP | xinput::DPAD_RIGHT, 30),
        (21, -21),
        "diagonals stay inside the well"
    );
}

#[test]
fn every_device_has_every_layer_the_layout_draws_and_nothing_else() {
    let directory = format!("{}/assets/devices", env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0;
    for entry in std::fs::read_dir(&directory).unwrap() {
        let folder = entry.unwrap().path();
        let device =
            Device::parse(&std::fs::read_to_string(folder.join("manifest")).unwrap()).unwrap();
        let mut expected = device_layers();
        expected.sort();
        let mut actual: Vec<String> = device.image_names().map(String::from).collect();
        actual.sort();
        assert_eq!(actual, expected, "{}", folder.display());
        for name in actual {
            assert!(
                folder.join(format!("{name}.png")).exists(),
                "{} has no {name}.png",
                folder.display()
            );
        }
        for (anchor, _) in device.anchors() {
            assert!(
                anchor_text(anchor).is_some(),
                "{}: no text for anchor {anchor}",
                folder.display()
            );
        }
        assert!(
            device.stick("dpad").is_some(),
            "{}: no stick-dpad anchor",
            folder.display()
        );
        checked += 1;
    }
    assert!(checked > 0);
}

#[test]
fn analog_mode_shows_the_stick_instead_of_dpad_presses() {
    let ops = draw_connected(LiveView {
        buttons: xinput::DPAD_UP,
        dpad: DpadMode::LeftStick,
        ..LiveView::default()
    });
    assert!(placed(&ops, "press-dpad").is_empty());
    assert!(!placed(&ops, "stick-thumb").is_empty());
}

#[test]
fn plain_dpad_mode_draws_no_gate() {
    let ops = draw_connected(LiveView::default());
    assert!(placed(&ops, "stick-well").is_empty() && placed(&ops, "stick-thumb").is_empty());
}

#[test]
fn active_turbo_and_dpad_mode_are_named_under_the_headline() {
    let ops = draw_connected(LiveView {
        turbo: Turbo {
            buttons: xinput::B | xinput::RIGHT_SHOULDER,
            left_trigger: false,
            right_trigger: true,
        },
        dpad: DpadMode::LeftStick,
        ..LiveView::default()
    });
    assert!(texts(&ops).contains(&"Turbo A, R1, R2 · D-pad as left analog stick".to_string()));
    assert_eq!(
        placed(&ops, "turbo"),
        vec![
            ("turbo-a", 626, 396),
            ("turbo-r1", 616, 68),
            ("turbo-r2", 548, 68),
        ]
    );
    assert!(placed(&ops, "stick-well").contains(&("stick-well-l", 388, 388)));
    assert!(images(&ops).contains(&"btn-select") && images(&ops).contains(&"btn-start"));
}

#[test]
fn waiting_offers_only_the_buttons_that_work() {
    let all = texts(&draw(
        Screen::Waiting(Mode::Gamepad, Diagnosis::None),
        None,
        None,
    ));
    assert!(all.contains(&"QUIT".to_string()) && all.contains(&"SETTINGS".to_string()));
    assert!(!all.contains(&"MODE".to_string()));
}

#[test]
fn loading_dots_highlight_one_dot_per_phase() {
    let screen = Screen::Waiting(Mode::Xbox, Diagnosis::None);
    let without = circles_painted(&draw(screen.clone(), None, None), Paint::Accent);
    for phase in 0..6 {
        let with = circles_painted(&draw(screen.clone(), None, Some(phase)), Paint::Accent);
        assert_eq!(with, without + 1, "phase {phase}");
    }
}

#[test]
fn connected_names_the_mode_and_how_to_quit() {
    let all = texts(&draw(Screen::Connected(Kind::Xbox), None, None));
    assert!(all.contains(&"Connected".to_string()) && all.contains(&"Xbox-style".to_string()));
    assert!(all.contains(&QUIT_HINT.to_string()));
}

#[test]
fn hold_progress_fills_its_share_of_the_track() {
    let ops = draw(Screen::Connected(Kind::Xbox), Some(60), None);
    let width_of = |wanted: Paint| {
        ops.iter()
            .find_map(|op| match op {
                Op::Fill { w, paint, .. } if *paint == wanted && *w > 100 => Some(*w),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(width_of(Paint::Accent), width_of(Paint::Track) * 60 / 100);
    assert!(texts(&ops).contains(&"Keep holding to quit".to_string()));
}

#[test]
fn the_hold_bar_takes_the_hint_row_so_it_never_covers_the_drawing() {
    for screen in [
        Screen::Connected(Kind::Xbox),
        Screen::Waiting(Mode::Xbox, Diagnosis::None),
    ] {
        let ops = draw(screen, Some(40), None);
        let all = texts(&ops);
        assert!(!all.contains(&QUIT_HINT.to_string()) && !all.contains(&"QUIT".to_string()));
        let bar_top = ops
            .iter()
            .filter_map(|op| match op {
                Op::Fill {
                    y,
                    w,
                    paint: Paint::Track,
                    ..
                } => Some((*w, *y)),
                _ => None,
            })
            .max()
            .map(|(_, y)| y);
        assert!(bar_top.is_some_and(|y| y > 680), "{bar_top:?}");
    }
}

const ONE_LINE_BUDGET: usize = 40;

#[test]
fn headlines_fit_on_one_line_so_nothing_wraps_into_the_next_row() {
    let mut lines: Vec<&str> = DIAGNOSES
        .into_iter()
        .map(|diagnosis| waiting_lines(diagnosis).0)
        .collect();
    lines.extend([QUIT_HINT, SHORTCUTS_HINT]);
    for line in lines {
        assert!(
            line.len() <= ONE_LINE_BUDGET,
            "{line:?} is {} characters",
            line.len()
        );
    }
}

#[test]
fn everything_fits_on_screen() {
    let screens = [
        Screen::Waiting(Mode::Automatic, Diagnosis::SetupStuck),
        Screen::Waiting(Mode::Automatic, Diagnosis::NoComputer),
        Screen::Connected(Kind::Gamepad),
    ];
    let mut drawings: Vec<(String, Vec<Op>)> = Vec::new();
    for screen in screens {
        for hold in [None, Some(100)] {
            drawings.push((format!("{screen:?}"), draw(screen.clone(), hold, Some(1))));
        }
    }
    for row in Row::ALL {
        drawings.push((
            format!("{row:?}"),
            draw_overlay(Some(settings_view(row)), None, None),
        ));
    }
    drawings.push(("debug".into(), draw_overlay(None, Some(debug_view()), None)));
    for (screen, ops) in drawings {
        {
            for op in ops {
                match op {
                    Op::Fill { x, y, w, h, .. } => {
                        assert!(
                            x >= 0 && y >= 0 && w >= 0 && x + w <= WIDTH && y + h <= HEIGHT,
                            "{screen:?}"
                        )
                    }
                    Op::Circle { cx, cy, radius, .. } => assert!(
                        cx - radius >= 0
                            && cx + radius <= WIDTH
                            && cy - radius >= 0
                            && cy + radius <= HEIGHT,
                        "{screen:?}"
                    ),
                    Op::Text { x, y, .. } | Op::Image { x, y, .. } => {
                        assert!((0..=WIDTH).contains(&x) && (0..=HEIGHT).contains(&y))
                    }
                }
            }
        }
    }
}

#[test]
fn settings_list_every_row_with_its_value_and_the_version() {
    let all = texts(&draw_overlay(Some(settings_view(Row::Face)), None, None));
    for row in Row::ALL {
        assert!(all.contains(&row_label(row).to_string()), "{row:?}");
    }
    assert!(all.contains(&"Nintendo layout".to_string()));
    assert!(all.contains(&"Generic".to_string()), "the pending mode");
    assert!(
        all.iter()
            .any(|text| text.starts_with("Pocket Controller "))
    );
}

#[test]
fn the_panel_shows_every_choice_of_the_cursor_row_and_highlights_one() {
    let ops = draw_overlay(Some(settings_view(Row::Mode)), None, None);
    let shown = images(&ops);
    assert!(shown.contains(&"mode-gamepad-48-on"));
    assert!(shown.contains(&"mode-auto-48-off") && shown.contains(&"mode-xbox-48-off"));
    assert!(shown.contains(&"chevron-left") && !shown.contains(&"chevron-right"));
    let screen_row = texts(&draw_overlay(Some(settings_view(Row::Screen)), None, None));
    for label in ["Dim", "Stay on", "Turn off"] {
        assert!(screen_row.contains(&label.to_string()), "{label}");
    }
}

#[test]
fn a_mode_change_while_connected_warns_in_plain_text() {
    let lines = texts(&draw_overlay(Some(settings_view(Row::Mode)), None, None)).join(" ");
    assert!(lines.contains(MODE_CHANGE_WARNING), "{lines}");
    let mut same = settings_view(Row::Mode);
    same.mode = Mode::Xbox;
    let lines = texts(&draw_overlay(Some(same), None, None)).join(" ");
    assert!(!lines.contains(MODE_CHANGE_WARNING));
}

#[test]
fn rumble_is_greyed_with_a_reason_in_generic_mode() {
    let mut view = settings_view(Row::Rumble);
    view.kind = Kind::Gamepad;
    let ops = draw_overlay(Some(view), None, None);
    assert!(ops.iter().any(|op| matches!(
        op,
        Op::Text { text, paint: Paint::Ink | Paint::InkMuted, .. } if text == RUMBLE_UNAVAILABLE
    )));
    assert!(!texts(&ops).contains(&"On".to_string()));
}

#[test]
fn panel_text_wraps_inside_the_panel() {
    for row in Row::ALL {
        let ops = draw_overlay(Some(settings_view(row)), None, None);
        for op in ops {
            if let Op::Text {
                role: Role::Detail,
                text,
                x,
                align: Align::Left,
                ..
            } = op
                && x >= PANEL_LEFT
            {
                assert!(
                    x + fake_measure(Role::Detail, &text) <= PANEL_LEFT + PANEL_WIDTH,
                    "{text:?}"
                );
            }
        }
    }
}

#[test]
fn the_debug_row_offers_a_instead_of_change() {
    let all = texts(&draw_overlay(Some(settings_view(Row::Debug)), None, None));
    assert!(all.contains(&"OPEN".to_string()) && !all.contains(&"Change".to_string()));
}

#[test]
fn the_debug_view_names_what_the_computer_receives_beside_the_live_drawing() {
    let ops = draw_overlay(None, Some(debug_view()), None);
    let all = texts(&ops);
    for expected in [
        "Configured",
        "Xbox-style",
        "Interrupt IN, bulk OUT",
        "Seen",
        "128 of 255",
        "A  Up",
        "X -32767   Y 0",
    ] {
        assert!(all.contains(&expected.to_string()), "{expected}: {all:?}");
    }
    assert!(placed(&ops, "body").iter().all(|(_, x, _)| *x > 500));
    assert!(images(&ops).contains(&"press-a"));
}

#[test]
fn the_quit_hold_still_shows_over_settings() {
    let ops = draw_overlay(Some(settings_view(Row::Face)), None, Some(50));
    assert!(texts(&ops).contains(&"Keep holding to quit".to_string()));
}

#[test]
fn every_choice_fits_inside_its_card() {
    for row in Row::ALL {
        let ops = draw_overlay(Some(settings_view(row)), None, None);
        let cards: Vec<(i32, i32)> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Fill {
                    x,
                    y,
                    w,
                    paint: Paint::Card | Paint::Text,
                    ..
                } if *y == CARDS_TOP && *x >= PANEL_LEFT => Some((*x, *w)),
                _ => None,
            })
            .collect();
        for op in &ops {
            if let Op::Text {
                role: Role::Detail,
                text,
                x,
                align: Align::Center,
                ..
            } = op
            {
                let half = fake_measure(Role::Detail, text) / 2;
                let inside = cards.iter().any(|(left, width)| {
                    let padding = CARD_RADIUS - 12;
                    *x - half >= *left - padding && *x + half <= left + width + padding
                });
                assert!(inside, "{row:?}: {text:?}");
            }
        }
    }
}

#[test]
fn the_face_row_draws_what_the_computer_receives() {
    let mut view = settings_view(Row::Face);
    assert!(images(&draw_overlay(Some(view), None, None)).contains(&"face-nintendo"));
    view.settings.face = pocket_controller::layer::FaceLayout::Position;
    assert!(images(&draw_overlay(Some(view), None, None)).contains(&"face-xbox"));
}

#[test]
fn a_dark_screen_setting_shows_the_keys_that_wake_it() {
    use pocket_controller::settings::ScreenWhilePlaying;
    let mut view = settings_view(Row::Screen);
    for (screen, shown) in [
        (ScreenWhilePlaying::DimAfter, true),
        (ScreenWhilePlaying::OffAfter, true),
        (ScreenWhilePlaying::AlwaysOn, false),
    ] {
        view.settings.screen = screen;
        let ops = draw_overlay(Some(view), None, None);
        let keys = images(&ops);
        assert_eq!(
            keys.contains(&"key-menu") && keys.contains(&"key-power"),
            shown,
            "{screen:?}"
        );
    }
}
