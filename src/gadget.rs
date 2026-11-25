use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub trait GadgetFs {
    fn exists(&self, path: &Path) -> bool;
    fn read(&self, path: &Path) -> io::Result<String>;
    fn write(&self, path: &Path, value: &str) -> io::Result<()>;
    fn create_dir(&self, path: &Path) -> io::Result<()>;
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
    fn symlink(&self, target: &Path, link: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn list(&self, path: &Path) -> io::Result<Vec<String>>;
    fn list_links(&self, path: &Path) -> io::Result<Vec<String>>;
    fn unmount(&self, path: &Path) -> io::Result<()>;
}

pub const OWN_GADGET_NAME: &str = "pocket-controller";
pub const FUNCTIONFS_INSTANCE: &str = "pocket-controller";
const DEFAULT_CONFIG: &str = "c.1";
const ENGLISH: &str = "0x409";
const UNBIND: &str = "\n";
const DEVICE_ATTRIBUTES: [&str; 7] = [
    "idVendor",
    "idProduct",
    "bcdDevice",
    "bcdUSB",
    "bDeviceClass",
    "bDeviceSubClass",
    "bDeviceProtocol",
];
const DEVICE_STRINGS: [&str; 3] = ["manufacturer", "product", "serialnumber"];

#[derive(Clone, Debug)]
pub struct Layout {
    pub gadgets: PathBuf,
    pub controllers: PathBuf,
    pub marker: PathBuf,
    pub functionfs_mount: PathBuf,
    pub settle: Duration,
}

impl Layout {
    pub fn system() -> Self {
        Self {
            gadgets: PathBuf::from("/sys/kernel/config/usb_gadget"),
            controllers: PathBuf::from("/sys/class/udc"),
            marker: PathBuf::from("/tmp/pocket-controller.port"),
            functionfs_mount: PathBuf::from("/tmp/pocket-controller-functionfs"),
            settle: Duration::from_secs(1),
        }
    }
}

fn function_name() -> String {
    format!("ffs.{FUNCTIONFS_INSTANCE}")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub vendor: u16,
    pub product: u16,
    pub release: u16,
    pub device_class: u8,
    pub manufacturer: &'static str,
    pub product_name: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub gadget: String,
    pub controller: String,
    pub config: String,
    pub created_by_us: bool,
    pub attributes: Vec<(String, String)>,
    pub strings: Vec<(String, String)>,
    pub links: Vec<String>,
}

impl Snapshot {
    pub fn serialize(&self) -> String {
        let mut lines = vec![
            format!("gadget={}", self.gadget),
            format!("controller={}", self.controller),
            format!("config={}", self.config),
            format!("created_by_us={}", self.created_by_us),
        ];
        lines.extend(
            self.attributes
                .iter()
                .map(|(name, value)| format!("attribute:{name}={value}")),
        );
        lines.extend(
            self.strings
                .iter()
                .map(|(name, value)| format!("string:{name}={value}")),
        );
        lines.extend(self.links.iter().map(|link| format!("link={link}")));
        lines.join("\n") + "\n"
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut snapshot = Self::default();
        for line in text.lines() {
            let (key, value) = line.split_once('=')?;
            let value = value.to_string();
            match key {
                "gadget" => snapshot.gadget = value,
                "controller" => snapshot.controller = value,
                "config" => snapshot.config = value,
                "created_by_us" => snapshot.created_by_us = value == "true",
                "link" => snapshot.links.push(value),
                _ => {
                    if let Some(name) = key.strip_prefix("attribute:") {
                        snapshot.attributes.push((name.to_string(), value));
                    } else if let Some(name) = key.strip_prefix("string:") {
                        snapshot.strings.push((name.to_string(), value));
                    } else {
                        return None;
                    }
                }
            }
        }
        let is_plain_name = |value: &str| {
            !value.is_empty() && !value.contains('/') && value != "." && value != ".."
        };
        let names_are_plain = [&snapshot.gadget, &snapshot.controller, &snapshot.config]
            .into_iter()
            .chain(&snapshot.links)
            .all(|name| is_plain_name(name))
            && snapshot
                .attributes
                .iter()
                .chain(&snapshot.strings)
                .all(|(name, _)| is_plain_name(name));
        names_are_plain.then_some(snapshot)
    }

    fn gadget_dir(&self, layout: &Layout) -> PathBuf {
        layout.gadgets.join(&self.gadget)
    }

    fn config_dir(&self, layout: &Layout) -> PathBuf {
        self.gadget_dir(layout).join("configs").join(&self.config)
    }

    fn function_dir(&self, layout: &Layout) -> PathBuf {
        self.gadget_dir(layout)
            .join("functions")
            .join(function_name())
    }
}

#[derive(Debug)]
pub struct PortLease {
    snapshot: Snapshot,
    last_unbind: Cell<Option<Instant>>,
}

fn with_path(error: io::Error, action: &str, path: &Path) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("could not {action} {}: {error}", path.display()),
    )
}

