# Screen assets

SVG sources and their 1:1 PNGs for the 1024x768 screen. PNGs are transparent and are blitted pixel for pixel on black. Positions are the top-left corner (x, y) on the screen. Text positions give the vertical centre of the font's line box (ascent 1075, descent 320 per 1000 units), which is the centre of an SDL_ttf surface. No screen has a title row.

Rasterise with `nix-shell --packages resvg --run 'resvg --use-font-file <Rounded Mplus 1c Bold .ttf> in.svg out.png'`. The font is needed for the "?" in `link-unrecognised` and the letters in the `brick` assets.

## Waiting screen

| Asset or text | Size | Position | Use |
| --- | --- | --- | --- |
| `link-unplugged` | 640x200 | (192, 130) | `Diagnosis::None`: plug below the socket, grey cable |
| `link-unrecognised` | 640x200 | (192, 130) | `Diagnosis::NoComputer`: plugged in, amber "?" badge at the socket |
| `link-stuck` | 640x200 | (192, 130) | `Diagnosis::SetupStuck`: plugged in, three amber dots in the bottom run of the cable |
| `icon-waiting` | 48x48 | (x, 406) | Before the "Not connected" headline: grey ring with three dots |
| `icon-warning` | 48x48 | (x, 406) | Before both warning headlines: amber triangle with "!" |
| Headline | 36 px white | centre y 430 | |
| Instruction | 28 px `#999999` | first line centre y 484, 38 px between lines | Wrapped to a column of at most 700 px |
| Hint row | | y 693 to 737 | `(A) SETTINGS (X) DEBUG (Y) MODE` left, `(B) QUIT` right |

- Icon and headline are centred as one group: `x = (1024 - (48 + 16 + headline_width)) / 2`, and the headline starts at `x + 64`.
- The drawn part of the illustration spans x 308 to 716, clear of the chip.

## Mode chip, top right

| Asset | Size | Position | Use |
| --- | --- | --- | --- |
| `mode-auto` | 28x28 | (chip_x + 14, 36) | Circular arrows, word "Automatic" |
| `mode-xbox` | 28x28 | (chip_x + 14, 36) | Controller with grips, word "Xbox-style" |
| `mode-gamepad` | 28x28 | (chip_x + 14, 36) | Flat pad, word "Generic" |

- Chip: a `#1e2329` pill, y 30 to 70 (height 40, radius 20), right edge at x 984. Waiting screen only, hidden while the mode picker is open.
- `chip_width = 14 + 28 + 8 + word_width + 18`, `chip_x = 984 - chip_width`. The word starts at `chip_x + 50`, white, 24 px, vertical centre y 50.
- Measured at 24 px: "Automatic" 119 px (chip x 797), "Xbox-style" 121 px (chip x 795), "Generic" 87 px (chip x 829).

## Mode picker

Replaces the status icon, headline and instruction while open. The illustration and hints stay.

| Asset | Size | Position | Use |
| --- | --- | --- | --- |
| `mode-auto-48-on`, `mode-auto-48-off` | 48x48 | (176, 374) | Row 1, Automatic |
| `mode-xbox-48-on`, `mode-xbox-48-off` | 48x48 | (176, 466) | Row 2, Xbox-style |
| `mode-gamepad-48-on`, `mode-gamepad-48-off` | 48x48 | (176, 558) | Row 3, Generic |

- `-on` is black, for the selected row. `-off` is `#999999`, for the other rows.
- Rows: 704x84 at x 160, y 356, 448 and 540 (92 px pitch). The selected row is a white pill, radius 24, with a black title and a `#333333` line. Other rows have no fill and `#999999` text.
- Text is left aligned at x 236.

| Text | Size | Centre, relative to row y | Row 1 | Row 2 | Row 3 |
| --- | --- | --- | --- | --- | --- |
| Title | 28 px | + 28 | y 384 | y 476 | y 568 |
| Line | 15 px | + 60 | y 416 | y 508 | y 600 |
| "Recommended" text | 16 px, white | + 30 | y 386 | | |

- The line is 15 px, not 18 px: the longest line is 738 px wide at 18 px and the pill has 608 px. At 15 px it is 615 px and ends at x 851.
- "Recommended" tag, Automatic row only: a `#9b2257` pill 130x28 (radius 14) at (389, 372), which is 14 px after the title, with 12 px side padding.

