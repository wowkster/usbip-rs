use std::{
    ffi::{CStr, OsStr},
    os::unix::ffi::OsStrExt,
};

use endian_codec::{DecodeBE, EncodeBE, PackedSize};

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
    pub usb_device: UsbDeviceInfo,
}

pub const SYSFS_PATH_MAX: usize = 256;
pub const SYSFS_BUS_ID_SIZE: usize = 32;

#[derive(Debug, Clone, PackedSize, EncodeBE, DecodeBE)]
#[repr(C)]
pub struct UsbDeviceInfo {
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

/// Represents a potentially null terminated char buffer
#[derive(Clone)]
#[repr(C)]
pub struct CharBuf<const N: usize> {
    buffer: [u8; N],
}

impl<const N: usize> CharBuf<N> {
    pub fn new(value: &str) -> Option<Self> {
        Self::try_from(value).ok()
    }

    pub fn new_truncated(value: &str) -> Self {
        Self::try_from(&value[..value.len().min(N - 1)]).unwrap()
    }

    pub fn as_c_str(&self) -> Option<&CStr> {
        CStr::from_bytes_until_nul(&self.buffer).ok()
    }
}

impl<const N: usize> TryFrom<&str> for CharBuf<N> {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.len() >= N {
            return Err(());
        }

        let mut buffer = [0; _];
        buffer[..value.len()].copy_from_slice(&value.as_bytes());

        Ok(Self { buffer })
    }
}

impl<const N: usize> TryFrom<&OsStr> for CharBuf<N> {
    type Error = ();

    fn try_from(value: &OsStr) -> Result<Self, Self::Error> {
        if value.len() >= N {
            return Err(());
        }

        let mut buffer = [0; _];
        buffer[..value.len()].copy_from_slice(&value.as_bytes());

        Ok(Self { buffer })
    }
}

impl<const N: usize> PackedSize for CharBuf<N> {
    const PACKED_LEN: usize = core::mem::size_of::<Self>();
}

impl<const N: usize> EncodeBE for CharBuf<N> {
    fn encode_as_be_bytes(&self, bytes: &mut [u8]) {
        bytes.copy_from_slice(&self.buffer);
    }
}

impl<const N: usize> DecodeBE for CharBuf<N> {
    fn decode_from_be_bytes(bytes: &[u8]) -> Self {
        // TODO: could we omit the buffer initialization?

        let mut buffer = [0; _];
        buffer.copy_from_slice(bytes);

        Self { buffer }
    }
}

impl<const N: usize> core::fmt::Debug for CharBuf<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct(&format!("CharBuf<{N}>"));

        if let Some(c_str) = self.as_c_str() {
            s.field("buffer", &c_str.to_string_lossy())
        } else {
            s.field("buffer", &self.buffer)
        }
        .finish()
    }
}

#[derive(Debug, Clone, PackedSize, EncodeBE, DecodeBE)]
#[repr(C)]
pub struct ListDevicesReply {
    pub num_devices: u32,
}
