use std::{io, path::Path};

use crate::{
    client::VHCI_STATE_PATH,
    drivers::vhci_hcd::{Error as VhciHcdError, VhciDeviceStatus, VhciHcd},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    VhciHcd(#[from] VhciHcdError),
    #[error("Port number is greater than the max port number advertised by vhci_hcd")]
    InvalidPortNumber,

    #[error("Failed to save `vhci_hcd` state to the file-system ({0})")]
    FsIo(io::Error),
}

pub fn detach_device(port: u16) -> Result<(), Error> {
    let mut vhci_hcd = VhciHcd::open()?;

    if port >= vhci_hcd.total_port_count() {
        return Err(Error::InvalidPortNumber);
    }

    for device in vhci_hcd.cached_imported_devices() {
        if device.port == port && device.status() == VhciDeviceStatus::NotConnected {
            tracing::info!("port {port} is already detached");
            return Ok(());
        }
    }

    remove_record(port).map_err(Error::FsIo)?;

    vhci_hcd.detach_device(port)?;

    tracing::info!("port {port} detached successfully");

    Ok(())
}

pub fn remove_record(port: u16) -> io::Result<()> {
    use std::fs;

    let state_path = Path::new(VHCI_STATE_PATH);
    let port_path = state_path.join(format!("port{port}"));

    if let Err(e) = fs::remove_file(port_path) {
        if e.kind() != io::ErrorKind::NotFound {
            return Err(e);
        }
    }

    if let Err(e) = fs::remove_dir(state_path) {
        if e.kind() != io::ErrorKind::DirectoryNotEmpty && e.kind() != io::ErrorKind::NotFound {
            return Err(e);
        }

        if e.kind() == io::ErrorKind::NotFound {
            tracing::warn!("vhci_hcd state directory not found")
        }
    }

    Ok(())
}
