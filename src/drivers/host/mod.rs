//! Driver for the Linux kernel usbip-host module
//! (/drivers/usb/usbip/stub_main.c)

use std::path::Path;

use crate::drivers::{SysfsIoError, write_sysfs_attribute};

#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct UsbipHost {
    // TODO
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchListOperation {
    Add,
    Remove,
}

impl UsbipHost {
    /// Adds the bus ID to usbip-host's match list. This is needed because when we
    /// write to the `bind` attribute provided by the linux driver core, it won't
    /// actually probe the usbip-host driver unless it's device id match table is
    /// compatible with the device. It would be inefficient if usbip-host advertised
    /// a match table that matched all devices, so we need to dyanmically modify it
    /// at runtime before attempting to bind the driver to the device.
    ///
    /// /// TODO: move into UsbipHost driver (not a standard sysfs driver operation)
    pub fn update_bus_id_match_list(
        bus_id: &str,
        operation: MatchListOperation,
    ) -> Result<(), SysfsIoError> {
        let path = Path::new("/sys/bus/usb/drivers/usbip-host/match_busid");

        let buf = match operation {
            MatchListOperation::Add => format!("add {bus_id}"),
            MatchListOperation::Remove => format!("del {bus_id}"),
        };

        write_sysfs_attribute(path, buf)
    }

    /// Asks the usbip-host driver to make a call into usbcore to try and
    /// initiate the driver matching process and bind the device back to its old
    /// driver. Fails if the device could not be bound back to its original
    /// driver.
    pub fn trigger_device_rebind(bus_id: &str) -> Result<(), SysfsIoError> {
        let path = Path::new("/sys/bus/usb/drivers/usbip-host/rebind");

        // TODO: should do the same type of error matching that we do in
        // bind_usb_driver to provide better error messages? rebind_store in
        // stub_main.c returns whatever error was returned by device_attach so
        // the codes are the same as bind_store in the driver core.

        write_sysfs_attribute(path, bus_id)
    }
}
