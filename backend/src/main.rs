mod model;
mod discover;

use std::path::Path;
use axum::{routing::get, Json, Router};
use escpos::driver::{ConsoleDriver, Driver, FileDriver};
use escpos::printer::Printer;
use escpos::printer_options::PrinterOptions;
use escpos::utils::{DebugMode, JustifyMode, Protocol, RealTimeStatusRequest, RealTimeStatusResponse, UnderlineMode};
use serde_json::{Value, json};
use log::info;
use serde::Serialize;
use crate::discover::{DefaultDiscovery, DiscoveryProvider};

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
        let provider = DefaultDiscovery::default();
        let printers = provider.discover_default()?;
        for p in printers {
            println!("{:#?}", p);
        }
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
