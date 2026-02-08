#![cfg(target_os = "linux")]

use anyhow::Result;
use glob::glob;
use std::collections::HashMap;

use crate::model::{Candidate, Transport};

#[cfg(feature = "linux-udev")]
use udev::Enumerator;

#[derive(Debug, Default, Clone)]
pub struct LinuxDiscovery {
    pub include_serial: bool,
    pub use_udev: bool,
}

impl LinuxDiscovery {
    pub fn new() -> Self {
        Self {
            include_serial: true,
            use_udev: true,
        }
    }

    pub fn discover(&self) -> Result<Vec<Candidate>> {
        let mut cands = Vec::new();

        cands.extend(scan_usb_lp_nodes()?);

        if self.include_serial {
            cands.extend(scan_serial_nodes()?);
        }

        if self.use_udev {
            enrich_with_udev(&mut cands)?;
        }

        dedup_by_transport_path(&mut cands);
        cands.sort_by(|a, b| b.confidence.cmp(&a.confidence));

        Ok(cands)
    }
}

/// Scan /dev/usb/lp* (USB printer class via usblp kernel driver).
fn scan_usb_lp_nodes() -> Result<Vec<Candidate>> {
    let mut out = Vec::new();
    for entry in glob("/dev/usb/lp*")? {
        let Ok(path) = entry else { continue };
        let p = path.to_string_lossy().to_string();

        out.push(Candidate {
            transport: Transport::UsbLp { path: p.clone() },
            make_model: None,
            serial: None,
            vid: None,
            pid: None,
            confidence: 80,
            notes: vec!["Found /dev/usb/lp* node (USB printer class)".into()],
        });
    }
    Ok(out)
}

/// Scan serial devices that are commonly used for receipt printers.
fn scan_serial_nodes() -> Result<Vec<Candidate>> {
    let mut out = Vec::new();
    for pat in ["/dev/ttyUSB*", "/dev/ttyACM*"] {
        for entry in glob(pat)? {
            let Ok(path) = entry else { continue };
            let p = path.to_string_lossy().to_string();

            out.push(Candidate {
                transport: Transport::Serial { path: p.clone() },
                make_model: None,
                serial: None,
                vid: None,
                pid: None,
                confidence: 40,
                notes: vec![format!("Found serial device node ({pat})")],
            });
        }
    }
    Ok(out)
}

/// Best-effort enrichment using udev properties.
/// This helps identify make/model/serial/VID/PID and whether the USB interface includes printer class.
#[cfg(feature = "linux-udev")]
fn enrich_with_udev(cands: &mut [Candidate]) -> Result<()> {
    // Build a map: devnode -> udev properties
    let devmap = build_udev_devnode_map()?;

    for cand in cands.iter_mut() {
        // Only device-node transports have a path we can match in udev.
        let Some(devnode) = cand.transport_path() else { continue };

        let Some(props) = devmap.get(devnode) else { continue };

        // Friendly vendor/model names
        let vendor = props
            .get("ID_VENDOR_FROM_DATABASE")
            .or_else(|| props.get("ID_VENDOR"))
            .cloned();
        let model = props
            .get("ID_MODEL_FROM_DATABASE")
            .or_else(|| props.get("ID_MODEL"))
            .cloned();

        if cand.make_model.is_none() && (vendor.is_some() || model.is_some()) {
            let mm = format!(
                "{} {}",
                vendor.clone().unwrap_or_default(),
                model.clone().unwrap_or_default()
            )
                .trim()
                .to_string();
            if !mm.is_empty() {
                cand.make_model = Some(mm);
                cand.confidence = cand.confidence.max(55);
            }
        }

        // Serial + VID/PID if available
        if cand.serial.is_none() {
            cand.serial = props
                .get("ID_SERIAL_SHORT")
                .or_else(|| props.get("ID_SERIAL"))
                .cloned();
        }
        if cand.vid.is_none() {
            cand.vid = props.get("ID_VENDOR_ID").cloned();
        }
        if cand.pid.is_none() {
            cand.pid = props.get("ID_MODEL_ID").cloned();
        }

        // USB printer class heuristic via ID_USB_INTERFACES
        if let Some(ifaces) = props.get("ID_USB_INTERFACES") {
            // Usually includes ":0701" for printer interface class/subclass.
            if ifaces.contains(":0701") || ifaces.contains(":0700") || ifaces.contains(":07") {
                cand.confidence = cand.confidence.max(90);
                cand.notes.push("udev: ID_USB_INTERFACES indicates USB printer class (07)".into());
            }
        }

        // Keyword heuristic: doesn't gate, only boosts confidence a bit.
        if let Some(mm) = &cand.make_model {
            let mm_l = mm.to_lowercase();
            for kw in [
                "epson", "star", "bixolon", "citizen", "sewoo", "zjiang", "xprinter", "pos",
                "receipt", "thermal",
            ] {
                if mm_l.contains(kw) {
                    cand.confidence = cand.confidence.max(70);
                    cand.notes.push(format!("make/model contains keyword '{kw}'"));
                    break;
                }
            }
        }
    }

    Ok(())
}

#[cfg(not(feature = "linux-udev"))]
fn enrich_with_udev(_cands: &mut [Candidate]) -> Result<()> {
    Ok(())
}

/// Enumerate udev devices and build a map from devnode -> property map.
/// We scan multiple subsystems because distros vary in where devnodes appear.
#[cfg(feature = "linux-udev")]
fn build_udev_devnode_map() -> Result<HashMap<String, HashMap<String, String>>> {
    let mut map: HashMap<String, HashMap<String, String>> = HashMap::new();

    let mut en = Enumerator::new()?;
    // These cover most real-world cases for printer/tty nodes.
    for subsystem in ["usb", "tty", "usbmisc", "printer", "lp"] {
        let _ = en.match_subsystem(subsystem);
    }

    for dev in en.scan_devices()? {
        let Some(node) = dev.devnode() else { continue };
        let node = node.to_string_lossy().to_string();

        let mut props = HashMap::new();
        for p in dev.properties() {
            let Some(name) = p.name().to_str() else { continue };
            let val = p.value().to_string_lossy().to_string();
            props.insert(name.to_string(), val);
        }

        // Only keep entries that have at least something useful.
        if props.contains_key("ID_MODEL")
            || props.contains_key("ID_MODEL_FROM_DATABASE")
            || props.contains_key("ID_VENDOR")
            || props.contains_key("ID_VENDOR_FROM_DATABASE")
            || props.contains_key("ID_USB_INTERFACES")
        {
            map.insert(node, props);
        }
    }

    Ok(map)
}

fn dedup_by_transport_path(cands: &mut Vec<Candidate>) {
    cands.sort_by_key(|c| c.transport_path().unwrap_or("").to_string());
    cands.dedup_by(|a, b| a.transport_path() == b.transport_path());
}
