## [0.1.0] - 2026-11-23

### Features

- Add Pocket Controller, a USB controller mode for handhelds
- Drop the MENU + SELECT passthrough
- Add a settings overlay and a debug view
- Show face layouts and wake keys in settings
- Render screenshots on the computer

### Bug fixes

- Show the replug warning only on the mode row

### Refactor

- *(screen)* Draw the handheld from one SVG per device
- *(screen)* Split the layout into one module per screen part
- Keep the MENU shortcuts in one table
- Detect the handheld model and keep its hardware facts in a profile
- Put each USB mode behind a Personality trait
