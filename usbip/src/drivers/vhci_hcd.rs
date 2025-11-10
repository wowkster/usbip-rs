use std::{fs, io, os::fd::RawFd, str::FromStr};

use compact_str::CompactString;

use crate::proto::{CharBuf, UsbDeviceInfo};

#[derive(Debug, thiserror::Error)]
pub enum VhciHcdError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Failed to get udev attribute value")]
    FailedToGetUdevAttribute,
    #[error("Failed to parse udev attribute value")]
    FailedToParseUdevAttribute,
    #[error("No available ports")]
    NoAvailablePorts,
    #[error("Failed to get udev device parent")]
    FailedToGetUdevParent,
    #[error("No available controllers")]
    NoAvailableContollers,
    #[error("Parsed data from udev attribute was invalid")]
    InvalidUdevAttributeData,
    #[error("No free ports available (all in use)")]
    NoFreePorts,
}

/// USB/IP 'Virtual' Host Controller (VHCI) Driver
#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct VhciHcd {
    #[derivative(Debug = "ignore")]
    context: udev::Udev,
    device: udev::Device,

    num_ports: u32,
    num_controllers: u32,

    /// List of imported device slots allocated by the kernel (len = num_ports)
    imported_devices: Vec<VhciImportedDevice>,
}

#[derive(Debug, Clone, Default)]
pub struct VhciImportedDevice {
    pub hub_speed: HubSpeed,
    pub port: u16,
    pub state: VhciDeviceState,
}

impl VhciImportedDevice {
    fn remote_device_id(&self) -> u32 {
        match &self.state {
            VhciDeviceState::NotConnected | VhciDeviceState::NotAssigned => 0,
            VhciDeviceState::Used(d) | VhciDeviceState::Error(d) => d.remote_device_id,
        }
    }

    pub fn remote_bus_num(&self) -> u16 {
        (self.remote_device_id() >> 16) as u16
    }

    pub fn remote_dev_num(&self) -> u16 {
        (self.remote_device_id() & 0xFFFF) as u16
    }

    pub fn status(&self) -> VhciDeviceStatus {
        match self.state {
            VhciDeviceState::NotConnected => VhciDeviceStatus::NotConnected,
            VhciDeviceState::NotAssigned => VhciDeviceStatus::NotAssigned,
            VhciDeviceState::Used(_) => VhciDeviceStatus::Used,
            VhciDeviceState::Error(_) => VhciDeviceStatus::Error,
        }
    }

    pub fn connected_device(&self) -> Option<&VhciConnectedDevice> {
        match &self.state {
            VhciDeviceState::NotConnected | VhciDeviceState::NotAssigned => None,
            VhciDeviceState::Used(d) | VhciDeviceState::Error(d) => Some(d),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::TryFromPrimitive, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u32)]
pub enum VhciDeviceStatus {
    /// VDEV_ST_NULL
    ///
    /// vdev does not connect a remote device
    NotConnected = 4,
    /// VDEV_ST_NOTASSIGNED
    ///
    /// vdev is used, but the USB address is not assigned yet
    NotAssigned,
    /// VDEV_ST_USED
    Used,
    /// VDEV_ST_ERROR
    Error,
}

#[derive(Debug, Clone, Default)]
pub enum VhciDeviceState {
    #[default]
    NotConnected,
    NotAssigned,
    Used(VhciConnectedDevice),
    Error(VhciConnectedDevice),
}

#[derive(Debug, Clone)]
pub struct VhciConnectedDevice {
    /// Encodes the bus_num and dev_num of the device on the remote machine
    pub remote_device_id: u32,
    /// The socket fd passed to vhci_hcd during device attachment
    pub socket_fd: u32,
    /// The info gathered from udev about the locally mounted device (created by
    /// vhci_hcd)
    pub device: UsbDeviceInfo, // TODO: use a parsed and validated type (for speed and other parameters)
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    num_enum::TryFromPrimitive,
    serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
#[repr(u32)]
pub enum HubSpeed {
    #[default]
    High = 0,
    Super,
}

const USBIP_VHCI_BUS_TYPE: &str = "platform";
const USBIP_VHCI_DEVICE_NAME: &str = "vhci_hcd.0";

impl VhciHcd {
    pub fn open() -> Result<Self, VhciHcdError> {
        let context = udev::Udev::new()?;

        let device = udev::Device::from_subsystem_sysname_with_context(
            context.clone(),
            USBIP_VHCI_BUS_TYPE.into(),
            USBIP_VHCI_DEVICE_NAME.into(),
        )?;

        let num_ports = device
            .attribute_value("nports")
            .ok_or(VhciHcdError::FailedToGetUdevAttribute)?
            .to_str()
            .ok_or(VhciHcdError::FailedToParseUdevAttribute)?
            .parse::<u32>()
            .map_err(|_| VhciHcdError::FailedToParseUdevAttribute)?;

        if num_ports == 0 {
            return Err(VhciHcdError::NoAvailablePorts);
        }

        tracing::debug!("available ports = {num_ports}");

        let platform = device.parent().ok_or(VhciHcdError::FailedToGetUdevParent)?;
        let sys_path = platform.syspath();

        let mut num_controllers = 0;
        for entry in fs::read_dir(sys_path)? {
            if entry?
                .file_name()
                .to_string_lossy()
                .starts_with("vhci_hcd.")
            {
                num_controllers += 1;
            }
        }

        tracing::debug!("available controllers = {num_controllers}");

        if num_controllers == 0 {
            return Err(VhciHcdError::NoAvailableContollers);
        }

        let mut this = Self {
            context,
            device,
            num_ports,
            num_controllers,
            imported_devices: vec![Default::default(); num_ports as usize],
        };

        this.refresh_improted_device_list()?;

        Ok(this)
    }

