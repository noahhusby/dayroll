use crate::model::Candidate;

#[cfg(target_os = "linux")]
mod linux;

pub trait DiscoveryProvider {
    fn discover_default(&self) -> anyhow::Result<Vec<Candidate>> {
        #[cfg(target_os = "linux")]
        {
            linux::LinuxDiscovery::default().discover()
        }

        #[cfg(not(target_os = "linux"))]
        {
            Ok(Vec::new())
        }
    }
}

#[derive(Default)]
pub struct DefaultDiscovery;

impl DiscoveryProvider for DefaultDiscovery {}