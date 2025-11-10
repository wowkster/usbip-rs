use endian_codec::{DecodeBE, EncodeBE, PackedSize};

use crate::proto::char_buf::CharBuf;

pub mod char_buf;

pub const USBIP_VERSION: u16 = 0x0111;

// implicitly packed due to layout, so we can avoid using `#[repr(packed)]`
#[derive(Debug, Clone, PackedSize, EncodeBE, DecodeBE)]
#[repr(C)]
pub struct OperationHeader {
    pub version: u16,
    pub code: u16,
    pub status: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum Direction {
    Request = 0x8000,
    Reply = 0x0000,
}

impl Direction {
    pub fn from_code(code: u16) -> Self {
        match code & 0x8000 {
            0 => Self::Reply,
            _ => Self::Request,
        }
    }
}

/// Core operations provided by the user-space server before the socket switched
/// into kernel space
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum OperationKind {
    /// Dummy Code
    Unspecified = 0x00,
    /// Retrieve USB device information. (still not used)
    ///
    /// NOT IMPLEMENTED IN ORIGINAL
    ///
    /// TODO: implement this :)
    DeviceInfo = 0x02,
    /// Import a remote USB device.
    Import = 0x03,
    /// Export a USB device to a remote host.
    ///
    /// NOT IMPLEMENTED IN ORIGINAL
    ///
    /// TODO: implement this :)
    Export = 0x06,
    /// un-Export a USB device from a remote host.
    ///
    /// NOT IMPLEMENTED IN ORIGINAL
    ///
    /// TODO: implement this :)
    UnExport = 0x07,
    /// Negotiate IPSec encryption key. (still not used)
    ///
    /// NOT IMPLEMENTED IN ORIGINAL
    ///
    /// TODO: can this be implemented without modifying the kernel modules?
    EncryptionKey = 0x04,
    /// Retrieve the list of exported USB devices.
    ListDevices = 0x05,
}

impl OperationKind {
    pub fn from_code(code: u16) -> Option<Self> {
        Some(match code & 0x7FFF {
            0x00 => Self::Unspecified,
            0x02 => Self::DeviceInfo,
            0x03 => Self::Import,
            0x06 => Self::Export,
            0x07 => Self::UnExport,
            0x04 => Self::EncryptionKey,
            0x05 => Self::ListDevices,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OperationStatus {
    /// Request completed successfully
    Ok = 0x00,
    /// Request failed
    Failure = 0x01,
    /// Device requested for import is not available (already exported)
    DeviceBusy = 0x02,
    /// Device requested for import is in error state
    DeviceError = 0x03,
    /// Device requested does not exist on the host
    NoSuchDevice = 0x04,
    /// Some other opaque error
    Error = 0x05,
}

impl OperationStatus {
    pub fn from_raw(value: u32) -> Option<Self> {
        Some(match value {
            0x00 => Self::Ok,
            0x01 => Self::Failure,
            0x02 => Self::DeviceBusy,
            0x03 => Self::DeviceError,
            0x04 => Self::NoSuchDevice,
            0x05 => Self::Error,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, thiserror::Error)]
pub enum OperationError {
    #[error("request failed")]
    RequestFailed,
    #[error("device is already exported")]
    DeviceBusy,
    #[error("device is in error state")]
    DeviceError,
    #[error("device does not exist on the server")]
    NoSuchDevice,
    #[error("version in header did not match expected")]
    VersionMismatch,
    #[error("direction in header did not match expected")]
    DirectionMismatch,
    #[error("received PDU with invalid data")]
    InvalidData,
    #[error("some other error ocrrured")]
    Other,
}

#[derive(Debug, Clone, PackedSize, EncodeBE, DecodeBE)]
#[repr(C)]
pub struct ImportRequest {
    pub bus_id: CharBuf<SYSFS_BUS_ID_SIZE>,
}

#[derive(Debug, Clone, PackedSize, EncodeBE, DecodeBE)]
#[repr(C)]
pub struct ImportReply {
    pub usb_device: RawUsbDeviceInfo,
}

pub const SYSFS_PATH_MAX: usize = 256;
pub const SYSFS_BUS_ID_SIZE: usize = 32;

#[derive(Debug, Clone, PackedSize, EncodeBE, DecodeBE)]
#[repr(C)]
pub struct RawUsbDeviceInfo {
    pub path: CharBuf<SYSFS_PATH_MAX>,
    pub bus_id: CharBuf<SYSFS_BUS_ID_SIZE>,

    pub bus_num: u32,
    pub dev_num: u32,
    pub speed: u32,

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

#[derive(Debug, Clone, PackedSize, EncodeBE, DecodeBE)]
#[repr(C)]
pub struct UsbInterfaceInfo {
    pub b_interface_class: u8,
    pub b_interface_sub_class: u8,
    pub b_interface_protocol: u8,
    _padding: u8,
}

#[derive(Debug, Clone, PackedSize, EncodeBE, DecodeBE)]
#[repr(C)]
pub struct ListDevicesReply {
    pub num_devices: u32,
}