## Connected screen and shortcuts

One screen. The Brick is always at the same position. While MENU is held, `brick-leaders` and the labels are added, the note line replaces the headline row and the options line, and only the right half of the hint row shows.

Draw order: `brick`, pressed D-pad overlays (D-pad mode only), button overlays, `brick-gate`, `brick-gate-l` or `brick-gate-r`, `brick-stick-dot` (analog modes only), then `brick-leaders` and labels while MENU is held.

| Asset | Size | Position |
| --- | --- | --- |
| `brick` | 432x504 | (296, 52) |
| `brick-leaders` | 432x504 | (296, 52) |
| `brick-press-x`, `brick-turbo-x`, `brick-turbo-press-x` | 32x32 | (596, 366) |
| `brick-press-a`, `brick-turbo-a`, `brick-turbo-press-a` | 32x32 | (626, 396) |
| `brick-press-b`, `brick-turbo-b`, `brick-turbo-press-b` | 32x32 | (596, 426) |
| `brick-press-y`, `brick-turbo-y`, `brick-turbo-press-y` | 32x32 | (566, 396) |
| `brick-press-l1`, `brick-turbo-l1`, `brick-turbo-press-l1` | 60x22 | (348, 68) |
| `brick-press-l2`, `brick-turbo-l2`, `brick-turbo-press-l2` | 60x22 | (416, 68) |
| `brick-press-r2`, `brick-turbo-r2`, `brick-turbo-press-r2` | 60x22 | (548, 68) |
| `brick-press-r1`, `brick-turbo-r1`, `brick-turbo-press-r1` | 60x22 | (616, 68) |
| `brick-press-menu` | 40x22 | (442, 473) |
| `brick-press-select` | 40x22 | (492, 473) |
| `brick-press-start` | 40x22 | (542, 473) |
| `brick-press-stick` for L3 | 24x10 | (478, 407) |
| `brick-press-stick` for R3 | 24x10 | (522, 407) |
| `brick-press-dpad-up` | 18x22 | (403, 377) |
| `brick-press-dpad-down` | 18x22 | (403, 425) |
| `brick-press-dpad-left` | 22x18 | (377, 403) |
| `brick-press-dpad-right` | 22x18 | (425, 403) |
| `brick-gate` | 86x86 | (369, 369) |
| `brick-gate-l`, `brick-gate-r` | 48x48 | (388, 388) |
| `brick-stick-dot` | 20x20 | (cx + x*R - 10, cy - y*R - 10) |
| `brick-press-volume-up` | 8x33 | (324, 160) |
| `brick-press-volume-down` | 8x33 | (324, 197) |
| `brick-press-power` | 8x30 | (692, 212) |
| `brick-fn-on` | 8x30 | (692, 160) |

- Each face button and shoulder tab has three files, picked by state: `brick-press-*` (pressed: white, black glyph), `brick-turbo-*` (turbo on: accent `#9b2257`, white glyph), `brick-turbo-press-*` (turbo on and pressed: white with an accent border, black glyph).
- The accent colour means turbo only. The D-pad mode is shown by shape, and only in an analog mode: in plain D-pad mode nothing extra is drawn.
- Analog mode: `brick-gate` is a filled disc (a stick well, radius 43, `#3a3f47` with a 2 px `#5a626c` rim) centred on (cx, cy) = (412, 412). It covers the D-pad cross completely, so the D-pad press overlays are not drawn in analog mode. `brick-gate-l` or `brick-gate-r` is a faint watermark letter (44 px, `#4a515b`) centred in the well, saying which stick. Draw it after `brick-gate` and before the thumb.
- `brick-stick-dot` is the stick thumb: a light disc (`#e6e6e6`, 16 px) with a 2 px black outline. Draw it centred at (cx + x*R, cy - y*R) with R = 30 and x, y from -1 to 1 (diagonals at 0.71), which keeps it inside the rim. At the centre its top-left is (402, 402).
- The pressed D-pad overlays are white and 4 px smaller than the arm on its outer sides.
- Shoulder tabs, left to right: L1, L2, R2, R1.
- Side controls: the volume and power overlays are white, like the other pressed overlays. The FN switch is a dark slot (`#444444`) with a thumb: `brick` shows it off, with a dim thumb in the lower half, and `brick-fn-on` shows it on, with a white thumb in the upper half. None of them has a label.

