use std::{ffi::OsStr, io};

use crate::drivers::{
    DriverBindingError, DriverUnbindingError, SysfsIoError, bind_usb_driver,
    host::{MatchListOperation, UsbipHost},
    unbind_usb_driver,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to create udev context ({0})")]
    CreareUdevContext(io::Error),
    #[error("USB device not found ({0})")]
    UdevDeviceNotFound(io::Error),

    #[error("Bind loop detected. Device is attached by `vhci_hcd` driver.")]
    AlreadyBoundToVhci,

    // #[error("Could not unbind other driver from device on bus ID `{0}`")]
    #[error("Failed to query udev attribute `{attribute}` from device with bus ID `{bus_id}`")]
    FailedToGetUdevDeviceAttribute { bus_id: String, attribute: String },
    /// Hub devices may not be unbound from their drivers and cannot be bound to usbip_host
    #[error("Cannot bind USB hub device on bus ID `{0}`")]
    CannotBindHub(String),

    #[error("Device on bus ID `{0}` is already bound to `usbip-host`")]
    AlreadyBoundToUsbipHost(String),

    #[error("USB driver `{driver}` could not be bound to device with bus ID `{bus_id}`: {source}")]
    BindingDriver {
        source: DriverBindingError,
        driver: String,
        bus_id: String,
    },
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

/// Binds a USB device to the usbip-host driver. If the device is already bound
/// to another driver it will be unbound before rebinding to usbip-host.
///
/// NOTE: must be a device bus ID (`x-y` or `x-y.z.w`), and NOT an interface bus
/// ID (`x-y:z.w` or `x-y.z:w.a`)
///
/// NOTE: Not all device are allowed to be bound here. Specifically, hub devices
/// and any devices already attached with vhci_hcd may not be exported using
/// usbip-host. Leaf devices created by a hub may be exported as normal.
pub fn bind_device(local_bus_id: &str) -> Result<(), Error> {
    let context = udev::Udev::new().map_err(Error::CreareUdevContext)?;

    let usb_device = udev::Device::from_subsystem_sysname_with_context(
        context.clone(),
        "usb".into(),
        local_bus_id.into(),
    )
    .map_err(Error::UdevDeviceNotFound)?;

    // Check if this device was attached by the `vhci_hcd` host controller
    // driver. If this is the case, we technically could still bind it and
    // re-export it, but the usbip-host kernel module doesn't allow this.
    let dev_path = usb_device.devpath().to_str().unwrap();
    if dev_path.contains("vhci_hcd") {
        // TODO: there is probably a better way to do this which would reduce
        // weird driver naming conflicts. We can probably traverse the device
        // parent hierarchy to look for the hcd driver being vhci_hcd.

        return Err(Error::AlreadyBoundToVhci);
    }

    // Make sure that this device is not a USB hub device. These are special
    // devices which the kernel treats differently and the usbip-host driver
    // does not support them at this time.
    let b_device_class = usb_device.attribute_value("bDeviceClass").ok_or_else(|| {
        Error::FailedToGetUdevDeviceAttribute {
            bus_id: local_bus_id.into(),
            attribute: "bDeviceClass".into(),
        }
    })?;
    if b_device_class == OsStr::new("09") {
        return Err(Error::CannotBindHub(local_bus_id.into()));
    }

    // If the device doesn't have a driver bound to it already, we can just
    // continue forwards with binding to usbip-host
    if let Some(driver) = usb_device.driver() {
        // Check that this device is not already bound to the usbip-host driver (we
        // don't try to rebind in this case).
        if driver == OsStr::new("usbip-host") {
            return Err(Error::AlreadyBoundToUsbipHost(local_bus_id.into()));
        }

        unbind_usb_driver(&driver, local_bus_id).map_err(|e| Error::UnbindingDriver {
            source: e,
            driver: driver.to_string_lossy().into(),
            bus_id: local_bus_id.into(),
        })?;
    }

    UsbipHost::update_bus_id_match_list(local_bus_id, MatchListOperation::Add)
        .map_err(Error::UpdatingMatchList)?;

    if let Err(e) = bind_usb_driver(OsStr::new("usbip-host"), local_bus_id) {
        // try to remove, but if we encounter an error, there isnt much we can
        // do. if we successfully added the first time then its likely that this
        // will succeed.
        let _ = UsbipHost::update_bus_id_match_list(local_bus_id, MatchListOperation::Remove);

        return Err(Error::BindingDriver {
            source: e,
            driver: "usbip-host".into(),
            bus_id: local_bus_id.into(),
        });
    };

    Ok(())
}
