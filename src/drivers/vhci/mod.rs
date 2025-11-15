use std::{
    fs,
    io::{self, ErrorKind},
    os::fd::RawFd,
    str::FromStr,
};

use compact_str::CompactString;

use crate::{UsbDeviceInfo, UsbSpeed};

pub mod state;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to create udev context ({0})")]
    CreatingUdevContext(io::Error),
    #[error("Device `vhci_hcd.0` not found. Is is the kernel module `vhci_hcd` loaded?")]
    VhciDeviceNotFound,
    #[error("Failed to open device `vhci_hcd.0` with udev ({0})")]
    VhciDeviceUdev(io::Error),
    #[error("Could not access parent device `platform` of `vhci_hcd.0`")]
    VhciDeviceParentNotFound,

    #[error("Failed to get value for udev attribute `{0}` from `vhci_hcd` device")]
    VhciDeviceMissingUdevAttribute(String),
    #[error("Failed to decode value of udev attribute `{0}` of `vhci_hcd` device as UTF-8")]
    VhciDeviceUtf8UdevAttribute(String),
    #[error("Failed to parse value of udev attribute `{0}` of `vhci_hcd` device")]
    VhciDeviceParsingUdevAttribute(String),

    #[error(
        "An I/O error occurred while communicating with the `vhci_hcd` device through sysfs ({0})"
    )]
    SysfsIo(io::Error),
    #[error(
        "Cannot write to `vhci_hcd` device due to a lack of permissions{}",
        format_permissions_help()
    )]
    SysfsPermissionDenied,
    #[error(
        "No ports available on `vhci_hcd` root hub(s). How the did you even manage to screw this up?"
    )]
    VhciNoAvailablePorts,
    #[error(
        "An I/O error occurred while attempting to enumerate available `vhci_hcd` contollers ({0})"
    )]
    EnumeratingControllers(io::Error),
    #[error(
        "Data parsed from `vhci_hcd` device status attributes did not match up with previously acquired device information"
    )]
    ConflictingStatusData,
    #[error("No free ports available matching requried speed (all in use)")]
    NoFreePorts,

    #[error(
        "An I/O error occurred while querying imported USB device with bus ID `{bus_id}` ({error})"
    )]
    QueryingLocalUsbDevice { bus_id: String, error: io::Error },
    #[error(
        "Failed to get value for udev attribute `{attribute}` from USB device with bus ID `{bus_id}`"
    )]
    UsbDeviceMissingUdevAttribute { bus_id: String, attribute: String },
    #[error(
        "Failed to decode value of udev attribute `{attribute}` of USB device with bus ID `{bus_id}` as UTF-8"
    )]
    UsbDeviceUtf8UdevAttribute { bus_id: String, attribute: String },
    #[error(
        "Failed to parse value for udev attribute `{attribute}` of USB device with bus ID `{bus_id}`"
    )]
    UsbDeviceParsingUdevAttribute { bus_id: String, attribute: String },
}

// TODO: factor this out for common sysfs access errors later
fn format_permissions_help() -> String {
    if !nix::unistd::geteuid().is_root() {
        " (not running as root). Try executing again with sudo.".into()
    } else {
        " (already running as root. how did we get ourselves here?)".into()
    }
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

    /// List of root hub ports allocated by the kernel (len = num_ports)
    virtual_devices: Vec<VhciDevice>,
}

#[derive(Debug, Clone, Default)]
pub struct VhciDevice {
    pub hub_speed: HubSpeed,
    pub port: u16,
    pub state: VhciDeviceState,
}

impl VhciDevice {
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

