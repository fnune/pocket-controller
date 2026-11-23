# Screen assets

Images for the 1024x768 screen, blitted 1:1 on black by `pocket-controller-screen`. Every PNG is generated from an SVG source: edit the SVG, run `just assets`, commit both. `just check` fails if a PNG or manifest is out of date.

- `screen/assets/*.svg`: images every device shares, such as the mode glyphs, the connection illustrations, the status icons and the hint pills. Their positions live in `screen/src/layout.rs`.
- `screen/devices/<device>.svg`: one drawing per handheld, with every overlay as a named layer. `just assets` writes `screen/assets/devices/<device>/`, with one PNG per layer and a `manifest` of positions the helper reads at runtime.
- `screen/fonts/rounded-mplus-1c-bold-subset.ttf`: the only font used when rendering, so output does not depend on the fonts installed on the machine. It is a subset; add glyphs to it before using a character it lacks.

The helper picks a device with `--device <name>`, `trimui-brick` by default.

## Adding a device

Copy `screen/devices/trimui-brick.svg` to `screen/devices/<device>.svg` and redraw it. The canvas is the whole 1024x768 screen, so every coordinate is a screen position.

Each top-level `<svg id="…" x="…" y="…" width="…" height="…" viewBox="…">` is a layer. `x` and `y` are its top-left corner on the screen and must be whole numbers. A layer the device lacks can be left out, but the tests expect the full set below; mark a deliberate gap in the test if a device has no such control.

| Layer | Drawn when |
| --- | --- |
| `body` | Always: the handheld with every control at rest |
| `press-dpad-up`, `-down`, `-left`, `-right` | That direction is held, in plain D-pad mode |
| `press-menu`, `press-select`, `press-start` | That button is held |
| `press-l3`, `press-r3` | That stick click is held |
| `press-volume-up`, `press-volume-down` | That volume key is held |
| `fn-on` | The FN switch is on (`body` shows it off) |
| `press-<k>`, `turbo-<k>`, `turbo-press-<k>` for `k` in `a`, `b`, `x`, `y`, `l1`, `l2`, `r1`, `r2` | Pressed; turbo on; turbo on and pressed. `a` is the button printed A, wherever it sits |
| `stick-well` | The D-pad acts as an analog stick: a disc that covers the D-pad |
| `stick-well-l`, `stick-well-r` | On the well, saying which stick |
| `stick-thumb` | On the well, placed at rest; the helper moves it with the stick |
| `leaders` | While MENU is held: lines from the labels to the controls |
| `choice` | While MENU is held: marks the active "D-pad acts as" label; the helper places it |

The `<g id="anchors" display="none">` group holds points as `<circle id="…" cx="…" cy="…">`:

- `label-*` and `caption-*`: where the helper writes a label while MENU is held, with `data-align` set to `left`, `center` or `right`. `cy` is the text's vertical centre. The words come from `anchor_text` in `layout.rs`.
- `stick-dpad`: the centre of the stick well, with `r` as the distance the thumb's centre travels.

Draw order: `body`, D-pad presses, other presses, the side controls, turbo states, the stick well, its letter and thumb, then the leaders and labels.

## Style

| Use | Colour |
| --- | --- |
| Background | `#000000` |
| Text, pressed controls | `#ffffff` |
| Secondary text, captions | `#999999` |
| Accent: turbo, the selected mode's tag | `#9b2257` |
| Chip and progress track | `#1e2329` |
| Controls at rest | `#777777` |
| Stick well | `#3a3f47`, with a 2 px `#5a626c` rim |
| Stick letter | `#4a515b` |
| Stick thumb | `#e6e6e6`, with a 2 px black outline |

- Pressed is white with a black glyph, turbo is the accent with a white glyph, turbo and pressed is white with an accent border.
- Accent means turbo on the handheld drawing, and nothing else.
- Lines and outlines are at least 2 px, so they stay even at 1:1.
- Text in SVGs uses `font-family="'Rounded Mplus 1c Bold'"`, quoted, or the renderer falls back to another font.
