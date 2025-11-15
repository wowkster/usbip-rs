#![forbid(unsafe_code)]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use compact_str::{CompactString, ToCompactString};

use crate::proto::RawUsbDeviceInfo;

pub mod client;
pub mod drivers;
mod hwdb;
pub mod net;
pub mod proto;
pub mod server;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    strum::EnumString,
    num_enum::TryFromPrimitive,
    serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
#[repr(u32)]
pub enum UsbSpeed {
    /// Enumerating
    #[strum(serialize = "unknown")]
    Unknown,
    /// USB 1.1
    #[strum(serialize = "1.5")]
    Low,
    /// USB 1.1
    #[strum(serialize = "12")]
    Full,
    /// USB 2.0
    #[strum(serialize = "480")]
    High,
    /// Wireless (USB 2.5)
    #[strum(serialize = "53.3-480")]
    Wireless,
    /// USB 3.0
    #[strum(serialize = "5000")]
    Super,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UsbDeviceInfo {
    pub sys_path: String,
    pub bus_id: CompactString,

    pub bus_num: u32,
    pub dev_num: u32,
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

#[derive(Debug, thiserror::Error)]
#[error("Failed to validate raw USB device info object")]
pub struct UsbDeviceInfoValidationError;

impl TryFrom<RawUsbDeviceInfo> for UsbDeviceInfo {
    type Error = UsbDeviceInfoValidationError;

    fn try_from(value: RawUsbDeviceInfo) -> Result<Self, Self::Error> {
        let sys_path = value
            .path
            .as_c_str()
            .ok_or(UsbDeviceInfoValidationError)?
            .to_str()
            .map_err(|_| UsbDeviceInfoValidationError)?
            .to_string();
        let bus_id = value
            .bus_id
            .as_c_str()
            .ok_or(UsbDeviceInfoValidationError)?
            .to_str()
            .map_err(|_| UsbDeviceInfoValidationError)?
            .to_compact_string();

        let speed = UsbSpeed::try_from(value.speed).map_err(|_| UsbDeviceInfoValidationError)?;

        Ok(Self {
            sys_path,
            bus_id,
            bus_num: value.bus_num,
            dev_num: value.dev_num,
            speed,
            id_vendor: value.id_vendor,
            id_product: value.id_product,
            bcd_device: value.bcd_device,
            b_device_class: value.b_device_class,
            b_device_sub_class: value.b_device_sub_class,
            b_device_protocol: value.b_device_protocol,
            b_configuration_value: value.b_configuration_value,
            b_num_configurations: value.b_num_configurations,
            b_num_interfaces: value.b_num_interfaces,
        })
    }
}
