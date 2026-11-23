use std::collections::HashMap;

use crate::layout::Align;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub x: i32,
    pub y: i32,
    pub align: Align,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stick {
    pub x: i32,
    pub y: i32,
    pub travel: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Device {
    images: HashMap<String, (i32, i32)>,
    anchors: Vec<(String, Anchor)>,
    sticks: HashMap<String, Stick>,
}

fn whole(field: Option<&str>, line: &str) -> Result<i32, String> {
    field
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| format!("expected a whole number in {line:?}"))
}

fn align(field: Option<&str>, line: &str) -> Result<Align, String> {
    match field {
        Some("left") => Ok(Align::Left),
        Some("center") => Ok(Align::Center),
        Some("right") => Ok(Align::Right),
        _ => Err(format!("expected left, center or right in {line:?}")),
    }
}

impl Device {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut device = Self::default();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let mut fields = line.split_whitespace();
            let kind = fields.next();
            let name = fields
                .next()
                .ok_or_else(|| format!("expected a name in {line:?}"))?
                .to_string();
            match kind {
                Some("image") => {
                    let position = (whole(fields.next(), line)?, whole(fields.next(), line)?);
                    device.images.insert(name, position);
                }
                Some("anchor") => {
                    let anchor = Anchor {
                        x: whole(fields.next(), line)?,
                        y: whole(fields.next(), line)?,
                        align: align(fields.next(), line)?,
                    };
                    device.anchors.push((name, anchor));
                }
                Some("stick") => {
                    let stick = Stick {
                        x: whole(fields.next(), line)?,
                        y: whole(fields.next(), line)?,
                        travel: whole(fields.next(), line)?,
                    };
                    device.sticks.insert(name, stick);
                }
                _ => return Err(format!("unknown manifest line {line:?}")),
            }
        }
        Ok(device)
    }

    pub fn image(&self, name: &str) -> Option<(i32, i32)> {
        self.images.get(name).copied()
    }

    pub fn image_names(&self) -> impl Iterator<Item = &str> {
        self.images.keys().map(String::as_str)
    }

    pub fn anchors(&self) -> impl Iterator<Item = (&str, Anchor)> {
        self.anchors
            .iter()
            .map(|(name, anchor)| (name.as_str(), *anchor))
    }

    pub fn anchor(&self, name: &str) -> Option<Anchor> {
        self.anchors()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, anchor)| anchor)
    }

    pub fn stick(&self, name: &str) -> Option<Stick> {
        self.sticks.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_lists_images_anchors_and_sticks() {
        let device = Device::parse(
            "image body 296 52\nanchor label-menu 412 526 right\nstick dpad 412 412 30\n",
        )
        .unwrap();
        assert_eq!(device.image("body"), Some((296, 52)));
        assert_eq!(
            device.anchor("label-menu"),
            Some(Anchor {
                x: 412,
                y: 526,
                align: Align::Right
            })
        );
        assert_eq!(device.stick("dpad").map(|stick| stick.travel), Some(30));
    }

    #[test]
    fn a_broken_manifest_line_names_itself() {
        let error = Device::parse("image body left 52").unwrap_err();
        assert!(error.contains("image body left 52"), "{error}");
    }
}