    pub fn refresh_improted_device_list(&mut self) -> Result<(), VhciHcdError> {
        for i in 0..self.num_controllers {
            let attr_name = if i == 0 {
                "status"
            } else {
                &format!("status.{i}")
            };

            tracing::debug!("controller {i}");

            let status_attr = self
                .device
                .attribute_value(attr_name)
                .ok_or(VhciHcdError::FailedToGetUdevAttribute)?
                .to_str()
                .ok_or(VhciHcdError::FailedToParseUdevAttribute)?
                .to_owned();

            let mut total_devices = 0;

            for (j, r) in parse_vhci_hcd_status_attr(&status_attr).enumerate() {
                if total_devices == self.num_ports {
                    return Err(VhciHcdError::InvalidUdevAttributeData);
                }

                total_devices += 1;

                let status_line = r.map_err(|_| VhciHcdError::FailedToParseUdevAttribute)?;

                let speed = match status_line.hub.as_str() {
                    "hs" => HubSpeed::High,
                    "ss" => HubSpeed::Super,
                    _ => return Err(VhciHcdError::FailedToParseUdevAttribute),
                };

                if status_line.port >= self.num_ports as _ {
                    return Err(VhciHcdError::InvalidUdevAttributeData);
                }

                let status = VhciDeviceStatus::try_from(status_line.status)
                    .map_err(|_| VhciHcdError::FailedToParseUdevAttribute)?;

                let state = match status {
                    VhciDeviceStatus::NotConnected => VhciDeviceState::NotConnected,
                    VhciDeviceStatus::NotAssigned => VhciDeviceState::NotAssigned,
                    s @ (VhciDeviceStatus::Used | VhciDeviceStatus::Error) => {
                        let device = self.query_imported_device(&status_line.local_bus_id)?;

                        let connected_device = VhciConnectedDevice {
                            remote_device_id: status_line.device_id,
                            socket_fd: status_line.socket_fd,
                            device,
                        };

                        if s == VhciDeviceStatus::Used {
                            VhciDeviceState::Used(connected_device)
                        } else {
                            VhciDeviceState::Error(connected_device)
                        }
                    }
                };

                self.imported_devices[j as usize] = VhciImportedDevice {
                    hub_speed: speed,
                    port: status_line.port,
                    state,
                };
            }

            if total_devices < self.num_ports {
                return Err(VhciHcdError::InvalidUdevAttributeData);
            }
        }

        Ok(())
    }