fn write_attr(fs: &dyn GadgetFs, path: &Path, value: &str) -> io::Result<()> {
    fs.write(path, value)
        .map_err(|error| with_path(error, "write", path))
}

fn ensure_dir(fs: &dyn GadgetFs, path: &Path) -> io::Result<()> {
    match fs.create_dir(path) {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => {
            Err(with_path(error, "create", path))
        }
        _ => Ok(()),
    }
}

fn ignore_missing(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

fn read_trimmed(fs: &dyn GadgetFs, path: &Path) -> Option<String> {
    fs.read(path)
        .ok()
        .map(|value| value.trim_end_matches('\n').to_string())
}

fn bound_controller(fs: &dyn GadgetFs, gadget: &Path) -> Option<String> {
    read_trimmed(fs, &gadget.join("UDC")).filter(|value| !value.trim().is_empty())
}

fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names
}

pub fn first_controller(fs: &dyn GadgetFs, layout: &Layout) -> Option<String> {
    sorted(fs.list(&layout.controllers).ok()?)
        .into_iter()
        .next()
}

fn find_firmware_gadget(fs: &dyn GadgetFs, layout: &Layout) -> Option<(String, String)> {
    sorted(fs.list(&layout.gadgets).ok()?)
        .into_iter()
        .find_map(|name| {
            let controller = bound_controller(fs, &layout.gadgets.join(&name))?;
            Some((name, controller))
        })
}

fn snapshot_firmware(
    fs: &dyn GadgetFs,
    layout: &Layout,
    gadget: String,
    controller: String,
) -> io::Result<Snapshot> {
    let gadget_dir = layout.gadgets.join(&gadget);
    let config = sorted(fs.list(&gadget_dir.join("configs"))?)
        .into_iter()
        .next()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("gadget {gadget} has no configuration"),
            )
        })?;
    let attributes = DEVICE_ATTRIBUTES
        .iter()
        .filter_map(|name| Some((name.to_string(), read_trimmed(fs, &gadget_dir.join(name))?)))
        .collect();
    let strings = DEVICE_STRINGS
        .iter()
        .filter_map(|name| {
            Some((
                name.to_string(),
                read_trimmed(fs, &gadget_dir.join("strings").join(ENGLISH).join(name))?,
            ))
        })
        .collect();
    let links = sorted(fs.list_links(&gadget_dir.join("configs").join(&config))?)
        .into_iter()
        .filter(|link| *link != function_name())
        .collect();
    Ok(Snapshot {
        gadget,
        controller,
        config,
        created_by_us: false,
        attributes,
        strings,
        links,
    })
}

