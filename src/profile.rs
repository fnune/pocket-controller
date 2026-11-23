#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Brick,
    BrickPro,
    SmartPro,
    SmartProS,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    pub drawing: Option<&'static str>,
    pub motor: Option<&'static str>,
    pub sunxi_backlight: bool,
}

const TG5040_MOTOR: &str = "/sys/class/gpio/gpio227/value";
const TG5050_MOTOR: &str = "/sys/class/gpio/gpio236/value";
const LAUNCHER_PREFIX: &[u8] = b"Trimui ";

impl Model {
    pub fn from_device_name(name: &str) -> Option<Self> {
        match name.trim() {
            "brick" => Some(Self::Brick),
            "brickpro" => Some(Self::BrickPro),
            "smartpro" => Some(Self::SmartPro),
            "smartpros" => Some(Self::SmartProS),
            _ => None,
        }
    }

    pub fn from_launcher_name(name: &str) -> Option<Self> {
        match name {
            "Trimui Brick" => Some(Self::Brick),
            "Trimui Brick Pro" => Some(Self::BrickPro),
            "Trimui Smart Pro" => Some(Self::SmartPro),
            "Trimui Smart Pro S" => Some(Self::SmartProS),
            _ => None,
        }
    }

    pub fn profile(self) -> Profile {
        let trimui = |drawing, motor| Profile {
            drawing,
            motor: Some(motor),
            sunxi_backlight: true,
        };
        match self {
            Self::Brick => trimui(Some("trimui-brick"), TG5040_MOTOR),
            Self::BrickPro | Self::SmartPro => trimui(None, TG5040_MOTOR),
            Self::SmartProS => trimui(None, TG5050_MOTOR),
            Self::Unknown => Profile {
                drawing: None,
                motor: None,
                sunxi_backlight: false,
            },
        }
    }
}

fn launcher_names(binary: &[u8]) -> impl Iterator<Item = &str> {
    binary
        .split(|byte| !byte.is_ascii_graphic() && *byte != b' ')
        .filter(|run| run.starts_with(LAUNCHER_PREFIX))
        .filter_map(|run| std::str::from_utf8(run).ok())
}

pub fn detect(device_name: Option<&str>, launcher: Option<&[u8]>) -> Model {
    device_name
        .and_then(Model::from_device_name)
        .or_else(|| {
            launcher.and_then(|binary| launcher_names(binary).find_map(Model::from_launcher_name))
        })
        .unwrap_or(Model::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nextui_names_the_device_in_its_environment() {
        assert_eq!(detect(Some("brick"), None), Model::Brick);
        assert_eq!(detect(Some("smartpros"), None), Model::SmartProS);
    }

    #[test]
    fn the_stock_launcher_names_the_device_when_nextui_does_not() {
        let launcher = b"\x7fELF\x00\x01junk\x00Trimui Brick Pro\x00more";
        assert_eq!(detect(None, Some(launcher)), Model::BrickPro);
        assert_eq!(
            detect(Some("unheard-of"), Some(b"\x00Trimui Brick\x00")),
            Model::Brick
        );
    }

    #[test]
    fn an_unrecognised_device_gets_no_hardware_extras() {
        assert_eq!(
            detect(None, Some(b"\x00Trimui Toaster\x00")),
            Model::Unknown
        );
        assert_eq!(
            Model::Unknown.profile(),
            Profile {
                drawing: None,
                motor: None,
                sunxi_backlight: false
            }
        );
    }

    #[test]
    fn only_devices_with_a_drawing_name_one() {
        assert_eq!(Model::Brick.profile().drawing, Some("trimui-brick"));
        assert_eq!(Model::SmartPro.profile().drawing, None);
    }

    #[test]
    fn every_named_drawing_ships() {
        for model in [
            Model::Brick,
            Model::BrickPro,
            Model::SmartPro,
            Model::SmartProS,
        ] {
            if let Some(drawing) = model.profile().drawing {
                let source = format!(
                    "{}/screen/devices/{drawing}.svg",
                    env!("CARGO_MANIFEST_DIR")
                );
                assert!(std::path::Path::new(&source).exists(), "{source}");
            }
        }
    }
}
