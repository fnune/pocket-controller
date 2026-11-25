use std::ffi::{CStr, CString, c_char, c_int, c_void};

#[repr(C)]
pub struct Window {
    _private: [u8; 0],
}

#[repr(C)]
pub struct PixelFormat {
    _private: [u8; 0],
}

#[repr(C)]
pub struct Font {
    _private: [u8; 0],
}

#[repr(C)]
pub struct Surface {
    pub flags: u32,
    pub format: *const PixelFormat,
    pub w: c_int,
    pub h: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: c_int,
    pub y: c_int,
    pub w: c_int,
    pub h: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

const INIT_VIDEO: u32 = 0x20;
const WINDOWPOS_UNDEFINED: c_int = 0x1FFF_0000;
const WINDOW_SHOWN: u32 = 0x4;
const CURSOR_HIDDEN: c_int = 0;

struct Library {
    handle: *mut c_void,
    name: &'static str,
}

impl Library {
    fn open(name: &'static str) -> Result<Self, String> {
        let file = CString::new(name).map_err(|error| error.to_string())?;
        let handle = unsafe { libc::dlopen(file.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) };
        if handle.is_null() {
            return Err(format!("could not load {name}: {}", last_dl_error()));
        }
        Ok(Self { handle, name })
    }

    fn symbol<T: Copy>(&self, symbol: &str) -> Result<T, String> {
        let wanted = CString::new(symbol).map_err(|error| error.to_string())?;
        let address = unsafe { libc::dlsym(self.handle, wanted.as_ptr()) };
        if address.is_null() {
            return Err(format!(
                "{} has no {symbol}: {}",
                self.name,
                last_dl_error()
            ));
        }
        Ok(unsafe { std::mem::transmute_copy(&address) })
    }
}

fn last_dl_error() -> String {
    let message = unsafe { libc::dlerror() };
    if message.is_null() {
        "unknown error".to_string()
    } else {
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    }
}

pub struct Sdl {
    init: unsafe extern "C" fn(u32) -> c_int,
    quit: unsafe extern "C" fn(),
    get_error: unsafe extern "C" fn() -> *const c_char,
    show_cursor: unsafe extern "C" fn(c_int) -> c_int,
    create_window:
        unsafe extern "C" fn(*const c_char, c_int, c_int, c_int, c_int, u32) -> *mut Window,
    get_window_surface: unsafe extern "C" fn(*mut Window) -> *mut Surface,
    update_window_surface: unsafe extern "C" fn(*mut Window) -> c_int,
    map_rgb: unsafe extern "C" fn(*const PixelFormat, u8, u8, u8) -> u32,
    fill_rect: unsafe extern "C" fn(*mut Surface, *const Rect, u32) -> c_int,
    upper_blit: unsafe extern "C" fn(*mut Surface, *const Rect, *mut Surface, *mut Rect) -> c_int,
    free_surface: unsafe extern "C" fn(*mut Surface),
    rw_from_file: unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void,
    save_bmp_rw: unsafe extern "C" fn(*mut Surface, *mut c_void, c_int) -> c_int,
    ttf_init: unsafe extern "C" fn() -> c_int,
    ttf_open_font: unsafe extern "C" fn(*const c_char, c_int) -> *mut Font,
    ttf_size_utf8: unsafe extern "C" fn(*mut Font, *const c_char, *mut c_int, *mut c_int) -> c_int,
    ttf_render_utf8_blended: unsafe extern "C" fn(*mut Font, *const c_char, Color) -> *mut Surface,
    img_load: Option<unsafe extern "C" fn(*const c_char) -> *mut Surface>,
    _libraries: Vec<Library>,
}

pub struct Screen<'a> {
    sdl: &'a Sdl,
    window: *mut Window,
    surface: *mut Surface,
}

impl Sdl {
    pub fn load() -> Result<Self, String> {
        let sdl = Library::open("libSDL2-2.0.so.0")?;
        let ttf = Library::open("libSDL2_ttf-2.0.so.0")?;
        let image = Library::open("libSDL2_image-2.0.so.0")
            .inspect_err(|error| {
                eprintln!("pocket-controller-screen: drawing without images: {error}")
            })
            .ok();
        let img_load = image
            .as_ref()
            .and_then(|image| image.symbol("IMG_Load").ok());
        Ok(Self {
            init: sdl.symbol("SDL_Init")?,
            quit: sdl.symbol("SDL_Quit")?,
            get_error: sdl.symbol("SDL_GetError")?,
            show_cursor: sdl.symbol("SDL_ShowCursor")?,
            create_window: sdl.symbol("SDL_CreateWindow")?,
            get_window_surface: sdl.symbol("SDL_GetWindowSurface")?,
            update_window_surface: sdl.symbol("SDL_UpdateWindowSurface")?,
            map_rgb: sdl.symbol("SDL_MapRGB")?,
            fill_rect: sdl.symbol("SDL_FillRect")?,
            upper_blit: sdl.symbol("SDL_UpperBlit")?,
            free_surface: sdl.symbol("SDL_FreeSurface")?,
            rw_from_file: sdl.symbol("SDL_RWFromFile")?,
            save_bmp_rw: sdl.symbol("SDL_SaveBMP_RW")?,
            ttf_init: ttf.symbol("TTF_Init")?,
            ttf_open_font: ttf.symbol("TTF_OpenFont")?,
            ttf_size_utf8: ttf.symbol("TTF_SizeUTF8")?,
            ttf_render_utf8_blended: ttf.symbol("TTF_RenderUTF8_Blended")?,
            img_load,
            _libraries: [Some(sdl), Some(ttf), image]
                .into_iter()
                .flatten()
                .collect(),
        })
    }

