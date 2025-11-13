//! Implements functions used to keep track of some of the usbip connection
//! state in userspace. Since the resolving of remote hosts is handled by the
//! userspace portion of the code before handing the socket off to the kernel,
//! it's not possible to figure out which remote host is associated which each
//! device port unless we store that state somwhere. The original usbip CLI uses
//! a directory in `/var/run` so we also do it the same way for backwards
//! compatability.

use std::{
    fs,
    io::{self, ErrorKind, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

const VHCI_STATE_PATH: &str = "/var/run/vhci_hcd";

#[derive(Debug, thiserror::Error)]
pub enum FsStateError {
    #[error("Failed to save userspace `vhci_hcd` state to the file-system ({0})")]
    IoWrite(io::Error),
    #[error("File-system `vhci_hcd` state path already exists, but is not a directory")]
    NotADirectory,

    #[error(
        "Failed to read userspace `vhci_hcd` state from the file-system for device on port {1} ({0})"
    )]
    IoRead(io::Error, u16),
    #[error("Failed to parse file-system `vhci_hcd` state file for device on port {0}")]
    Parsing(u16),

    #[error("Failed to delete userspace `vhci_hcd` state from the file-system ({0})")]
    IoRemove(io::Error),
}

/// Represents the connection paramters that were used during initial device
/// attachment
#[derive(Debug)]
pub struct ConnectionRecord {
    /// IP Host used to connect to the usbip server
    pub host: String,
    /// TCP port used to connect to the usbip server
    pub port: u16,
    /// Remote USB bus ID that this vhci_hcd port is connected to
    pub bus_id: String,
}

/// Records the remote connection in a file like `/var/run/vhci_hcd/portX` to be
/// referenced by other processes. This is done in the same way as the original
/// implementation to keep backwards compatability.
pub fn save_connection_record(rh_port: u32, record: ConnectionRecord) -> Result<(), FsStateError> {
    /* ==== mkdir with permissions ==== */

    let state_path = Path::new(VHCI_STATE_PATH);

    match fs::create_dir(state_path) {
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            if !state_path
                .metadata()
                .map_err(FsStateError::IoWrite)?
                .is_dir()
            {
                return Err(FsStateError::NotADirectory);
            }
        }
        Err(e) => return Err(FsStateError::IoWrite(e)),
    }

    let mut perms = fs::metadata(state_path)
        .map_err(FsStateError::IoWrite)?
        .permissions();
    perms.set_mode(0o700);
    fs::set_permissions(state_path, perms).map_err(FsStateError::IoWrite)?;

    /* ==== create the port file ==== */

    let port_path = state_path.join(format!("port{rh_port}"));

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o700)
        .open(port_path)
        .map_err(FsStateError::IoWrite)?;

    file.write_all(format!("{} {} {}\n", record.host, record.port, record.bus_id).as_bytes())
        .map_err(FsStateError::IoWrite)?;

    Ok(())
}

pub fn read_connection_record(rh_port: u16) -> Result<ConnectionRecord, FsStateError> {
    use std::fs;

    let port_path = Path::new(VHCI_STATE_PATH).join(format!("port{rh_port}"));

    let mut file = fs::OpenOptions::new()
        .read(true)
        .open(port_path)
        .map_err(|e| FsStateError::IoRead(e, rh_port))?;

    let mut buf = String::new();
    file.read_to_string(&mut buf)
        .map_err(|e| FsStateError::IoRead(e, rh_port))?;

    let (remote_host, port, remote_bus_id) = sscanf::sscanf!(buf.trim(), "{str} {u16} {str}")
        .map_err(|_| FsStateError::Parsing(rh_port))?;

    Ok(ConnectionRecord {
        host: remote_host.into(),
        port,
        bus_id: remote_bus_id.into(),
    })
}

/// Deletes a previously saved connection record from the file system state
/// directory. If no other entries exist in the `/var/run/vhci_hcd` directory,
/// it is also removed.
pub fn delete_connection_record(port: u16, remove_state_dir: bool) -> Result<(), FsStateError> {
    let state_path = Path::new(VHCI_STATE_PATH);
    let port_path = state_path.join(format!("port{port}"));

    if let Err(e) = fs::remove_file(port_path) {
        if e.kind() != io::ErrorKind::NotFound {
            return Err(FsStateError::IoRemove(e));
        }
    }

    if remove_state_dir {
        if let Err(e) = fs::remove_dir(state_path) {
            if e.kind() != io::ErrorKind::DirectoryNotEmpty && e.kind() != io::ErrorKind::NotFound {
                return Err(FsStateError::IoRemove(e));
            }

            if e.kind() == io::ErrorKind::NotFound {
                tracing::warn!("vhci_hcd state directory not found")
            }
        }
    }

    Ok(())
}
