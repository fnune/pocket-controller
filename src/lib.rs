pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod chord;
pub mod control;
pub mod evdev;
pub mod frame;
pub mod functionfs;
pub mod gadget;
pub mod hid;
pub mod interrupt;
pub mod layer;
pub mod personality;
pub mod profile;
pub mod settings;
pub mod shortcuts;
pub mod system;
pub mod xinput;
