use std::{io, str::Utf8Error};

use crate::{
    client::port::get_device_display_strings,
    drivers::vhci_hcd::UsbSpeed,
    net::UsbIpSocket,
    proto::{ListDevicesReply, OperationError, OperationKind, UsbDeviceInfo, UsbInterfaceInfo},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Network connection failed ({0})")]
    NetworkIo(io::Error),

    #[error("usbip network operation failed ({0})")]
    Operation(#[from] OperationError),

    #[error("Failed to parse PDU content")]
    Procotol,
    #[error("Failed to decode PDU strings as UTF-8")]
    Utf8(#[from] Utf8Error),

    #[error("Failed to initialize udev hwdb")]
    UdevHwdb(io::Error),
}

// TODO: remove standard size prefixes?

#[derive(Debug, serde::Serialize)]
pub struct ExportedDevice {
    pub host: String,
    pub port: u16,

    pub url: String,

    pub sys_path: String,
    pub bus_id: String,

    pub bus_num: u16,
    pub dev_num: u16,

    pub vendor_display: Option<String>,
    pub product_display: Option<String>,

    pub speed: UsbSpeed,

    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,

    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,

    pub class_display: Option<String>,
    pub sub_class_display: Option<String>,
    pub protocol_display: Option<String>,

    pub b_configuration_value: u8,
    pub b_num_configurations: u8,
    pub b_num_interfaces: u8,

    pub interfaces: Vec<ExportedDeviceInterface>,
}

#[derive(Debug, serde::Serialize)]
pub struct ExportedDeviceInterface {
    pub b_interface_class: u8,
    pub b_interface_sub_class: u8,
    pub b_interface_protocol: u8,

    pub class_display: Option<String>,
    pub sub_class_display: Option<String>,
    pub protocol_display: Option<String>,
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
        let usb_device = socket
            .recv_encoded::<UsbDeviceInfo>()
            .map_err(Error::NetworkIo)?;

        let sys_path = usb_device
            .path
            .as_c_str()
            .ok_or_else(|| Error::Procotol)?
            .to_string_lossy()
            .to_string();
        let bus_id = usb_device
            .bus_id
            .as_c_str()
            .ok_or_else(|| Error::Procotol)?
            .to_str()?
            .to_string();

        let (manufacturer_display, product_display) = get_device_display_strings(
            #[cfg(feature = "runtime-hwdb")]
            &hwdb,
            usb_device.id_vendor,
            usb_device.id_product,
        );

        let (class_display, sub_class_display, protocol_display) = get_class_display_strings(
            #[cfg(feature = "runtime-hwdb")]
            &hwdb,
            usb_device.b_device_class,
            usb_device.b_device_sub_class,
            usb_device.b_device_protocol,
        );

        let mut exported = ExportedDevice {
            host: host.to_string(),
            port: UsbIpSocket::DEFAULT_PORT, // TODO: update when we add dynamic port support
            url: format!("usbip://{host}:{}/{bus_id}", UsbIpSocket::DEFAULT_PORT),
            sys_path,
            bus_id,
            bus_num: usb_device.bus_num as _,
            dev_num: usb_device.dev_num as _,
            vendor_display: manufacturer_display,
            product_display,
            speed: UsbSpeed::try_from(usb_device.speed).map_err(|_| Error::Procotol)?,
            id_vendor: usb_device.id_vendor,
            id_product: usb_device.id_product,
            bcd_device: usb_device.bcd_device,
            b_device_class: usb_device.b_device_class,
            b_device_sub_class: usb_device.b_device_sub_class,
            b_device_protocol: usb_device.b_device_protocol,
            class_display,
            sub_class_display,
            protocol_display,
            b_configuration_value: usb_device.b_configuration_value,
            b_num_configurations: usb_device.b_num_configurations,
            b_num_interfaces: usb_device.b_num_interfaces,
            interfaces: Vec::with_capacity(usb_device.b_num_interfaces as _),
        };

        for _ in 0..usb_device.b_num_interfaces {
            let iface = socket
                .recv_encoded::<UsbInterfaceInfo>()
                .map_err(Error::NetworkIo)?;

            let (class_display, sub_class_display, protocol_display) = get_class_display_strings(
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
                class_display,
                sub_class_display,
                protocol_display,
            });
        }

        results.push(exported);
    }

    Ok(results)
}

fn get_class_display_strings(
    #[cfg(feature = "runtime-hwdb")] hwdb: &udev::Hwdb,
    class: u8,
    sub_class: u8,
    protocol: u8,
) -> (Option<String>, Option<String>, Option<String>) {
    #[cfg(feature = "runtime-hwdb")]
    let (class, sub_class, protocol) = {
        // TODO: investigate using interface level queries first and then
        // falling back to device level if none are found

        // TODO: add an option to fall back to baked hwdb

        let results: Vec<_> = hwdb
            .query(format!(
                "usb:v*p*d*dc{class:02X}dsc{sub_class:02X}dp{protocol:02X}*"
            ))
            .collect();

        let mut class = results
            .iter()
            .find(|e| e.name().to_string_lossy() == "ID_USB_CLASS_FROM_DATABASE")
            .map(|e| e.value().to_string_lossy().to_string());
        let mut sub_class = results
            .iter()
            .find(|e| e.name().to_string_lossy() == "ID_USB_SUBCLASS_FROM_DATABASE")
            .map(|e| e.value().to_string_lossy().to_string());
        let mut protocol = results
            .iter()
            .find(|e| e.name().to_string_lossy() == "ID_USB_PROTOCOL_FROM_DATABASE")
            .map(|e| e.value().to_string_lossy().to_string());

        (class, sub_class, protocol)
    };

    #[cfg(feature = "baked-hwdb")]
    let (class, sub_class, protocol) = {
        let mut class_display = None;
        let mut sub_class_display = None;
        let mut protocol_display = None;

        for c in usb_ids::Classes::iter() {
            if c.id() == class {
                class_display = Some(c.name().to_string());

                for s in c.sub_classes() {
                    if s.id() == sub_class {
                        sub_class_display = Some(s.name().to_string());

                        for p in s.protocols() {
                            if p.id() == protocol {
                                protocol_display = Some(p.name().to_string());
                                break;
                            }
                        }

                        break;
                    }
                }

                break;
            }
        }

        (class_display, sub_class_display, protocol_display)
    };

    (class, sub_class, protocol)
}