    fn query_imported_device(&mut self, local_bus_id: &str) -> Result<UsbDeviceInfo, VhciHcdError> {
        let udev = udev::Device::from_subsystem_sysname_with_context(
            self.context.clone(),
            "usb".into(),
            local_bus_id.into(),
        )?;

        macro_rules! extract_attr {
            ($name:ident) => {
                udev.attribute_value(stringify!($name))
                    .ok_or(VhciHcdError::FailedToGetUdevAttribute)?
                    .to_string_lossy()
                    .trim()
            };
        }

        macro_rules! parse_attr {
            ($ty:ty, $name:ident) => {
                <$ty>::from_str(extract_attr!($name))
                    .map_err(|_| VhciHcdError::FailedToParseUdevAttribute)?
            };
        }

        macro_rules! parse_attr_hex {
            ($ty:ty, $name:ident) => {
                <$ty>::from_str_radix(extract_attr!($name), 16)
                    .map_err(|_| VhciHcdError::FailedToParseUdevAttribute)?
            };
        }

        let path = udev.syspath().to_string_lossy();
        let bus_id = udev.sysname().to_string_lossy();

        Ok(UsbDeviceInfo {
            path: CharBuf::new_truncated(&path),
            bus_id: CharBuf::new_truncated(&bus_id),
            bus_num: parse_attr_hex!(u32, busnum),
            dev_num: parse_attr_hex!(u32, devnum),
            speed: parse_attr!(UsbSpeed, speed) as _, // TODO: this is super ew
            id_vendor: parse_attr_hex!(u16, idVendor),
            id_product: parse_attr_hex!(u16, idProduct),
            bcd_device: parse_attr_hex!(u16, bcdDevice),
            b_device_class: parse_attr_hex!(u8, bDeviceClass),
            b_device_sub_class: parse_attr_hex!(u8, bDeviceSubClass),
            b_device_protocol: parse_attr_hex!(u8, bDeviceProtocol),
            b_configuration_value: parse_attr_hex!(u8, bConfigurationValue), // TODO: special handling
            b_num_configurations: parse_attr_hex!(u8, bNumConfigurations),
            b_num_interfaces: parse_attr_hex!(u8, bNumInterfaces), // TODO: special handling
        })
    }

    pub fn get_free_port(&mut self, speed: UsbSpeed) -> Result<u32, VhciHcdError> {
        for i in 0..self.num_ports {
            let device = &self.imported_devices[i as usize];

            match speed {
                UsbSpeed::Super => {
                    if device.hub_speed != HubSpeed::Super {
                        continue;
                    }
                }
                _ => {
                    if device.hub_speed != HubSpeed::High {
                        continue;
                    }
                }
            }

            if device.status() == VhciDeviceStatus::NotConnected {
                return Ok(i);
            }
        }

        Err(VhciHcdError::NoFreePorts)
    }

    pub fn attach_device(
        &mut self,
        rh_port: u32,
        socket_fd: RawFd,
        bus_num: u32,
        dev_num: u32,
        speed: u32,
    ) -> Result<(), VhciHcdError> {
        use std::{fs, io::Write};

        let device_id = (bus_num << 16) | dev_num;
        let buf = format!("{rh_port} {socket_fd} {device_id} {speed}");
        let attach_path = self.device.syspath().join("attach");

        let mut file = fs::OpenOptions::new().write(true).open(attach_path)?;
        file.write_all(buf.as_bytes())?;

        Ok(())
    }

    pub fn detach_device(&mut self, port: u16) -> Result<(), VhciHcdError> {
        use std::{fs, io::Write};

        let buf = format!("{port}");
        let detach_path = self.device.syspath().join("detach");

        let mut file = fs::OpenOptions::new().write(true).open(detach_path)?;
        file.write_all(buf.as_bytes())?;

        Ok(())
    }

    pub fn controller_count(&self) -> u16 {
        self.num_controllers as _
    }

    pub fn total_port_count(&self) -> u16 {
        // TODO: check the kernel module to see if we are off by 2x
        self.num_ports as _
    }

    pub fn ports_per_controller(&self) -> u16 {
        // TODO: check the kernel module to see if we are off by 2x
        self.total_port_count() / self.controller_count()
    }

    pub fn cached_imported_devices(&self) -> &[VhciImportedDevice] {
        &self.imported_devices
    }
}

#[derive(Debug)]
struct VhciHcdStatusLine {
    hub: CompactString,
    port: u16,
    status: u32,
    speed: u8,
    device_id: u32,
    socket_fd: u32,
    local_bus_id: CompactString,
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to parse vhci_hcd controller status")]
struct VhciHcdStatusParseError;

impl FromStr for VhciHcdStatusLine {
    type Err = VhciHcdStatusParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (hub, port, status, speed, device_id, socket_fd, local_bus_id) =
            sscanf::sscanf!(s, "{str}  {u16} {u32} {u8} {u32:x} {u32} {str}",)
                .map_err(|_| VhciHcdStatusParseError)?;

        Ok(Self {
            hub: hub.into(),
            port,
            status,
            speed,
            device_id,
            socket_fd,
            local_bus_id: local_bus_id.into(),
        })
    }
}

/// Parses the output of /sys/devices/platform/vhci_hcd.0/status line by line
fn parse_vhci_hcd_status_attr(
    text: &str,
) -> impl Iterator<Item = Result<VhciHcdStatusLine, VhciHcdStatusParseError>> {
    text.lines().skip(1).map(|l| l.parse())
}

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
