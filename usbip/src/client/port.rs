use std::{
    io::{self, Read},
    path::Path,
};

use crate::{
    client::VHCI_STATE_PATH,
    drivers::vhci_hcd::{Error as VhciHcdError, HubSpeed, UsbSpeed, VhciDeviceStatus, VhciHcd},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    VhciHcdDriver(#[from] VhciHcdError),

    #[error("Failed to read file-system `vhci_hcd` state file for device on port {1}")]
    FsIo(io::Error, u16),
    #[error("Failed to parse file-system `vhci_hcd` state file for device on port {1}")]
    FsStateParsing(sscanf::Error, u16),

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

    pub local_sys_path: String,
    pub local_bus_id: String,

    pub local_bus_num: u16,
    pub local_dev_num: u16,

    pub remote_bus_num: u16,
    pub remote_dev_num: u16,

    pub vendor_display: Option<String>,
    pub product_display: Option<String>,

    pub manufacturer: String,
    pub product: String,

    pub speed: UsbSpeed,

    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,

    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,
    pub b_configuration_value: u8,
    pub b_num_configurations: u8,
    pub b_num_interfaces: u8,
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

        let (url, remote_host, remote_port, remote_bus_id) = match read_record(imported_dev.port) {
            Ok(RemoteConnectionInfo {
                remote_host,
                remote_port,
                remote_bus_id,
            }) => (
                Some(format!(
                    "usbip://{remote_host}:{remote_port}/{remote_bus_id}"
                )),
                Some(remote_host),
                Some(remote_port),
                Some(remote_bus_id),
            ),
            Err(e) => {
                tracing::error!("failed to read state for port {}: {e}", imported_dev.port);
                Default::default()
            }
        };

        let local_sys_path = local_dev
            .device
            .path
            .as_c_str()
            .expect("previously decoded string should be valid")
            .to_string_lossy()
            .to_string();
        let local_bus_id = local_dev
            .device
            .bus_id
            .as_c_str()
            .expect("previously decoded string should be valid")
            .to_string_lossy()
            .to_string();

        let (manufacturer, product) = get_device_strings(&local_bus_id)?;
        let (vendor_display, product_display) = get_device_display_strings(
            #[cfg(feature = "runtime-hwdb")]
            &hwdb,
            local_dev.device.id_vendor,
            local_dev.device.id_product,
        );

        let speed = UsbSpeed::try_from(local_dev.device.speed)
            .expect("already validated in vhci_hcd driver");

        res.push(ImportedDevice {
            port: imported_dev.port,
            hub_speed: imported_dev.hub_speed,
            status: imported_dev.status(),
            remote_host,
            remote_port,
            remote_bus_id,
            url,
            local_sys_path,
            local_bus_id,
            local_bus_num: local_dev.device.bus_num as _,
            local_dev_num: local_dev.device.dev_num as _,
            remote_bus_num: imported_dev.remote_bus_num(),
            remote_dev_num: imported_dev.remote_dev_num(),
            vendor_display,
            product_display,
            manufacturer,
            product,
            speed,
            id_vendor: local_dev.device.id_vendor,
            id_product: local_dev.device.id_product,
            bcd_device: local_dev.device.bcd_device,
            b_device_class: local_dev.device.b_device_class,
            b_device_sub_class: local_dev.device.b_device_sub_class,
            b_device_protocol: local_dev.device.b_device_protocol,
            b_configuration_value: local_dev.device.b_configuration_value,
            b_num_configurations: local_dev.device.b_num_configurations,
            b_num_interfaces: local_dev.device.b_num_interfaces,
        });
    }

    Ok(res)
}

struct RemoteConnectionInfo {
    remote_host: String,
    remote_port: u16,
    remote_bus_id: String,
}

fn read_record(rh_port: u16) -> Result<RemoteConnectionInfo, Error> {
    use std::fs;

    let port_path = Path::new(VHCI_STATE_PATH).join(format!("port{rh_port}"));

    let mut file = fs::OpenOptions::new()
        .read(true)
        .open(port_path)
        .map_err(|e| Error::FsIo(e, rh_port))?;

    let mut buf = String::new();
    file.read_to_string(&mut buf)
        .map_err(|e| Error::FsIo(e, rh_port))?;

    let (remote_host, remote_port, remote_bus_id) =
        sscanf::sscanf!(buf.trim(), "{str} {u16} {str}")
            .map_err(|e| Error::FsStateParsing(e, rh_port))?;

    Ok(RemoteConnectionInfo {
        remote_host: remote_host.into(),
        remote_port,
        remote_bus_id: remote_bus_id.into(),
    })
}

fn get_device_strings(local_bus_id: &str) -> Result<(String, String), Error> {
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

pub(crate) fn get_device_display_strings(
    #[cfg(feature = "runtime-hwdb")] hwdb: &udev::Hwdb,
    vendor_id: u16,
    product_id: u16,
) -> (Option<String>, Option<String>) {
    #[cfg(feature = "runtime-hwdb")]
    let (vendor, product) = {
        let results: Vec<_> = hwdb
            .query(format!("usb:v{vendor_id:04X}p{product_id:04X}*"))
            .collect();

        let mut vendor = results
            .iter()
            .find(|e| e.name().to_string_lossy() == "ID_VENDOR_FROM_DATABASE")
            .map(|e| e.value().to_string_lossy().to_string());
        let mut product = results
            .iter()
            .find(|e| e.name().to_string_lossy() == "ID_MODEL_FROM_DATABASE")
            .map(|e| e.value().to_string_lossy().to_string());

        (vendor, product)
    };

    #[cfg(feature = "baked-hwdb")]
    let (vendor, product) = {
        let mut vendor = None;
        let mut product = None;

        for v in usb_ids::Vendors::iter() {
            if v.id() == vendor_id {
                vendor = Some(v.name().to_string());

                for d in v.devices() {
                    if d.id() == product_id {
                        product = Some(d.name().to_string());
                        break;
                    }
                }

                break;
            }
        }

        (vendor, product)
    };

    (vendor, product)
}