MENU not held:

| Piece | Size | Position |
| --- | --- | --- |
| `icon-connected` | 48x48 | (x, 580) |
| "Connected" | 36 px white | vertical centre y 604 |
| "as" | 36 px `#999999` | vertical centre y 604 |
| `mode-xbox-48-off` or `mode-gamepad-48-off` | 48x48 | y 580 |
| Mode title | 36 px `#999999` | vertical centre y 604 |
| Options line | 28 px `#999999` | centre (512, 650), may be absent |

- Headline row, centred as a group from measured widths: `icon 48, gap 16, "Connected", gap 14, "as", gap 14, glyph 48, gap 12, mode title`. With "Xbox-style" in the preview font: icon x 237, "Connected" x 301, "as" x 495, glyph x 545, title x 605.

MENU held, label anchors as (x, y, alignment). y is the vertical centre of the text, x is the edge named by the alignment (or the middle for centre). Labels are 22 px white unless noted.

| Control | Label | Anchor |
| --- | --- | --- |
| L1 and L2 | Turbo | (290, 58, right) |
| D-pad group heading | D-pad acts as | (290, 340, right), 18 px `#999999` |
| D-pad up | D-pad | (286, 376, right) |
| D-pad left | Left analog stick | (286, 416, right) |
| D-pad right | Right analog stick | (286, 456, right) |
| R2 and R1 | Turbo | (734, 58, left) |
| A, B, X, Y | Turbo | (734, 412, left) |
| L3 | L3 | (490, 438, centre), 18 px `#999999` |
| R3 | R3 | (534, 438, centre), 18 px `#999999` |
| MENU | Hold | (412, 526, right) |
| START | Settings | (612, 526, left) |

- `brick-choice` (16x16, a white check) marks the active row under "D-pad acts as": `x = label_left - 8 - 16`, where `label_left = 286 - label_width`, and `y = row_y - 8`, so y 368, 408 or 448. In the preview font x is 200 for "D-pad", 81 for "Left analog stick" and 70 for "Right analog stick".
- L3 and R3 have no leader. SELECT has no leader and no label. The volume rocker (left side), the FN switch and the power button (right side) have no label.
- One "Turbo" label serves each shoulder pair and one serves the four face buttons.
- Note line ("Turbo A: on"): centred on (512, 604), 28 px white.

## Bottom hint row

One row at vertical centre y 715, 24 px white text, pills at y 699. The connected screen shows both halves. With MENU held only the right half shows. The waiting screen keeps its own hint row.

| Piece | Size | Position |
| --- | --- | --- |
| `btn-menu` | 52x32 | (40, 699) |
| "Hold for shortcuts" | 24 px white | left edge x 106, vertical centre y 715 |
| `btn-select` | 52x32 | (693, 699) |
| "+" | 24 px white | centre (763, 715) |
| `btn-start` | 52x32 | (780, 699) |
| "Hold to quit" | 24 px white | left edge x 846, vertical centre y 715, ends at x 984 |

- Gaps: 14 px between a pill and its text, 10 px on each side of "+". The right half is right-aligned to x 984, with "+" 15 px and "Hold to quit" 138 px wide.
- `btn-menu` carries the Brick's MENU glyph, a triangle of three small hexagons. The same glyph is on the MENU pill of `brick`.
- While the quit chord is held the row is replaced by "Keep holding to quit" (24 px white, left edge x 40, centre y 715, 232 px wide) and a 16 px bar at y 707 to 723 from 24 px after the text (x 296) to x 984. The preview uses a `#1e2329` track and a white fill, both with radius 8.

## Error screen and spare art

| Asset | Size | Position | Use |
| --- | --- | --- | --- |
| `icon-error` | 48x48 | not placed for the layout without a title | Before the error headline: red octagon with a cross |
| `link-connected` | 640x200 | unused | Spare: plugged in, green check badge |

## Previews

`preview-unplugged.png`, `preview-unrecognised.png`, `preview-stuck.png`, `preview-picker-auto.png`, `preview-picker-xbox.png`, `preview-connected.png`, `preview-sheet.png` and `preview-hold.png` are full-screen compositions with faked text. They are not shipped. `preview-picker-auto-two-lines.png` and `preview-picker-xbox-two-lines.png` show an alternative with the line wrapped at 18 px.
