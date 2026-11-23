use super::*;

pub(super) fn kind_title(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Xbox => ("Xbox-style", "mode-xbox-48-off"),
        Kind::Gamepad => ("Generic", "mode-gamepad-48-off"),
    }
}

pub(super) const CONNECTED_AS: &str = "as";
pub(super) const WORD_GAP: i32 = 14;
pub(super) const MODE_GLYPH_GAP: i32 = 12;

pub(super) fn connected_headline(canvas: &mut Canvas, kind: Kind, cy: i32) {
    let (title, glyph) = kind_title(kind);
    let headline = "Connected";
    let headline_width = canvas.design_width_of(Role::Headline, headline);
    let title_width = canvas.design_width_of(Role::Headline, title);
    let as_width = canvas.design_width_of(Role::Headline, CONNECTED_AS);
    let total = ICON_SIZE
        + ICON_GAP
        + headline_width
        + WORD_GAP
        + as_width
        + WORD_GAP
        + ICON_SIZE
        + MODE_GLYPH_GAP
        + title_width;
    let mut x = (DESIGN_WIDTH - total) / 2;
    canvas.image("icon-connected", x, cy - ICON_SIZE / 2);
    x += ICON_SIZE + ICON_GAP;
    canvas.text(Role::Headline, headline, x, cy, Align::Left, Paint::Text);
    x += headline_width + WORD_GAP;
    canvas.text(
        Role::Headline,
        CONNECTED_AS,
        x,
        cy,
        Align::Left,
        Paint::Muted,
    );
    x += as_width + WORD_GAP;
    canvas.image(glyph, x, cy - ICON_SIZE / 2);
    x += ICON_SIZE + MODE_GLYPH_GAP;
    canvas.text(Role::Headline, title, x, cy, Align::Left, Paint::Muted);
}

pub(super) fn printed_turbo_names(turbo: Turbo) -> Vec<&'static str> {
    let mut names: Vec<&str> = [
        (xinput::B, "A"),
        (xinput::A, "B"),
        (xinput::Y, "X"),
        (xinput::X, "Y"),
        (xinput::LEFT_SHOULDER, "L1"),
        (xinput::RIGHT_SHOULDER, "R1"),
    ]
    .into_iter()
    .filter(|(bit, _)| turbo.buttons & bit != 0)
    .map(|(_, name)| name)
    .collect();
    if turbo.left_trigger {
        names.push("L2");
    }
    if turbo.right_trigger {
        names.push("R2");
    }
    names
}

pub(super) fn options_line(live: &LiveView) -> Option<String> {
    let mut parts = Vec::new();
    let turbo = printed_turbo_names(live.turbo);
    if !turbo.is_empty() {
        parts.push(format!("Turbo {}", turbo.join(", ")));
    }
    match live.dpad {
        DpadMode::Dpad => {}
        DpadMode::LeftStick => parts.push("D-pad as left analog stick".to_string()),
        DpadMode::RightStick => parts.push("D-pad as right analog stick".to_string()),
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

pub(super) const CONNECTED_HEADLINE_Y: i32 = 604;
pub(super) const OPTIONS_Y: i32 = 650;
