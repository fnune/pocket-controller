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

#[cfg(test)]
mod tests {
    #[test]
    fn the_pak_store_listing_names_this_version() {
        let listing = include_str!("../pak.json");
        let expected = format!("\"version\": \"v{}\"", crate::VERSION);
        assert!(listing.contains(&expected), "{expected}");
    }
}