    pub fn connected_device(&self) -> Option<&VhciImportedDevice> {
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
    Used(VhciImportedDevice),
    Error(VhciImportedDevice),
}

#[derive(Debug, Clone)]
pub struct VhciImportedDevice {
    /// Encodes the bus_num and dev_num of the device on the remote machine
    pub remote_device_id: u32,
    /// The socket fd passed to vhci_hcd during device attachment
    pub socket_fd: u32,
    /// The info gathered from udev about the locally mounted device (created by
    /// vhci_hcd)
    pub device: UsbDeviceInfo,
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

impl VhciHcd {
    pub fn open() -> Result<Self, Error> {
        let context = udev::Udev::new().map_err(Error::CreatingUdevContext)?;

        let device = udev::Device::from_subsystem_sysname_with_context(
            context.clone(),
            "platform".into(),
            "vhci_hcd.0".into(),
        )
        .map_err(|e| {
            // udev returns ENODEV if the sysfs device was not there
            if e.raw_os_error()
                .is_some_and(|c| nix::errno::Errno::from_raw(c) == nix::errno::Errno::ENODEV)
            {
                Error::VhciDeviceNotFound
            } else {
                Error::VhciDeviceUdev(e.into())
            }
        })?;

        const NUM_PORTS_ATTR: &str = "nports";

        let num_ports = device
            .attribute_value(NUM_PORTS_ATTR)
            .ok_or_else(|| Error::VhciDeviceMissingUdevAttribute(NUM_PORTS_ATTR.into()))?
            .to_str()
            .ok_or_else(|| Error::VhciDeviceUtf8UdevAttribute(NUM_PORTS_ATTR.into()))?
            .parse::<u32>()
            .map_err(|_| Error::VhciDeviceParsingUdevAttribute(NUM_PORTS_ATTR.into()))?;

        if num_ports == 0 {
            return Err(Error::VhciNoAvailablePorts);
        }

        tracing::debug!("available ports = {num_ports}");

        let platform = device
            .parent()
            .ok_or_else(|| Error::VhciDeviceParentNotFound)?;
        let sys_path = platform.syspath();

        let mut num_controllers = 0;
        for entry in fs::read_dir(sys_path).map_err(Error::EnumeratingControllers)? {
            if entry
                .map_err(Error::EnumeratingControllers)?
                .file_name()
                .to_string_lossy()
                .starts_with("vhci_hcd.")
            {
                num_controllers += 1;
            }
        }

        tracing::debug!("available controllers = {num_controllers}");

        assert_ne!(
            num_controllers, 0,
            "should always have more than one controller if we opened the device initially"
        );

        let mut this = Self {
            context,
            device,
            num_ports,
            num_controllers,
            virtual_devices: vec![Default::default(); num_ports as usize],
        };

        this.refresh_improted_device_list()?;

        Ok(this)
    }

