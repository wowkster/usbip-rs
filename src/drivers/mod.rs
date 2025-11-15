use std::{
    ffi::OsStr,
    fs,
    io::{self, ErrorKind, Write},
    path::Path,
};

use nix::errno::Errno;

pub mod host;
pub mod vhci;

#[derive(Debug, thiserror::Error)]
pub enum DriverBindingError {
    #[error(transparent)]
    Sysfs(SysfsIoError),
    /// ENODEV
    #[error("failed to match driver or the device does not exist")]
    NoDevice,
    /// EINVAL
    #[error("device is already bound to another driver")]
    AlreadyBoundOther,
    /// EEXIST
    #[error("device is already bound to this driver")]
    AlreadyBound,
}

/// Try to bind the given driver to the given usb device. Will fail if the
/// driver does not exist, access to sysfs is denied, another driver is already
/// bound to the device, or if the device does not exist.
pub(crate) fn bind_usb_driver(driver: &OsStr, bus_id: &str) -> Result<(), DriverBindingError> {
    let path = Path::new("/sys/bus/usb/drivers/").join(driver).join("bind");

    let result = write_sysfs_attribute(&path, bus_id);

    if let Err(SysfsIoError::Other(e)) = &result
        && let Some(errno) = e.raw_os_error().map(Errno::from_raw)
    {
        match errno {
            Errno::ENODEV => return Err(DriverBindingError::NoDevice),
            Errno::EINVAL => return Err(DriverBindingError::AlreadyBoundOther),
            Errno::EEXIST => return Err(DriverBindingError::AlreadyBound),
            _ => {}
        }
    }

    result.map_err(DriverBindingError::Sysfs)
}

#[derive(Debug, thiserror::Error)]
pub enum DriverUnbindingError {
    #[error(transparent)]
    Sysfs(SysfsIoError),
    /// ENODEV
    #[error("device does not exist")]
    NoDevice,
    /// EINVAL
    #[error("device is not already bound to this driver")]
    NotBound,
}

/// Try to unbind the given driver from the given usb device. Will fail if the
/// driver does not exist, acccess to sysfs is denied, the given device is
/// not bound to this driver, or if the device does not exist.
pub(crate) fn unbind_usb_driver(driver: &OsStr, bus_id: &str) -> Result<(), DriverUnbindingError> {
    let path = Path::new("/sys/bus/usb/drivers/")
        .join(driver)
        .join("unbind");

    let result = write_sysfs_attribute(&path, bus_id);

    if let Err(SysfsIoError::Other(e)) = &result
        && let Some(errno) = e.raw_os_error().map(Errno::from_raw)
    {
        match errno {
            Errno::ENODEV => return Err(DriverUnbindingError::NoDevice),
            Errno::EINVAL => return Err(DriverUnbindingError::NotBound),
            _ => {}
        }
    }

    result.map_err(DriverUnbindingError::Sysfs)
}

#[derive(Debug, thiserror::Error)]
pub enum SysfsIoError {
    #[error(
        "missing permissions to access sysfs attribute{}",
        format_permissions_help()
    )]
    PermissionDenied,
    #[error("sysfs attribute does not exist")]
    DoesNotExist,
    #[error(transparent)]
    Other(io::Error),
}

fn format_permissions_help() -> String {
    if !nix::unistd::geteuid().is_root() {
        " (not running as root). try executing again with sudo.".into()
    } else {
        " (already running as root. how did we get ourselves here?)".into()
    }
}

pub(crate) fn write_sysfs_attribute(
    path: &Path,
    value: impl AsRef<[u8]>,
) -> Result<(), SysfsIoError> {
    tracing::debug!(
        "writing to sysfs (path = \"{}\", value = {:?})",
        path.display(),
        String::from_utf8_lossy(value.as_ref())
    );

    let mut file = fs::OpenOptions::new().write(true).open(path).map_err(|e| {
        if e.kind() == ErrorKind::PermissionDenied {
            SysfsIoError::PermissionDenied
        } else {
            SysfsIoError::Other(e)
        }
    })?;
    file.write_all(value.as_ref())
        .map_err(SysfsIoError::Other)?;

    Ok(())
}
