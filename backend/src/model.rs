#[derive(Debug, Clone)]
pub enum Transport {
    UsbLp { path: String },
    Serial { path: String },
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub transport: Transport,
    pub make_model: Option<String>,
    pub serial: Option<String>,
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub confidence: u8,
    pub notes: Vec<String>,
}

impl Candidate {
    pub fn transport_path(&self) -> Option<&str> {
        match &self.transport {
            Transport::UsbLp { path } => Some(path.as_str()),
            Transport::Serial { path } => Some(path.as_str()),
        }
    }
}