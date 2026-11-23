mod device;
mod layout;
mod sdl;

use std::io::BufRead;
use std::process::ExitCode;

use device::Device;
use layout::{Align, Op, Paint, Role};
use pocket_controller::frame::FrameReader;
use sdl::{Color, Font, Rect, Screen, Sdl};

const USAGE: &str = "usage: pocket-controller-screen --font PATH [--assets DIR] [--device NAME]";
const BUFFERS: usize = 2;
const LINE_SPACING_PERCENT: i32 = 115;

fn color(paint: Paint) -> Color {
    let rgb = |r, g, b| Color { r, g, b, a: 255 };
    match paint {
        Paint::Background => rgb(0, 0, 0),
        Paint::Text => rgb(255, 255, 255),
        Paint::Muted => rgb(0x99, 0x99, 0x99),
        Paint::Accent => rgb(0x9b, 0x22, 0x57),
        Paint::Track => rgb(0x1e, 0x23, 0x29),
        Paint::Ink => rgb(0, 0, 0),
        Paint::InkMuted => rgb(0x33, 0x33, 0x33),
        Paint::Faint => rgb(0x66, 0x66, 0x66),
        Paint::Card => rgb(0x2b, 0x31, 0x39),
    }
}

struct Fonts {
    headline: *mut Font,
    body: *mut Font,
    small: *mut Font,
    hint: *mut Font,
    detail: *mut Font,
    value: *mut Font,
    tag: *mut Font,
    sheet_label: *mut Font,
    caption: *mut Font,
}

impl Fonts {
    fn open(sdl: &Sdl, path: &str, width: i32) -> Result<Self, String> {
        let size = |per_mille: i32| width * per_mille / 1000;
        Ok(Self {
            headline: sdl.open_font(path, size(42))?,
            body: sdl.open_font(path, size(30))?,
            small: sdl.open_font(path, size(24))?,
            hint: sdl.open_font(path, size(24))?,
            detail: sdl.open_font(path, size(20))?,
            value: sdl.open_font(path, size(18))?,
            tag: sdl.open_font(path, size(16))?,
            sheet_label: sdl.open_font(path, size(22))?,
            caption: sdl.open_font(path, size(18))?,
        })
    }

    fn get(&self, role: Role) -> *mut Font {
        match role {
            Role::Headline => self.headline,
            Role::Body => self.body,
            Role::Small => self.small,
            Role::Hint => self.hint,
            Role::Detail => self.detail,
            Role::Value => self.value,
            Role::Tag => self.tag,
            Role::SheetLabel => self.sheet_label,
            Role::Caption => self.caption,
        }
    }
}

fn wrap(screen: &Screen, font: *mut Font, text: &str, max_width: i32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(last) if screen.text_size(font, &format!("{last} {word}")).0 <= max_width => {
                last.push(' ');
                last.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

struct Images {
    loaded: std::collections::HashMap<String, *mut sdl::Surface>,
}

impl Images {
    fn load<'n>(sdl: &Sdl, directory: &str, names: impl IntoIterator<Item = &'n str>) -> Self {
        let mut loaded = std::collections::HashMap::new();
        for name in names {
            match sdl.load_image(&format!("{directory}/{name}.png")) {
                Ok(image) => {
                    loaded.insert(name.to_string(), image);
                }
                Err(error) => eprintln!("pocket-controller-screen: {error}"),
            }
        }
        Self { loaded }
    }

    fn extend(&mut self, other: Images) {
        self.loaded.extend(other.loaded);
    }
}

fn load_device(assets: &str, name: &str) -> Device {
    let path = format!("{assets}/devices/{name}/manifest");
    let parsed = std::fs::read_to_string(&path)
        .map_err(|error| format!("could not read {path}: {error}"))
        .and_then(|text| Device::parse(&text).map_err(|error| format!("{path}: {error}")));
    parsed.unwrap_or_else(|error| {
        eprintln!("pocket-controller-screen: {error}");
        Device::default()
    })
}

fn draw(screen: &Screen, fonts: &Fonts, images: &Images, ops: &[Op]) {
    let (width, _) = screen.size();
    let max_width = width * 92 / 100;
    for op in ops {
        match op {
            Op::Fill { x, y, w, h, paint } => screen.fill(
                Some(Rect {
                    x: *x,
                    y: *y,
                    w: *w,
                    h: *h,
                }),
                color(*paint),
            ),
            Op::Image { name, x, y } => {
                if let Some(image) = images.loaded.get(name) {
                    screen.blit(*image, *x, *y);
                }
            }
            Op::Circle {
                cx,
                cy,
                radius,
                paint,
            } => screen.circle(*cx, *cy, *radius, color(*paint)),
            Op::Text {
                role,
                text,
                x,
                y,
                align,
                paint,
            } => {
                let font = fonts.get(*role);
                let mut top = None;
                for line in wrap(screen, font, text, max_width) {
                    let (line_width, line_height) = screen.text_size(font, &line);
                    let line_top = *top.get_or_insert(y - line_height / 2);
                    let left = match align {
                        Align::Left => *x,
                        Align::Center => x - line_width / 2,
                        Align::Right => x - line_width,
                    };
                    screen.text(font, &line, left, line_top, color(*paint));
                    top = Some(line_top + line_height * LINE_SPACING_PERCENT / 100);
                }
            }
        }
    }
    screen.present();
}

fn run() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let (mut font_path, mut assets, mut device_name) = (None, None, None);
    while let Some(argument) = arguments.next() {
        match (argument.as_str(), arguments.next()) {
            ("--font", Some(path)) => font_path = Some(path),
            ("--assets", Some(path)) => assets = Some(path),
            ("--device", Some(name)) => device_name = Some(name),
            _ => return Err(USAGE.to_string()),
        }
    }
    let font_path = font_path.ok_or(USAGE)?;
    let sdl = Sdl::load()?;
    let screen = sdl.open_screen()?;
    let (width, height) = screen.size();
    let fonts = Fonts::open(&sdl, &font_path, width)?;
    let (device, images) = match (assets.as_deref(), device_name) {
        (Some(assets), Some(device_name)) => {
            let device = load_device(assets, &device_name);
            let mut images = Images::load(&sdl, assets, layout::IMAGES.iter().copied());
            images.extend(Images::load(
                &sdl,
                &format!("{assets}/devices/{device_name}"),
                device.image_names(),
            ));
            (device, images)
        }
        (Some(assets), None) => (
            Device::default(),
            Images::load(&sdl, assets, layout::IMAGES.iter().copied()),
        ),
        (None, _) => (
            Device::default(),
            Images {
                loaded: std::collections::HashMap::new(),
            },
        ),
    };
    let mut reader = FrameReader::new();
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(|error| format!("could not read frames: {error}"))?;
        match reader.push_line(&line) {
            Some(Some(frame)) => {
                let measure = |role: Role, text: &str| screen.text_size(fonts.get(role), text).0;
                draw(
                    &screen,
                    &fonts,
                    &images,
                    &layout::layout(&frame, width, height, &measure, &device),
                );
            }
            Some(None) => eprintln!("pocket-controller-screen: skipped a frame it could not read"),
            None => {}
        }
    }
    for _ in 0..BUFFERS {
        screen.fill(None, color(Paint::Background));
        screen.present();
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("pocket-controller-screen: {message}");
            ExitCode::FAILURE
        }
    }
}
