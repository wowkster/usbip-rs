use std::{io, str::Utf8Error};

use crate::{
    UsbDeviceInfo, UsbDeviceInfoValidationError,
    hwdb::{get_class_display_strings, get_device_display_strings},
    net::UsbIpSocket,
    proto::{ListDevicesReply, OperationError, OperationKind, RawUsbDeviceInfo, UsbInterfaceInfo},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Network connection failed ({0})")]
    NetworkIo(io::Error),

    #[error("usbip network operation failed ({0})")]
    Operation(#[from] OperationError),

    #[error("Failed to parse PDU: {0}")]
    ProtocolUsbDevice(#[from] UsbDeviceInfoValidationError),
    #[error("Failed to decode PDU strings as UTF-8")]
    Utf8(#[from] Utf8Error),

    #[error("Failed to initialize udev hwdb")]
    UdevHwdb(io::Error),
}

#[derive(Debug, serde::Serialize)]
pub struct ExportedDevice {
    pub host: String,
    pub port: u16,

    pub url: String,

    pub remote_device_info: UsbDeviceInfo,

    pub vendor: Option<String>,
    pub product: Option<String>,

    pub class: Option<String>,
    pub sub_class: Option<String>,
    pub protocol: Option<String>,

    pub interfaces: Vec<ExportedDeviceInterface>,
}

#[derive(Debug, serde::Serialize)]
pub struct ExportedDeviceInterface {
    pub b_interface_class: u8,
    pub b_interface_sub_class: u8,
    pub b_interface_protocol: u8,

    pub class: Option<String>,
    pub sub_class: Option<String>,
    pub protocol: Option<String>,
}

pub fn list_exported_devices(host: &str) -> Result<Vec<ExportedDevice>, Error> {
    #[cfg(feature = "runtime-hwdb")]
    let hwdb = udev::Hwdb::new().map_err(Error::UdevHwdb)?; // TODO: fallback to baked hwdb?
    let mut socket = UsbIpSocket::connect_host_and_port(host, UsbIpSocket::DEFAULT_PORT)
        .map_err(Error::NetworkIo)?;

    let op_kind = OperationKind::ListDevices;

    socket
        .send_request_header(op_kind)
        .map_err(Error::NetworkIo)?;
    socket
        .recv_reply_header(op_kind)
        .map_err(Error::NetworkIo)??;

    let reply = socket
        .recv_encoded::<ListDevicesReply>()
        .map_err(Error::NetworkIo)?;

    tracing::debug!("expecting {} devices", reply.num_devices);

    let mut results = Vec::new();

    if reply.num_devices == 0 {
        tracing::info!("no exported devices found");
        return Ok(results);
    }

    for _ in 0..reply.num_devices {
        let remote_device: UsbDeviceInfo = socket
            .recv_encoded::<RawUsbDeviceInfo>()
            .map_err(Error::NetworkIo)?
            .try_into()?;

        let (vendor, product) = get_device_display_strings(
            #[cfg(feature = "runtime-hwdb")]
            &hwdb,
            remote_device.id_vendor,
            remote_device.id_product,
        );

        let (class, sub_class, protocol) = get_class_display_strings(
            #[cfg(feature = "runtime-hwdb")]
            &hwdb,
            remote_device.b_device_class,
            remote_device.b_device_sub_class,
            remote_device.b_device_protocol,
        );

        let num_interfaces = remote_device.b_num_interfaces;

        let mut exported = ExportedDevice {
            host: host.to_string(),
            port: UsbIpSocket::DEFAULT_PORT, // TODO: update when we add dynamic port support
            url: format!(
                "usbip://{host}:{}/{}",
                UsbIpSocket::DEFAULT_PORT,
                remote_device.bus_id
            ),
            remote_device_info: remote_device,
            vendor,
            product,
            class,
            sub_class,
            protocol,
            interfaces: Vec::with_capacity(num_interfaces as _),
        };

        for _ in 0..num_interfaces {
            let iface = socket
                .recv_encoded::<UsbInterfaceInfo>()
                .map_err(Error::NetworkIo)?;

            let (class, sub_class, protocol) = get_class_display_strings(
                #[cfg(feature = "runtime-hwdb")]
                &hwdb,
                iface.b_interface_class,
                iface.b_interface_sub_class,
                iface.b_interface_protocol,
            );

            exported.interfaces.push(ExportedDeviceInterface {
                b_interface_class: iface.b_interface_class,
                b_interface_sub_class: iface.b_interface_sub_class,
                b_interface_protocol: iface.b_interface_protocol,
                class,
                sub_class,
                protocol,
            });
        }

        results.push(exported);
    }

    Ok(results)
}
