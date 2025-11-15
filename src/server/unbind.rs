use std::{ffi::OsStr, io};

use crate::drivers::{DriverUnbindingError, SysfsIoError, unbind_usb_driver};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to create udev context ({0})")]
    CreareUdevContext(io::Error),
    #[error("USB device not found ({0})")]
    UdevDeviceNotFound(io::Error),

    #[error("USB device was not already bound to `usbip-host` driver")]
    NotAlreadyBound,
    #[error(
        "USB driver `{driver}` could not be unbound from device with bus ID `{bus_id}`: {source}"
    )]
    UnbindingDriver {
        source: DriverUnbindingError,
        driver: String,
        bus_id: String,
    },

    #[error("Cannot write to `usbip-host` device to update device ID match list: {0}")]
    UpdatingMatchList(SysfsIoError),
}

pub fn unbind_device(local_bus_id: &str) -> Result<(), Error> {
    let context = udev::Udev::new().map_err(Error::CreareUdevContext)?;

    let usb_device = udev::Device::from_subsystem_sysname_with_context(
        context.clone(),
        "usb".into(),
        local_bus_id.into(),
    )
    .map_err(Error::UdevDeviceNotFound)?;

    if usb_device
        .driver()
        .is_none_or(|d| d != OsStr::new("usbip-host"))
    {
        return Err(Error::NotAlreadyBound);
    }

    unbind_usb_driver(OsStr::new("usbip-host"), local_bus_id).map_err(|e| {
        Error::UnbindingDriver {
            source: e,
            driver: "usbip-host".into(),
            bus_id: local_bus_id.into(),
        }
    })?;

    Ok(())
}