    /// Reads and parses the `status` or `status.X` attributes of the vhci_hcd
    /// device to get a list of imported devices from each controller. After
    /// collecting the results, uses udev to query more information from the USB
    /// devices to update the internal cache.
    pub fn refresh_improted_device_list(&mut self) -> Result<(), Error> {
        // we expect the total number of lines returned to match the `nports`
        // value we read during initialization. since the total number of
        // controllers and ports is baked into the kernel module at compile
        // time, this constraint should never be violated unless the module was
        // reloaded in between our driver's initialization and the calling of
        // this function. if that is the case, we have bigger problems anyway so
        // we report a conflict.

        let mut total_devices = 0;

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
                .ok_or_else(|| Error::VhciDeviceMissingUdevAttribute(attr_name.into()))?
                .to_str()
                .ok_or_else(|| Error::VhciDeviceUtf8UdevAttribute(attr_name.into()))?
                .to_owned();

            for (j, r) in parse_vhci_hcd_status_attr(&status_attr).enumerate() {
                if total_devices >= self.num_ports {
                    return Err(Error::ConflictingStatusData);
                }

                total_devices += 1;

                let status_line =
                    r.map_err(|_| Error::VhciDeviceParsingUdevAttribute(attr_name.into()))?;

                let speed = match status_line.hub.as_str() {
                    "hs" => HubSpeed::High,
                    "ss" => HubSpeed::Super,
                    _ => return Err(Error::VhciDeviceParsingUdevAttribute(attr_name.into())),
                };

                if status_line.port >= self.num_ports as _ {
                    return Err(Error::ConflictingStatusData);
                }

                let status = VhciDeviceStatus::try_from(status_line.status)
                    .map_err(|_| Error::VhciDeviceParsingUdevAttribute(attr_name.into()))?;

                let state = match status {
                    VhciDeviceStatus::NotConnected => VhciDeviceState::NotConnected,
                    VhciDeviceStatus::NotAssigned => VhciDeviceState::NotAssigned,
                    s @ (VhciDeviceStatus::Used | VhciDeviceStatus::Error) => {
                        let device = self.query_imported_device(&status_line.local_bus_id)?;

                        let connected_device = VhciImportedDevice {
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

                self.virtual_devices[j as usize] = VhciDevice {
                    hub_speed: speed,
                    port: status_line.port,
                    state,
                };
            }
        }

        if total_devices < self.num_ports {
            return Err(Error::ConflictingStatusData);
        }

        Ok(())
    }

    fn query_imported_device(&mut self, local_bus_id: &str) -> Result<UsbDeviceInfo, Error> {
        let udev = udev::Device::from_subsystem_sysname_with_context(
            self.context.clone(),
            "usb".into(),
            local_bus_id.into(),
        )
        .map_err(|error| Error::QueryingLocalUsbDevice {
            bus_id: local_bus_id.into(),
            error,
        })?;

        macro_rules! extract_attr {
            ($name:ident) => {
                udev.attribute_value(stringify!($name))
                    .ok_or_else(|| Error::UsbDeviceMissingUdevAttribute {
                        bus_id: local_bus_id.into(),
                        attribute: stringify!($name).into(),
                    })?
                    .to_str()
                    .ok_or_else(|| Error::UsbDeviceUtf8UdevAttribute {
                        bus_id: local_bus_id.into(),
                        attribute: stringify!($name).into(),
                    })?
                    .trim()
            };
        }

        macro_rules! parse_attr {
            ($ty:ty, $name:ident) => {
                <$ty>::from_str(extract_attr!($name)).map_err(|_| {
                    Error::UsbDeviceParsingUdevAttribute {
                        bus_id: local_bus_id.into(),
                        attribute: stringify!($name).into(),
                    }
                })?
            };
        }

        macro_rules! parse_attr_hex {
            ($ty:ty, $name:ident) => {
                <$ty>::from_str_radix(extract_attr!($name), 16).map_err(|_| {
                    Error::UsbDeviceParsingUdevAttribute {
                        bus_id: local_bus_id.into(),
                        attribute: stringify!($name).into(),
                    }
                })?
            };
        }

        let sys_path =
            udev.syspath()
                .to_str()
                .ok_or_else(|| Error::UsbDeviceUtf8UdevAttribute {
                    bus_id: local_bus_id.into(),
                    attribute: "syspath".into(),
                })?;
        let bus_id = udev
            .sysname()
            .to_str()
            .ok_or_else(|| Error::UsbDeviceUtf8UdevAttribute {
                bus_id: local_bus_id.into(),
                attribute: "sysname".into(),
            })?;

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
            b_configuration_value: parse_attr_hex!(u8, bConfigurationValue), // TODO: special handling
            b_num_configurations: parse_attr_hex!(u8, bNumConfigurations),
            b_num_interfaces: parse_attr_hex!(u8, bNumInterfaces), // TODO: special handling
        })
    }

    pub fn get_free_port(&mut self, speed: UsbSpeed) -> Result<u32, Error> {
        for i in 0..self.num_ports {
            let device = &self.virtual_devices[i as usize];

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

        Err(Error::NoFreePorts)
    }

    pub fn attach_device(
        &mut self,
        rh_port: u32,
        socket_fd: RawFd,
        bus_num: u32,
        dev_num: u32,
        speed: u32,
    ) -> Result<(), Error> {
        use std::{fs, io::Write};

        let device_id = (bus_num << 16) | dev_num;
        let buf = format!("{rh_port} {socket_fd} {device_id} {speed}");
        let attach_path = self.device.syspath().join("attach");

        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(attach_path)
            .map_err(|e| {
                if e.kind() == ErrorKind::PermissionDenied {
                    Error::SysfsPermissionDenied
                } else {
                    Error::SysfsIo(e.into())
                }
            })?;
        file.write_all(buf.as_bytes()).map_err(Error::SysfsIo)?;

        Ok(())
    }

    pub fn detach_device(&mut self, port: u16) -> Result<(), Error> {
        use std::{fs, io::Write};

        let buf = format!("{port}");
        let detach_path = self.device.syspath().join("detach");

        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(detach_path)
            .map_err(|e| {
                if e.kind() == ErrorKind::PermissionDenied {
                    Error::SysfsPermissionDenied
                } else {
                    Error::SysfsIo(e.into())
                }
            })?;
        file.write_all(buf.as_bytes()).map_err(Error::SysfsIo)?;

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

    pub fn cached_imported_devices(&self) -> &[VhciDevice] {
        &self.virtual_devices
    }
}

#[allow(dead_code)]
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
