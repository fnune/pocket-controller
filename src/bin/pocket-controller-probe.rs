use std::fs::OpenOptions;
use std::io::Write;
use std::process::ExitCode;

use pocket_controller::gadget::{Identity, Layout, PortLease};
use pocket_controller::system::RealFs;
use pocket_controller::xinput;

const USAGE: &str = "usage: pocket-controller-probe (int|bulk)-(in|out):MAXPACKET ...\n\
example: pocket-controller-probe int-in:32 int-out:32\n\
       pocket-controller-probe --brightness [LEVEL]";

#[derive(Clone, Copy, Debug)]
struct EndpointSpec {
    interrupt: bool,
    input: bool,
    max_packet: u16,
}

fn parse_spec(text: &str) -> Result<EndpointSpec, String> {
    let (kind, max_packet) = text.split_once(':').ok_or(USAGE)?;
    let (transfer, direction) = kind.split_once('-').ok_or(USAGE)?;
    Ok(EndpointSpec {
        interrupt: match transfer {
            "int" => true,
            "bulk" => false,
            _ => return Err(USAGE.to_string()),
        },
        input: match direction {
            "in" => true,
            "out" => false,
            _ => return Err(USAGE.to_string()),
        },
        max_packet: max_packet.parse().map_err(|_| USAGE.to_string())?,
    })
}

fn descriptors(specs: &[EndpointSpec], high_speed: bool) -> Vec<u8> {
    let mut bytes = vec![9, 4, 0, 0, specs.len() as u8, 0xff, 0x5d, 0x01, 0];
    for (index, spec) in specs.iter().enumerate() {
        let number = index as u8 + 1;
        let address = if spec.input { 0x80 | number } else { number };
        let (attributes, max_packet, interval) = match (spec.interrupt, high_speed) {
            (true, false) => (3, spec.max_packet, 1),
            (true, true) => (3, spec.max_packet, 4),
            (false, false) => (2, spec.max_packet.min(64), 0),
            (false, true) => (2, 512, 0),
        };
        let [low, high] = max_packet.to_le_bytes();
        bytes.extend_from_slice(&[7, 5, address, attributes, low, high, interval]);
    }
    bytes
}

fn run() -> Result<(), String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some("--brightness") {
        if let Some(level) = arguments.get(1) {
            let level = level.parse().map_err(|_| USAGE.to_string())?;
            pocket_controller::system::write_brightness(level)
                .map_err(|error| error.to_string())?;
        }
        let level =
            pocket_controller::system::read_brightness().map_err(|error| error.to_string())?;
        println!("brightness: {level}");
        return Ok(());
    }
    let specs = std::env::args()
        .skip(1)
        .map(|argument| parse_spec(&argument))
        .collect::<Result<Vec<_>, _>>()?;
    if specs.is_empty() {
        return Err(USAGE.to_string());
    }
    let count = 1 + specs.len() as u32;
    let blob = xinput::functionfs_blob(
        &descriptors(&specs, false),
        &descriptors(&specs, true),
        count,
    );
    let fs = RealFs;
    let layout = Layout::system();
    let lease = PortLease::take(&fs, &layout).map_err(|error| error.to_string())?;
    let identity = Identity {
        vendor: 0x1d6b,
        product: 0x0105,
        release: 0x0100,
        device_class: 0x00,
        manufacturer: "Pocket Controller",
        product_name: "pocket-controller probe",
    };
    let result = (|| -> Result<(), String> {
        lease
            .attach(&fs, &layout, &identity, "probe")
            .map_err(|error| error.to_string())?;
        std::fs::create_dir_all(&layout.functionfs_mount).map_err(|error| error.to_string())?;
        let status = std::process::Command::new("mount")
            .args([
                "-t",
                "functionfs",
                pocket_controller::gadget::FUNCTIONFS_INSTANCE,
            ])
            .arg(&layout.functionfs_mount)
            .status()
            .map_err(|error| error.to_string())?;
        if !status.success() {
            return Err("mount failed".into());
        }
        let mut ep0 = OpenOptions::new()
            .read(true)
            .write(true)
            .open(layout.functionfs_mount.join("ep0"))
            .map_err(|error| error.to_string())?;
        ep0.write_all(&blob)
            .map_err(|error| format!("descriptors refused: {error}"))?;
        ep0.write_all(&xinput::functionfs_strings())
            .map_err(|error| format!("strings refused: {error}"))?;
        let bound = lease
            .bind(&fs, &layout)
            .map_err(|error| format!("bind failed: {error}"));
        let _ = lease.unbind(&fs, &layout);
        drop(ep0);
        bound
    })();
    let given_back = lease
        .give_back(&fs, &layout)
        .map_err(|error| error.to_string());
    println!(
        "{}: {}",
        std::env::args().skip(1).collect::<Vec<_>>().join(" "),
        match &result {
            Ok(()) => "binds".to_string(),
            Err(error) => error.clone(),
        }
    );
    given_back
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
