use core::str::FromStr;

use crate::{UsbDeviceInfo, UsbSpeed};

#[derive(Debug, thiserror::Error)]
pub enum UsbInfoExtractError {
    #[error("Failed to get value for udev attribute `{0}`")]
    AttributeMissing(String),
    #[error("Failed to decode value of udev attribute `{0}` as UTF-8")]
    AttributeNotUtf8(String),
    #[error("Failed to parse value for udev attribute `{0}`")]
    AttributeParsingFailed(String),
}

pub fn extract_usb_info_from_udev_device(
    udev: &udev::Device,
) -> Result<UsbDeviceInfo, UsbInfoExtractError> {
    macro_rules! extract_attr {
        ($name:ident) => {
            udev.attribute_value(stringify!($name))
                .ok_or_else(|| UsbInfoExtractError::AttributeMissing(stringify!($name).into()))?
                .to_str()
                .ok_or_else(|| UsbInfoExtractError::AttributeNotUtf8(stringify!($name).into()))?
                .trim()
        };
    }

    macro_rules! parse_attr {
        ($ty:ty, $name:ident) => {
            <$ty>::from_str(extract_attr!($name)).map_err(|_| {
                UsbInfoExtractError::AttributeParsingFailed(stringify!($name).into())
            })?
        };
    }

    macro_rules! parse_attr_hex {
        ($ty:ty, $name:ident) => {
            <$ty>::from_str_radix(extract_attr!($name), 16).map_err(|_| {
                UsbInfoExtractError::AttributeParsingFailed(stringify!($name).into())
            })?
        };
    }

    // Some values need special handling since they might not be set in all
    // cases and so parsing them may fail
    macro_rules! try_parse_attr_hex {
        ($ty:ty, $name:ident) => {
            <$ty>::from_str_radix(extract_attr!($name), 16).unwrap_or_default()
        };
    }

    let sys_path = udev
        .syspath()
        .to_str()
        .ok_or_else(|| UsbInfoExtractError::AttributeNotUtf8("syspath".into()))?;
    let bus_id = udev
        .sysname()
        .to_str()
        .ok_or_else(|| UsbInfoExtractError::AttributeNotUtf8("sysname".into()))?;

    Ok(UsbDeviceInfo {
        sys_path: sys_path.into(),
        bus_id: bus_id.into(),
        bus_num: parse_attr_hex!(u32, busnum),
        dev_num: parse_attr_hex!(u32, devnum),
        speed: parse_attr!(UsbSpeed, speed),
        id_vendor: parse_attr_hex!(u16, idVendor),
        id_product: parse_attr_hex!(u16, idProduct),
        bcd_device: parse_attr_hex!(u16, bcdDevice),
        b_device_class: parse_attr_hex!(u8, bDeviceClass),
        b_device_sub_class: parse_attr_hex!(u8, bDeviceSubClass),
        b_device_protocol: parse_attr_hex!(u8, bDeviceProtocol),
        b_configuration_value: try_parse_attr_hex!(u8, bConfigurationValue),
        b_num_configurations: parse_attr_hex!(u8, bNumConfigurations),
        b_num_interfaces: try_parse_attr_hex!(u8, bNumInterfaces),
    })
}
