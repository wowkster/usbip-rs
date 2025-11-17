use std::io::{self};

use crate::{
    UsbDeviceInfo,
    drivers::vhci::{
        Error as VhciHcdError, HubSpeed, VhciDeviceStatus, VhciHcd,
        state::{ConnectionRecord, read_connection_record},
    },
    hwdb::get_device_display_strings,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    VhciHcdDriver(#[from] VhciHcdError),

    #[error(
        "An I/O error occurred while querying string descriptors from imported USB device with bus ID `{1}` ({0})"
    )]
    QueryingLocalUsbDevice(io::Error, String),
    #[error(
        "Failed to get value for udev attribute `{attribute}` from USB device with bus ID `{bus_id}`"
    )]
    MissingUdevAttribute { bus_id: String, attribute: String },
}

#[derive(Debug, serde::Serialize)]
pub struct ImportedDevice {
    pub port: u16,
    pub hub_speed: HubSpeed,
    pub status: VhciDeviceStatus,

    pub remote_host: Option<String>,
    pub remote_port: Option<u16>,
    pub remote_bus_id: Option<String>,

    pub url: Option<String>,

    pub remote_bus_num: u16,
    pub remote_dev_num: u16,

    pub vendor: Option<String>,
    pub product: Option<String>,

    pub manufacturer_string: String,
    pub product_string: String,

    pub local_device_info: UsbDeviceInfo,
}

pub fn list_imported_devices() -> Result<Vec<ImportedDevice>, Error> {
    #[cfg(feature = "runtime-hwdb")]
    let hwdb = udev::Hwdb::new()?;
    let vhci_hdc = VhciHcd::open()?;

    let mut res = Vec::new();

    for imported_dev in vhci_hdc.cached_imported_devices() {
        let Some(local_dev) = imported_dev.connected_device() else {
            continue;
        };

        let (url, remote_host, remote_port, remote_bus_id) =
            match read_connection_record(imported_dev.port) {
                Ok(ConnectionRecord { host, port, bus_id }) => (
                    Some(format!("usbip://{host}:{port}/{bus_id}")),
                    Some(host),
                    Some(port),
                    Some(bus_id),
                ),
                Err(e) => {
                    tracing::error!("failed to read state for port {}: {e}", imported_dev.port);
                    Default::default()
                }
            };

        let (manufacturer_string, product_string) =
            query_device_string_descriptors(&local_dev.device.bus_id)?;
        let (vendor, product) = get_device_display_strings(
            #[cfg(feature = "runtime-hwdb")]
            &hwdb,
            local_dev.device.id_vendor,
            local_dev.device.id_product,
        );

        res.push(ImportedDevice {
            port: imported_dev.port,
            hub_speed: imported_dev.hub_speed,
            status: imported_dev.status(),
            remote_host,
            remote_port,
            remote_bus_id,
            url,
            remote_bus_num: imported_dev.remote_bus_num(),
            remote_dev_num: imported_dev.remote_dev_num(),
            vendor,
            product,
            manufacturer_string,
            product_string,
            local_device_info: local_dev.device.clone(),
        });
    }

    Ok(res)
}

fn query_device_string_descriptors(local_bus_id: &str) -> Result<(String, String), Error> {
    let dev = udev::Device::from_subsystem_sysname("usb".into(), local_bus_id.into())
        .map_err(|e| Error::QueryingLocalUsbDevice(e, local_bus_id.into()))?;

    let manufacturer = dev
        .attribute_value("manufacturer")
        .ok_or_else(|| Error::MissingUdevAttribute {
            bus_id: local_bus_id.into(),
            attribute: "manufacturer".into(),
        })?
        .to_string_lossy()
        .to_string();
    let product = dev
        .attribute_value("product")
        .ok_or_else(|| Error::MissingUdevAttribute {
            bus_id: local_bus_id.into(),
            attribute: "product".into(),
        })?
        .to_string_lossy()
        .to_string();

    Ok((manufacturer, product))
}
