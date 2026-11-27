## [0.2.0] - 2026-11-27

### Features

- Add a setting to swap the shoulder buttons

### Refactor

- Run the FunctionFS loops over Read and Write
- Run the pad and power readers over a Read source
- Lift the main loop's decisions out of run_loop and run
- Take the screen helper's text measure and arguments as values
- Put the display ioctls behind a Panel trait in Backlight
- Drop the unused session level from Backlight
- Move Hardware and the main loop from main.rs into a runtime module
- Put Session, ScreenHelper, Presenter and the display behind traits in Hardware
- Extract serve from run so the loop and shutdown run on any Hardware
- Share the Brick gadget fixture from the gadget fake module

## [0.1.0] - 2026-11-23

First release. Pocket Controller turns a handheld into a wired USB controller for a computer, with nothing to install on the computer. Only the TrimUI Brick on NextUI has been tested.

- Xbox-style and Generic USB modes, and Automatic, which picks Xbox-style when the computer has the driver
- MENU shortcuts: turbo per button, the D-pad as either analog stick, and a tap for Guide
- A live drawing of the handheld that shows pressed buttons, turbo and the stick the D-pad drives
- Settings for the face button layout, rumble, the screen while playing and the mode, and a debug view
- Packages for NextUI (`Pocket.Controller.pak.zip`) and the stock TrimUI firmware (`PocketController-trimui-stock.zip`)
