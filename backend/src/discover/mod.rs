use log::{error, info};
use crate::model::Candidate;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;

pub trait DiscoveryProvider {
    fn discover_default(&self) -> anyhow::Result<Vec<Candidate>> {
        error!("Detecting on crackkkoss2!");
        #[cfg(target_os = "linux")]
        {
            return linux::LinuxDiscovery::default().discover();
        }

        #[cfg(target_os = "macos")]
        {
            error!("Detecting on crackkkoss!");
            return macos::MacDiscovery::default().discover();
        }

        Ok(Vec::new())
    }
}

#[derive(Default)]
pub struct DefaultDiscovery;

impl DiscoveryProvider for DefaultDiscovery {}