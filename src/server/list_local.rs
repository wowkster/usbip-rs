use std::io;

use serde::Serialize;

use crate::{
    UsbDeviceInfo,
    hwdb::{get_class_display_strings, get_device_display_strings},
    util::{UsbInfoExtractError, extract_usb_info_from_udev_device},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to create udev context ({0})")]
    CreatingUdevContext(io::Error),
    #[error("Failed to create udev enumerator ({0})")]
    CreatingUdevEnumerator(io::Error),
    #[error("Failed to enumerato USB devices with udev ({0})")]
    EnumeratingUdevDevices(io::Error),

    #[error("Failed to query USB device with bus ID `{bus_id}` ({error})")]
    UsbInfoExtraction {
        bus_id: String,
        error: UsbInfoExtractError,
    },
}

#[derive(Debug, Serialize)]
pub struct ExportableDevice {
    pub device_info: UsbDeviceInfo,

    pub vendor: Option<String>,
    pub product: Option<String>,

    pub class: Option<String>,
    pub sub_class: Option<String>,
    pub protocol: Option<String>,
}

/// Lists all local (exportable) devices. This includes all USB devices which
/// are not hubs and are not virtual (attached by vhci_hcd) devices.
pub fn list_local_devices() -> Result<Vec<ExportableDevice>, Error> {
    #[cfg(feature = "runtime-hwdb")]
    let hwdb = udev::Hwdb::new()?;

    let udev = udev::Udev::new().map_err(Error::CreatingUdevContext)?;

    let mut enumerator =
        udev::Enumerator::with_udev(udev).map_err(Error::CreatingUdevEnumerator)?;

    enumerator
        .match_subsystem("usb")
        .map_err(Error::CreatingUdevEnumerator)?;
    enumerator
        .nomatch_attribute("bDeviceClass", "09")
        .map_err(Error::CreatingUdevEnumerator)?;

    let mut results = Vec::new();

    for dev in enumerator
        .scan_devices()
        .map_err(Error::EnumeratingUdevDevices)?
    {
        // FIXME: the udev crate does not expose the functionality that libudev
        // does for wildcard nomatch. once that is PR is merged, we should
        // update this to use it. (https://github.com/Smithay/udev-rs/issues/58)
        if dev.attribute_value("bInterfaceNumber").is_some() {
            continue;
        }

        // TODO: Ignore devices attached to vhci_hcd

        let device_info =
            extract_usb_info_from_udev_device(&dev).map_err(|e| Error::UsbInfoExtraction {
                bus_id: dev.sysname().to_string_lossy().into(),
                error: e,
            })?;

        let (vendor, product) = get_device_display_strings(
            #[cfg(feature = "runtime-hwdb")]
            &hwdb,
            device_info.id_vendor,
            device_info.id_product,
        );

        let (class, sub_class, protocol) = get_class_display_strings(
            #[cfg(feature = "runtime-hwdb")]
            &hwdb,
            device_info.b_device_class,
            device_info.b_device_sub_class,
            device_info.b_device_protocol,
        );

        results.push(ExportableDevice {
            device_info,
            vendor,
            product,
            class,
            sub_class,
            protocol,
        });
    }

    Ok(results)
}