fn detach_function(fs: &dyn GadgetFs, layout: &Layout, snapshot: &Snapshot) -> io::Result<()> {
    let gadget_dir = snapshot.gadget_dir(layout);
    if bound_controller(fs, &gadget_dir).is_some() {
        write_attr(fs, &gadget_dir.join("UDC"), UNBIND)?;
    }
    ignore_missing(fs.remove_file(&snapshot.config_dir(layout).join(function_name())))?;
    let _ = fs.unmount(&layout.functionfs_mount);
    let function = snapshot.function_dir(layout);
    ignore_missing(fs.remove_dir(&function)).map_err(|error| with_path(error, "remove", &function))
}

fn restore(fs: &dyn GadgetFs, layout: &Layout, snapshot: &Snapshot) -> io::Result<()> {
    detach_function(fs, layout, snapshot)?;
    let gadget_dir = snapshot.gadget_dir(layout);
    if snapshot.created_by_us {
        for dir in [
            snapshot.config_dir(layout).join("strings").join(ENGLISH),
            snapshot.config_dir(layout),
            gadget_dir.join("strings").join(ENGLISH),
            gadget_dir.clone(),
        ] {
            ignore_missing(fs.remove_dir(&dir))
                .map_err(|error| with_path(error, "remove", &dir))?;
        }
        return Ok(());
    }
    for (name, value) in &snapshot.attributes {
        write_attr(fs, &gadget_dir.join(name), value)?;
    }
    for (name, value) in &snapshot.strings {
        write_attr(
            fs,
            &gadget_dir.join("strings").join(ENGLISH).join(name),
            value,
        )?;
    }
    let config_dir = snapshot.config_dir(layout);
    std::thread::sleep(layout.settle);
    for link in &snapshot.links {
        let target = gadget_dir.join("functions").join(link);
        match fs.symlink(&target, &config_dir.join(link)) {
            Err(error) if error.kind() != io::ErrorKind::AlreadyExists => {
                return Err(with_path(error, "relink", &config_dir.join(link)));
            }
            _ => {}
        }
    }
    write_attr(fs, &gadget_dir.join("UDC"), &snapshot.controller)
}

pub fn recover_stale(fs: &dyn GadgetFs, layout: &Layout) -> io::Result<()> {
    let Ok(text) = fs.read(&layout.marker) else {
        return Ok(());
    };
    if let Some(snapshot) = Snapshot::parse(&text)
        && fs.exists(&snapshot.gadget_dir(layout))
    {
        restore(fs, layout, &snapshot)?;
    }
    ignore_missing(fs.remove_file(&layout.marker))
}

fn create_own_gadget(fs: &dyn GadgetFs, layout: &Layout) -> io::Result<Snapshot> {
    let controller = first_controller(fs, layout).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "this device has no USB device controller",
        )
    })?;
    let snapshot = Snapshot {
        gadget: OWN_GADGET_NAME.to_string(),
        controller,
        config: DEFAULT_CONFIG.to_string(),
        created_by_us: true,
        ..Snapshot::default()
    };
    let gadget_dir = snapshot.gadget_dir(layout);
    let config_dir = snapshot.config_dir(layout);
    for dir in [
        gadget_dir.clone(),
        gadget_dir.join("strings"),
        gadget_dir.join("strings").join(ENGLISH),
        gadget_dir.join("configs"),
        config_dir.clone(),
        config_dir.join("strings"),
        config_dir.join("strings").join(ENGLISH),
        gadget_dir.join("functions"),
    ] {
        ensure_dir(fs, &dir)?;
    }
    write_attr(
        fs,
        &config_dir
            .join("strings")
            .join(ENGLISH)
            .join("configuration"),
        "Controller",
    )?;
    write_attr(fs, &config_dir.join("MaxPower"), "500")?;
    Ok(snapshot)
}