    fn error(&self) -> String {
        let message = unsafe { (self.get_error)() };
        if message.is_null() {
            "unknown SDL error".to_string()
        } else {
            unsafe { CStr::from_ptr(message) }
                .to_string_lossy()
                .into_owned()
        }
    }

    pub fn open_screen(&self, size: Option<(c_int, c_int)>) -> Result<Screen<'_>, String> {
        let (width, height) = size.unwrap_or((0, 0));
        if unsafe { (self.init)(INIT_VIDEO) } < 0 {
            return Err(format!("SDL_Init failed: {}", self.error()));
        }
        unsafe { (self.show_cursor)(CURSOR_HIDDEN) };
        let title = c"pocket-controller";
        let window = unsafe {
            (self.create_window)(
                title.as_ptr(),
                WINDOWPOS_UNDEFINED,
                WINDOWPOS_UNDEFINED,
                width,
                height,
                WINDOW_SHOWN,
            )
        };
        if window.is_null() {
            return Err(format!("SDL_CreateWindow failed: {}", self.error()));
        }
        let surface = unsafe { (self.get_window_surface)(window) };
        if surface.is_null() {
            return Err(format!("SDL_GetWindowSurface failed: {}", self.error()));
        }
        if unsafe { (self.ttf_init)() } < 0 {
            return Err(format!("TTF_Init failed: {}", self.error()));
        }
        Ok(Screen {
            sdl: self,
            window,
            surface,
        })
    }

    pub fn load_image(&self, path: &str) -> Result<*mut Surface, String> {
        let load = self.img_load.ok_or("SDL2_image is not available")?;
        let file = CString::new(path).map_err(|error| error.to_string())?;
        let image = unsafe { load(file.as_ptr()) };
        if image.is_null() {
            return Err(format!("could not load {path}: {}", self.error()));
        }
        Ok(image)
    }

    pub fn open_font(&self, path: &str, size: c_int) -> Result<*mut Font, String> {
        let file = CString::new(path).map_err(|error| error.to_string())?;
        let font = unsafe { (self.ttf_open_font)(file.as_ptr(), size) };
        if font.is_null() {
            return Err(format!("could not open font {path}: {}", self.error()));
        }
        Ok(font)
    }
}

impl Drop for Sdl {
    fn drop(&mut self) {
        unsafe { (self.quit)() };
    }
}

impl Screen<'_> {
    pub fn size(&self) -> (c_int, c_int) {
        let surface = unsafe { &*self.surface };
        (surface.w, surface.h)
    }

    pub fn fill(&self, area: Option<Rect>, color: Color) {
        let surface = unsafe { &*self.surface };
        let pixel = unsafe { (self.sdl.map_rgb)(surface.format, color.r, color.g, color.b) };
        let rect = area
            .as_ref()
            .map_or(std::ptr::null(), |rect| rect as *const Rect);
        unsafe { (self.sdl.fill_rect)(self.surface, rect, pixel) };
    }

    pub fn circle(&self, cx: c_int, cy: c_int, radius: c_int, color: Color) {
        for dy in -radius..=radius {
            let half = ((radius * radius - dy * dy) as f64).sqrt() as c_int;
            self.fill(
                Some(Rect {
                    x: cx - half,
                    y: cy + dy,
                    w: 2 * half + 1,
                    h: 1,
                }),
                color,
            );
        }
    }

    pub fn text_size(&self, font: *mut Font, text: &str) -> (c_int, c_int) {
        let Ok(text) = CString::new(text) else {
            return (0, 0);
        };
        let (mut width, mut height) = (0, 0);
        unsafe { (self.sdl.ttf_size_utf8)(font, text.as_ptr(), &mut width, &mut height) };
        (width, height)
    }

    pub fn text(&self, font: *mut Font, text: &str, x: c_int, y: c_int, color: Color) {
        let Ok(text) = CString::new(text) else {
            return;
        };
        let rendered = unsafe { (self.sdl.ttf_render_utf8_blended)(font, text.as_ptr(), color) };
        if rendered.is_null() {
            return;
        }
        let mut target = Rect { x, y, w: 0, h: 0 };
        unsafe {
            (self.sdl.upper_blit)(rendered, std::ptr::null(), self.surface, &mut target);
            (self.sdl.free_surface)(rendered);
        }
    }

    pub fn blit(&self, image: *mut Surface, x: c_int, y: c_int) {
        let mut target = Rect { x, y, w: 0, h: 0 };
        unsafe { (self.sdl.upper_blit)(image, std::ptr::null(), self.surface, &mut target) };
    }

    pub fn save_bmp(&self, path: &str) -> Result<(), String> {
        let file = CString::new(path).map_err(|error| error.to_string())?;
        let stream = unsafe { (self.sdl.rw_from_file)(file.as_ptr(), c"wb".as_ptr()) };
        if stream.is_null() {
            return Err(format!("could not open {path}: {}", self.sdl.error()));
        }
        if unsafe { (self.sdl.save_bmp_rw)(self.surface, stream, 1) } < 0 {
            return Err(format!("could not save {path}: {}", self.sdl.error()));
        }
        Ok(())
    }

    pub fn present(&self) {
        unsafe { (self.sdl.update_window_surface)(self.window) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_library_that_is_not_installed_is_reported_by_name() {
        let error = Library::open("libpocket-controller-absent.so.0")
            .err()
            .unwrap();
        assert!(
            error.contains("libpocket-controller-absent.so.0"),
            "{error}"
        );
    }

    #[test]
    fn a_name_with_a_nul_byte_is_an_error_not_a_crash() {
        assert!(Library::open("lib\0evil.so").is_err());
    }
}
