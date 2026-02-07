use std::path::Path;
use axum::{routing::get, Json, Router};
use escpos::driver::{ConsoleDriver, Driver, FileDriver};
use escpos::printer::Printer;
use escpos::printer_options::PrinterOptions;
use escpos::utils::{DebugMode, JustifyMode, Protocol, RealTimeStatusRequest, RealTimeStatusResponse, UnderlineMode};
use glob::glob;
use serde_json::{Value, json};
use log::info;
use serde::Serialize;
use udev::Enumerator;

async fn integrations() -> Json<Value> {
    Json(json!({
        "response_code": 200,
        "error": false,
        "integrations": [
            {
                "slug": "remote_calendar",
                "instance_id": "48265F8E-8A69-4AD3-B46B-713094B7E240",
                "enabled": true,
            }
        ]
    }))
}

#[derive(Debug, Clone, Serialize)]
pub enum Transport {
    UsbLp { path: String },
    Serial { path: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct PrinterCandidate {
    pub transport: Transport,
    pub make_model: Option<String>,
    pub serial: Option<String>,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub confidence: u8, // 0..=100
    pub notes: Vec<String>,
}

pub fn discover_escpos_candidates() -> Result<Vec<PrinterCandidate>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();

    // 1) USB printer class nodes: /dev/usb/lp*
    for entry in glob("/dev/usb/lp*")? {
        if let Ok(path) = entry {
            let path_str = path.to_string_lossy().to_string();
            let mut cand = candidate_from_devnode(&path_str)?;
            cand.transport = Transport::UsbLp { path: path_str };
            // If it exists as /dev/usb/lpX, it’s *very* likely a USB printer class.
            cand.confidence = cand.confidence.max(80);
            cand.notes.push("Found /dev/usb/lp* node (USB printer class)".into());
            out.push(cand);
        }
    }

    // 2) Serial-like nodes: /dev/ttyUSB*, /dev/ttyACM*
    for pat in ["/dev/ttyUSB*", "/dev/ttyACM*"] {
        for entry in glob(pat)? {
            if let Ok(path) = entry {
                let path_str = path.to_string_lossy().to_string();
                let mut cand = candidate_from_devnode(&path_str)?;
                cand.transport = Transport::Serial { path: path_str };
                cand.confidence = cand.confidence.max(40);
                cand.notes.push(format!("Found serial device node ({pat})"));
                out.push(cand);
            }
        }
    }

    // Optional: de-dup by path
    out.sort_by_key(|c| match &c.transport {
        Transport::UsbLp { path } => path.clone(),
        Transport::Serial { path } => path.clone(),
    });
    out.dedup_by(|a, b| transport_path(a) == transport_path(b));

    // Optional: sort by confidence descending
    out.sort_by(|a, b| b.confidence.cmp(&a.confidence));

    Ok(out)
}

fn transport_path(c: &PrinterCandidate) -> &str {
    match &c.transport {
        Transport::UsbLp { path } => path.as_str(),
        Transport::Serial { path } => path.as_str(),
    }
}

fn candidate_from_devnode(devnode: &str) -> Result<PrinterCandidate, Box<dyn std::error::Error>> {
    let mut cand = PrinterCandidate {
        transport: Transport::UsbLp { path: devnode.to_string() }, // overwritten by caller
        make_model: None,
        serial: None,
        vid: None,
        pid: None,
        confidence: 10,
        notes: vec![],
    };

    // udev enumeration across subsystems that commonly include these nodes
    // We'll try to match by devnode path.
    let mut enumerator = Enumerator::new()?;
    enumerator.match_subsystem("usb")?;
    enumerator.match_subsystem("tty")?;
    enumerator.match_subsystem("usbmisc")?;
    enumerator.match_subsystem("printer")?; // some distros expose this

    for dev in enumerator.scan_devices()? {
        if let Some(node) = dev.devnode() {
            if node.to_string_lossy() == devnode {
                // Friendly names (may or may not exist)
                let vendor = dev.property_value("ID_VENDOR_FROM_DATABASE")
                    .or_else(|| dev.property_value("ID_VENDOR"))
                    .map(|v| v.to_string_lossy().to_string());

                let model = dev.property_value("ID_MODEL_FROM_DATABASE")
                    .or_else(|| dev.property_value("ID_MODEL"))
                    .map(|m| m.to_string_lossy().to_string());

                if vendor.is_some() || model.is_some() {
                    cand.make_model = Some(format!(
                        "{} {}",
                        vendor.clone().unwrap_or_default(),
                        model.clone().unwrap_or_default()
                    ).trim().to_string());
                    cand.confidence = cand.confidence.max(50);
                }

                cand.serial = dev.property_value("ID_SERIAL_SHORT")
                    .or_else(|| dev.property_value("ID_SERIAL"))
                    .map(|s| s.to_string_lossy().to_string());

                cand.vid = dev.property_value("ID_VENDOR_ID")
                    .map(|v| v.to_string_lossy().to_string());
                cand.pid = dev.property_value("ID_MODEL_ID")
                    .map(|p| p.to_string_lossy().to_string());

                // Heuristic: check USB interface class includes printer class (07)
                if let Some(ifaces) = dev.property_value("ID_USB_INTERFACES") {
                    let ifaces = ifaces.to_string_lossy();
                    // Example contains ":0701.." for printer class
                    if ifaces.contains(":0701") || ifaces.contains(":0700") || ifaces.contains(":07") {
                        cand.confidence = cand.confidence.max(85);
                        cand.notes.push("udev: ID_USB_INTERFACES indicates printer class (07)".into());
                    }
                }

                // Heuristic: ESC/POS brands often include keywords, but don’t require them
                if let Some(mm) = &cand.make_model {
                    let mm_l = mm.to_lowercase();
                    for kw in ["epson", "star", "bixolon", "citizen", "sewoo", "zjiang", "xprinter", "pos", "thermal"] {
                        if mm_l.contains(kw) {
                            cand.confidence = cand.confidence.max(70);
                            cand.notes.push(format!("make/model contains keyword '{kw}'"));
                            break;
                        }
                    }
                }

                return Ok(cand);
            }
        }
    }

    // If udev didn't match devnode, keep generic info
    cand.notes.push("No udev metadata match for devnode; leaving generic candidate".into());
    Ok(cand)
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new("/dev/usb/lp1");
    let driver = FileDriver::open(&path)?;
    //let driver = ConsoleDriver::open(true);
    let mut printer = Printer::new(driver.clone(), Protocol::default(), Some(PrinterOptions::default()));


    let command = std::env::args().nth(1).expect("No command given");
    if command == "print" {
        info!("Printing!");
        printer
            .debug_mode(Some(DebugMode::Dec))
            .init()?
            .smoothing(true)?
            .bold(true)?
            .underline(UnderlineMode::Single)?
            .writeln("Bold underline")?
            .justify(JustifyMode::CENTER)?
            .reverse(true)?
            .bold(false)?
            .writeln("Hello world - Reverse")?
            .feed()?
            .justify(JustifyMode::RIGHT)?
            .reverse(false)?
            .underline(UnderlineMode::None)?
            .size(2, 3)?
            .writeln("Hello world - Normal")?
            .print_cut()?;  // print() or print_cut() is mandatory to send the data to the printer
    } else if command == "detect" {
        let cands = discover_escpos_candidates()?;
        println!("{}", serde_json::to_string_pretty(&cands)?);
    } else if command == "status" {
        printer
            .debug_mode(Some(DebugMode::Dec))
            .real_time_status(RealTimeStatusRequest::Printer)?
            .real_time_status(RealTimeStatusRequest::RollPaperSensor)?
            .send_status()?;

        let mut buf = [0; 1];
        driver.read(&mut buf)?;

        let status = RealTimeStatusResponse::parse(RealTimeStatusRequest::Printer, buf[0])?;
        println!(
            "Printer online: {}",
            status.get(&RealTimeStatusResponse::Online).unwrap_or(&false)
        );
    }
    // let app = Router::new()
    //     .route("/", get(|| async { "Root get request!" }))
    //     .route("/integrations", get(integrations));
    //
    // let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    // axum::serve(listener, app).await.unwrap();
    Ok(())
}
