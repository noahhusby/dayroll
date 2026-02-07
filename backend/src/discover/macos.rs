#![cfg(target_os = "macos")]

use anyhow::Result;
use rusb::UsbContext;
use crate::model::{Candidate, Transport};

#[derive(Debug, Default, Clone)]
pub struct MacDiscovery {
    pub include_serial: bool,
    pub include_usb: bool,
}

impl MacDiscovery {
    pub fn new() -> Self {
        Self {
            include_serial: true,
            include_usb: true,
        }
    }

    pub fn discover(&self) -> Result<Vec<Candidate>> {
        let mut out = Vec::new();

        if self.include_serial {
            out.extend(discover_serial_ports()?);
        }

        if self.include_usb {
            out.extend(discover_usb_printer_class()?);
        }

        // Sort highest confidence first
        out.sort_by(|a, b| b.confidence.cmp(&a.confidence));
        Ok(out)
    }
}

#[cfg(feature = "mac-serial")]
fn discover_serial_ports() -> Result<Vec<Candidate>> {
    let mut out = Vec::new();

    for p in serialport::available_ports()? {
        // p.port_name is usually like: /dev/cu.usbserial-XXXX or /dev/tty.usbserial-XXXX
        let path = p.port_name.clone();

        let mut c = Candidate {
            transport: Transport::Serial { path: path.clone() },
            make_model: None,
            serial: None,
            vid: None,
            pid: None,
            confidence: 35,
            notes: vec![format!("serialport: {}", path)],
        };

        // Try to enrich with USB metadata if present
        match p.port_type {
            serialport::SerialPortType::UsbPort(info) => {
                c.vid = Some(format!("{:04x}", info.vid));
                c.pid = Some(format!("{:04x}", info.pid));
                c.serial = info.serial_number.clone();

                let mm = format!(
                    "{} {}",
                    info.manufacturer.clone().unwrap_or_default(),
                    info.product.clone().unwrap_or_default()
                )
                    .trim()
                    .to_string();

                if !mm.is_empty() {
                    c.make_model = Some(mm);
                    c.confidence = c.confidence.max(60);
                    c.notes.push("serialport: USB-backed serial device".into());
                }

                // Receipt printer keyword boost (non-gating)
                if let Some(mm) = &c.make_model {
                    let mm_l = mm.to_lowercase();
                    for kw in ["epson", "star", "bixolon", "citizen", "sewoo", "pos", "receipt", "thermal"] {
                        if mm_l.contains(kw) {
                            c.confidence = c.confidence.max(70);
                            c.notes.push(format!("make/model contains keyword '{kw}'"));
                            break;
                        }
                    }
                }
            }
            _ => {}
        }

        out.push(c);
    }

    Ok(out)
}

#[cfg(not(feature = "mac-serial"))]
fn discover_serial_ports() -> Result<Vec<Candidate>> {
    Ok(Vec::new())
}

/// Enumerate USB devices via libusb and find those exposing USB interface class 0x07 (printer).
#[cfg(feature = "mac-usb")]
fn discover_usb_printer_class() -> Result<Vec<Candidate>> {
    use rusb::{Context, Device, DeviceDescriptor};

    let ctx = Context::new()?;
    let devices = ctx.devices()?;

    let mut out = Vec::new();

    for dev in devices.iter() {
        let desc = dev.device_descriptor()?;
        if !device_has_printer_interface(&dev)? {
            continue;
        }

        let serial = read_usb_serial(&dev, &desc).ok().flatten();

        let mut c = Candidate {
            transport: Transport::UsbDevice {
                vid: desc.vendor_id(),
                pid: desc.product_id(),
                serial: serial.clone(),
            },
            make_model: None,
            serial,
            vid: Some(format!("{:04x}", desc.vendor_id())),
            pid: Some(format!("{:04x}", desc.product_id())),
            confidence: 80,
            notes: vec!["libusb: device exposes USB printer class interface (0x07)".into()],
        };

        // Optional: try to read manufacturer/product strings (best effort)
        if let Ok(Some((mfg, prod))) = read_usb_strings(&dev, &desc) {
            let mm = format!("{mfg} {prod}").trim().to_string();
            if !mm.is_empty() {
                c.make_model = Some(mm);
                c.confidence = c.confidence.max(85);
            }
        }

        out.push(c);
    }

    Ok(out)
}

#[cfg(not(feature = "mac-usb"))]
fn discover_usb_printer_class() -> Result<Vec<Candidate>> {
    Ok(Vec::new())
}

#[cfg(feature = "mac-usb")]
fn device_has_printer_interface(dev: &rusb::Device<rusb::Context>) -> anyhow::Result<bool> {
    let cfg = match dev.active_config_descriptor() {
        Ok(c) => c,
        Err(_) => return Ok(false),
    };

    for iface in cfg.interfaces() {
        for setting in iface.descriptors() {
            if setting.class_code() == 0x07 {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

#[cfg(feature = "mac-usb")]
fn read_usb_strings(
    dev: &rusb::Device<rusb::Context>,
    desc: &rusb::DeviceDescriptor,
) -> Result<Option<(String, String)>> {
    let handle = match dev.open() {
        Ok(h) => h,
        Err(_) => return Ok(None),
    };

    let timeout = std::time::Duration::from_millis(100);

    let mfg = desc
        .manufacturer_string_index()
        .and_then(|i| handle.read_string_descriptor_ascii(i).ok());

    let prod = desc
        .product_string_index()
        .and_then(|i| handle.read_string_descriptor_ascii(i).ok());

    match (mfg, prod) {
        (Some(a), Some(b)) => Ok(Some((a, b))),
        _ => Ok(None),
    }
}

#[cfg(feature = "mac-usb")]
fn read_usb_serial(
    dev: &rusb::Device<rusb::Context>,
    desc: &rusb::DeviceDescriptor,
) -> Result<Option<String>> {
    let handle = match dev.open() {
        Ok(h) => h,
        Err(_) => return Ok(None),
    };

    let timeout = std::time::Duration::from_millis(100);

    let serial = desc
        .serial_number_string_index()
        .and_then(|i| handle.read_string_descriptor_ascii(i).ok());

    Ok(serial)
}