impl PortLease {
    pub fn take(fs: &dyn GadgetFs, layout: &Layout) -> io::Result<Self> {
        recover_stale(fs, layout)?;
        let snapshot = match find_firmware_gadget(fs, layout) {
            Some((gadget, controller)) => snapshot_firmware(fs, layout, gadget, controller)?,
            None => create_own_gadget(fs, layout)?,
        };
        write_attr(fs, &layout.marker, &snapshot.serialize())?;
        let gadget_dir = snapshot.gadget_dir(layout);
        if bound_controller(fs, &gadget_dir).is_some() {
            write_attr(fs, &gadget_dir.join("UDC"), UNBIND)?;
        }
        let last_unbind = Cell::new(Some(Instant::now()));
        let config_dir = snapshot.config_dir(layout);
        for link in &snapshot.links {
            fs.remove_file(&config_dir.join(link))
                .map_err(|error| with_path(error, "unlink", &config_dir.join(link)))?;
        }
        Ok(Self {
            snapshot,
            last_unbind,
        })
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn controller(&self) -> &str {
        &self.snapshot.controller
    }

    pub fn serial(&self) -> Option<&str> {
        self.snapshot
            .strings
            .iter()
            .find(|(name, value)| name == "serialnumber" && !value.is_empty())
            .map(|(_, value)| value.as_str())
    }

    pub fn attach(
        &self,
        fs: &dyn GadgetFs,
        layout: &Layout,
        identity: &Identity,
        serial: &str,
    ) -> io::Result<PathBuf> {
        let gadget_dir = self.snapshot.gadget_dir(layout);
        let hex16 = |value: u16| format!("0x{value:04x}");
        let hex8 = |value: u8| format!("0x{value:02x}");
        for (name, value) in [
            ("idVendor", hex16(identity.vendor)),
            ("idProduct", hex16(identity.product)),
            ("bcdDevice", hex16(identity.release)),
            ("bcdUSB", hex16(0x0200)),
            ("bDeviceClass", hex8(identity.device_class)),
            ("bDeviceSubClass", hex8(identity.device_class)),
            ("bDeviceProtocol", hex8(identity.device_class)),
        ] {
            write_attr(fs, &gadget_dir.join(name), &value)?;
        }
        let strings = gadget_dir.join("strings").join(ENGLISH);
        ensure_dir(fs, &strings)?;
        write_attr(fs, &strings.join("manufacturer"), identity.manufacturer)?;
        write_attr(fs, &strings.join("product"), identity.product_name)?;
        write_attr(fs, &strings.join("serialnumber"), serial)?;
        let function = self.snapshot.function_dir(layout);
        ensure_dir(fs, &function)?;
        Ok(function)
    }

    pub fn bind(&self, fs: &dyn GadgetFs, layout: &Layout) -> io::Result<()> {
        if let Some(unbound_at) = self.last_unbind.get() {
            std::thread::sleep(layout.settle.saturating_sub(unbound_at.elapsed()));
        }
        let link = self.snapshot.config_dir(layout).join(function_name());
        match fs.symlink(&self.snapshot.function_dir(layout), &link) {
            Err(error) if error.kind() != io::ErrorKind::AlreadyExists => {
                return Err(with_path(error, "link", &link));
            }
            _ => {}
        }
        write_attr(
            fs,
            &self.snapshot.gadget_dir(layout).join("UDC"),
            &self.snapshot.controller,
        )
    }

    pub fn unbind(&self, fs: &dyn GadgetFs, layout: &Layout) -> io::Result<()> {
        let gadget_dir = self.snapshot.gadget_dir(layout);
        if bound_controller(fs, &gadget_dir).is_some() {
            write_attr(fs, &gadget_dir.join("UDC"), UNBIND)?;
            self.last_unbind.set(Some(Instant::now()));
        }
        Ok(())
    }

    pub fn detach(&self, fs: &dyn GadgetFs, layout: &Layout) -> io::Result<()> {
        detach_function(fs, layout, &self.snapshot)
    }

    pub fn give_back(self, fs: &dyn GadgetFs, layout: &Layout) -> io::Result<()> {
        restore(fs, layout, &self.snapshot)?;
        ignore_missing(fs.remove_file(&layout.marker))
    }
}

pub fn controller_state(fs: &dyn GadgetFs, layout: &Layout, controller: &str) -> Option<String> {
    read_trimmed(fs, &layout.controllers.join(controller).join("state"))
}

#[cfg(test)]
pub mod fake {
    use super::GadgetFs;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io;
    use std::path::{Path, PathBuf};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Node {
        Dir,
        File(String),
        Link(PathBuf),
    }

