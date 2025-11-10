use crate::drivers::vhci::{
    Error as VhciHcdError, VhciDeviceStatus, VhciHcd,
    state::{FsStateError, delete_connection_record},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    VhciHcd(#[from] VhciHcdError),
    #[error("Port number is greater than the max port number advertised by vhci_hcd")]
    InvalidPortNumber,

    #[error(transparent)]
    FsState(FsStateError),
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

    delete_connection_record(port).map_err(Error::FsState)?;

    vhci_hcd.detach_device(port)?;

    tracing::info!("port {port} detached successfully");

    Ok(())
}