    const DEFAULT_GROUPS: [&str; 4] = ["configs", "functions", "strings", "os_desc"];

    #[derive(Default)]
    pub struct FakeFs {
        nodes: RefCell<BTreeMap<PathBuf, Node>>,
        pub log: RefCell<Vec<String>>,
        pub refuse_new_gadgets: bool,
    }

    fn not_found(path: &Path) -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, path.display().to_string())
    }

    impl FakeFs {
        pub fn with_dirs(paths: &[&str]) -> Self {
            let fs = Self::default();
            for path in paths {
                let mut current = PathBuf::new();
                for part in Path::new(path).components() {
                    current.push(part);
                    fs.nodes.borrow_mut().insert(current.clone(), Node::Dir);
                }
            }
            fs
        }

        pub fn set(&self, path: &str, value: &str) {
            self.nodes
                .borrow_mut()
                .insert(PathBuf::from(path), Node::File(value.to_string()));
        }

        pub fn link(&self, link: &str, target: &str) {
            self.nodes
                .borrow_mut()
                .insert(PathBuf::from(link), Node::Link(PathBuf::from(target)));
        }

        pub fn get(&self, path: &str) -> Option<String> {
            match self.nodes.borrow().get(Path::new(path)) {
                Some(Node::File(value)) => Some(value.clone()),
                _ => None,
            }
        }

        pub fn has(&self, path: &str) -> bool {
            self.nodes.borrow().contains_key(Path::new(path))
        }

        pub fn snapshot(&self) -> BTreeMap<PathBuf, String> {
            self.nodes
                .borrow()
                .iter()
                .map(|(path, node)| (path.clone(), format!("{node:?}")))
                .collect()
        }

        fn children(&self, path: &Path) -> Vec<(PathBuf, Node)> {
            self.nodes
                .borrow()
                .iter()
                .filter(|(child, _)| child.parent() == Some(path))
                .map(|(child, node)| (child.clone(), node.clone()))
                .collect()
        }

        fn record(&self, entry: String) {
            self.log.borrow_mut().push(entry);
        }
    }

    impl GadgetFs for FakeFs {
        fn exists(&self, path: &Path) -> bool {
            self.nodes.borrow().contains_key(path)
        }

        fn read(&self, path: &Path) -> io::Result<String> {
            match self.nodes.borrow().get(path) {
                Some(Node::File(value)) => Ok(format!("{value}\n")),
                _ => Err(not_found(path)),
            }
        }

        fn write(&self, path: &Path, value: &str) -> io::Result<()> {
            let parent_is_dir = path
                .parent()
                .is_some_and(|parent| self.nodes.borrow().get(parent) == Some(&Node::Dir));
            if !parent_is_dir {
                return Err(not_found(path));
            }
            let stored = value.trim_end_matches('\n').to_string();
            self.record(format!("write {} = {:?}", path.display(), stored));
            self.nodes
                .borrow_mut()
                .insert(path.to_path_buf(), Node::File(stored));
            Ok(())
        }

        fn create_dir(&self, path: &Path) -> io::Result<()> {
            if self.exists(path) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    path.display().to_string(),
                ));
            }
            if path.parent().is_some_and(|parent| !self.exists(parent)) {
                return Err(not_found(path));
            }
            let is_new_gadget =
                path.parent().and_then(|p| p.file_name()) == Some("usb_gadget".as_ref());
            if is_new_gadget && self.refuse_new_gadgets {
                return Err(io::Error::from_raw_os_error(libc::ENOMEM));
            }
            self.record(format!("mkdir {}", path.display()));
            self.nodes
                .borrow_mut()
                .insert(path.to_path_buf(), Node::Dir);
            Ok(())
        }

        fn remove_dir(&self, path: &Path) -> io::Result<()> {
            if self.nodes.borrow().get(path) != Some(&Node::Dir) {
                return Err(not_found(path));
            }
            for (child, node) in self.children(path) {
                let is_default_group = child
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| DEFAULT_GROUPS.contains(&name));
                let busy = match node {
                    Node::Link(_) => true,
                    Node::Dir => {
                        !is_default_group
                            || !self
                                .children(&child)
                                .iter()
                                .all(|(_, node)| matches!(node, Node::File(_)))
                    }
                    Node::File(_) => false,
                };
                if busy {
                    return Err(io::Error::new(
                        io::ErrorKind::DirectoryNotEmpty,
                        path.display().to_string(),
                    ));
                }
            }
            self.record(format!("rmdir {}", path.display()));
            self.nodes
                .borrow_mut()
                .retain(|candidate, _| !candidate.starts_with(path));
            Ok(())
        }

        fn symlink(&self, target: &Path, link: &Path) -> io::Result<()> {
            if !self.exists(target) {
                return Err(not_found(target));
            }
            if self.exists(link) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    link.display().to_string(),
                ));
            }
            self.record(format!("link {}", link.display()));
            self.nodes
                .borrow_mut()
                .insert(link.to_path_buf(), Node::Link(target.to_path_buf()));
            Ok(())
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            let removed = self.nodes.borrow_mut().remove(path);
            match removed {
                Some(Node::Dir) => {
                    self.nodes
                        .borrow_mut()
                        .insert(path.to_path_buf(), Node::Dir);
                    Err(io::Error::other("is a directory"))
                }
                Some(_) => {
                    self.record(format!("rm {}", path.display()));
                    Ok(())
                }
                None => Err(not_found(path)),
            }
        }

        fn list(&self, path: &Path) -> io::Result<Vec<String>> {
            if !self.exists(path) {
                return Err(not_found(path));
            }
            Ok(self
                .children(path)
                .into_iter()
                .filter_map(|(child, _)| {
                    child.file_name().and_then(|n| n.to_str()).map(String::from)
                })
                .collect())
        }

        fn list_links(&self, path: &Path) -> io::Result<Vec<String>> {
            if !self.exists(path) {
                return Err(not_found(path));
            }
            Ok(self
                .children(path)
                .into_iter()
                .filter(|(_, node)| matches!(node, Node::Link(_)))
                .filter_map(|(child, _)| {
                    child.file_name().and_then(|n| n.to_str()).map(String::from)
                })
                .collect())
        }

        fn unmount(&self, path: &Path) -> io::Result<()> {
            self.record(format!("umount {}", path.display()));
            Ok(())
        }
    }

    pub const G1: &str = "/sys/kernel/config/usb_gadget/g1";
    pub const CONTROLLER: &str = "5100000.udc-controller";

    pub fn brick() -> FakeFs {
        let mut fs = FakeFs::with_dirs(&[
            "/sys/kernel/config/usb_gadget/g1/functions/ffs.adb",
            "/sys/kernel/config/usb_gadget/g1/configs/c.1/strings/0x409",
            "/sys/kernel/config/usb_gadget/g1/strings/0x409",
            "/sys/kernel/config/usb_gadget/g1/os_desc",
            "/sys/class/udc/5100000.udc-controller",
            "/tmp",
        ]);
        fs.refuse_new_gadgets = true;
        for (name, value) in [
            ("UDC", CONTROLLER),
            ("idVendor", "0x18d1"),
            ("idProduct", "0xd002"),
            ("bcdDevice", "0x0409"),
            ("bcdUSB", "0x0000"),
            ("bDeviceClass", "0x00"),
            ("bDeviceSubClass", "0x00"),
            ("bDeviceProtocol", "0x00"),
            ("strings/0x409/manufacturer", "TRIMUI"),
            ("strings/0x409/product", "TRIMUI ADB"),
            ("strings/0x409/serialnumber", "2c000c81b5c28711c9d"),
            ("configs/c.1/MaxPower", "500"),
        ] {
            fs.set(&format!("{G1}/{name}"), value);
        }
        fs.link(
            &format!("{G1}/configs/c.1/ffs.adb"),
            &format!("{G1}/functions/ffs.adb"),
        );
        fs.set(
            "/sys/class/udc/5100000.udc-controller/state",
            "not attached",
        );
        fs
    }

    pub fn test_layout() -> super::Layout {
        super::Layout {
            settle: std::time::Duration::ZERO,
            ..super::Layout::system()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{CONTROLLER, FakeFs, G1, brick, test_layout};
    use super::*;

    fn xbox() -> Identity {
        Identity {
            vendor: 0x045e,
            product: 0x028e,
            release: 0x0114,
            device_class: 0xff,
            manufacturer: "Pocket Controller",
            product_name: "Controller",
        }
    }

    fn position(fs: &FakeFs, needle: &str) -> usize {
        fs.log
            .borrow()
            .iter()
            .position(|entry| entry.contains(needle))
            .unwrap_or_else(|| {
                panic!(
                    "no log entry containing {needle:?} in {:#?}",
                    fs.log.borrow()
                )
            })
    }

    fn borrow_and_attach(fs: &FakeFs, layout: &Layout) -> PortLease {
        let lease = PortLease::take(fs, layout).unwrap();
        lease.attach(fs, layout, &xbox(), "1234").unwrap();
        lease.bind(fs, layout).unwrap();
        lease
    }

    #[test]
    fn taking_the_port_records_the_firmware_gadget_before_touching_it() {
        let fs = brick();
        let layout = test_layout();
        let lease = PortLease::take(&fs, &layout).unwrap();
        assert_eq!(lease.controller(), CONTROLLER);
        assert_eq!(lease.serial(), Some("2c000c81b5c28711c9d"));
        let marker = fs.get("/tmp/pocket-controller.port").unwrap();
        assert!(marker.contains("gadget=g1"));
        assert!(marker.contains("link=ffs.adb"));
        assert!(marker.contains("attribute:idVendor=0x18d1"));
        assert!(marker.contains("string:product=TRIMUI ADB"));
        assert!(position(&fs, "write /tmp/pocket-controller.port") < position(&fs, "g1/UDC"));
        assert_eq!(fs.get(&format!("{G1}/UDC")).as_deref(), Some(""));
        assert!(!fs.has(&format!("{G1}/configs/c.1/ffs.adb")));
    }

    #[test]
    fn the_firmware_gadget_becomes_the_controller() {
        let fs = brick();
        let layout = test_layout();
        borrow_and_attach(&fs, &layout);
        assert_eq!(fs.get(&format!("{G1}/idVendor")).as_deref(), Some("0x045e"));
        assert_eq!(
            fs.get(&format!("{G1}/idProduct")).as_deref(),
            Some("0x028e")
        );
        assert_eq!(
            fs.get(&format!("{G1}/bDeviceClass")).as_deref(),
            Some("0xff")
        );
        assert_eq!(
            fs.get(&format!("{G1}/strings/0x409/product")).as_deref(),
            Some("Controller")
        );
        assert!(fs.has(&format!("{G1}/configs/c.1/ffs.pocket-controller")));
        assert!(!fs.has(&format!("{G1}/configs/c.1/ffs.adb")));
        assert_eq!(fs.get(&format!("{G1}/UDC")).as_deref(), Some(CONTROLLER));
    }

    #[test]
    fn giving_the_port_back_restores_the_firmware_gadget_exactly() {
        let fs = brick();
        let before = fs.snapshot();
        let layout = test_layout();
        let lease = borrow_and_attach(&fs, &layout);
        lease.give_back(&fs, &layout).unwrap();
        assert_eq!(fs.snapshot(), before);
    }

    #[test]
    fn switching_kinds_detaches_and_reattaches_without_touching_adb() {
        let fs = brick();
        let layout = test_layout();
        let lease = borrow_and_attach(&fs, &layout);
        lease.detach(&fs, &layout).unwrap();
        assert!(!fs.has(&format!("{G1}/functions/ffs.pocket-controller")));
        assert_eq!(fs.get(&format!("{G1}/UDC")).as_deref(), Some(""));
        lease.attach(&fs, &layout, &xbox(), "1234").unwrap();
        lease.bind(&fs, &layout).unwrap();
        assert!(fs.has(&format!("{G1}/configs/c.1/ffs.pocket-controller")));
        assert!(fs.has(&format!("{G1}/functions/ffs.adb")));
    }

    #[test]
    fn binding_waits_for_the_host_to_notice_the_unbind() {
        let fs = brick();
        let layout = Layout {
            settle: Duration::from_millis(80),
            ..Layout::system()
        };
        let lease = PortLease::take(&fs, &layout).unwrap();
        lease.attach(&fs, &layout, &xbox(), "1234").unwrap();
        let started = Instant::now();
        lease.bind(&fs, &layout).unwrap();
        assert!(started.elapsed() >= Duration::from_millis(70));
        let started = Instant::now();
        lease.bind(&fs, &layout).unwrap();
        assert!(started.elapsed() < Duration::from_millis(40));
    }

    #[test]
    fn a_crashed_run_is_restored_at_the_next_start() {
        let fs = brick();
        let before = fs.snapshot();
        let layout = test_layout();
        let lease = borrow_and_attach(&fs, &layout);
        std::mem::forget(lease);
        recover_stale(&fs, &layout).unwrap();
        assert_eq!(fs.snapshot(), before);
    }

    #[test]
    fn recovery_without_a_marker_changes_nothing() {
        let fs = brick();
        recover_stale(&fs, &test_layout()).unwrap();
        assert!(fs.log.borrow().is_empty());
    }

    #[test]
    fn a_marker_naming_a_path_is_discarded() {
        let fs = brick();
        fs.set(
            "/tmp/pocket-controller.port",
            "gadget=../g1\ncontroller=5100000.udc-controller\nconfig=c.1\ncreated_by_us=false\n",
        );
        recover_stale(&fs, &test_layout()).unwrap();
        assert!(!fs.has("/tmp/pocket-controller.port"));
        assert_eq!(fs.get(&format!("{G1}/UDC")).as_deref(), Some(CONTROLLER));
    }

    #[test]
    fn snapshot_survives_serialization() {
        let snapshot = Snapshot {
            gadget: "g1".into(),
            controller: CONTROLLER.into(),
            config: "c.1".into(),
            created_by_us: false,
            attributes: vec![("idVendor".into(), "0x18d1".into())],
            strings: vec![("product".into(), "TRIMUI ADB".into())],
            links: vec!["ffs.adb".into()],
        };
        assert_eq!(Snapshot::parse(&snapshot.serialize()), Some(snapshot));
    }

    #[test]
    fn without_a_firmware_gadget_an_own_gadget_is_created_and_removed() {
        let fs = FakeFs::with_dirs(&[
            "/sys/kernel/config/usb_gadget",
            "/sys/class/udc/musb-hdrc.2.auto",
            "/tmp",
        ]);
        let before = fs.snapshot();
        let layout = test_layout();
        let lease = borrow_and_attach(&fs, &layout);
        assert_eq!(lease.controller(), "musb-hdrc.2.auto");
        assert!(fs.has(
            "/sys/kernel/config/usb_gadget/pocket-controller/configs/c.1/ffs.pocket-controller"
        ));
        lease.give_back(&fs, &layout).unwrap();
        assert_eq!(fs.snapshot(), before);
    }

    #[test]
    fn no_controller_is_a_plain_error() {
        let fs = FakeFs::with_dirs(&["/sys/kernel/config/usb_gadget", "/sys/class/udc", "/tmp"]);
        let error = PortLease::take(&fs, &test_layout()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "this device has no USB device controller"
        );
    }
}
