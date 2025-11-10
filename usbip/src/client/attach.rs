use std::{
    io::{self, ErrorKind, Write},
    os::fd::AsRawFd,
    path::Path,
};

use crate::{
    client::VHCI_STATE_PATH,
    drivers::vhci_hcd::{Error as VhciHcdError, UsbSpeed, VhciHcd},
    net::UsbIpSocket,
    proto::{
        CharBuf, ImportReply, ImportRequest, OperationError, OperationKind, SYSFS_BUS_ID_SIZE,
        UsbDeviceInfo,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Network connection failed ({0})")]
    NetworkIo(io::Error),

    #[error("Provided bus ID is too long (max size is {SYSFS_BUS_ID_SIZE} bytes)")]
    BusIdTooLong,
    #[error("Bus ID returned by the server did not match the one that was sent")]
    BusIdMismatch,

    #[error("Failed to parse PDU")]
    ProtocolError,
    #[error("usbip network operation failed ({0})")]
    OperationError(#[from] OperationError),
    #[error(transparent)]
    VhciHcdDriver(#[from] VhciHcdError),

    #[error("Failed to save `vhci_hcd` state to the file-system ({0})")]
    FsIo(io::Error),
    #[error("File-system `vhci_hcd` state path already exists, but is not a directory")]
    FsStateNotADirectory,
}

pub fn attach_device(host: &str, bus_id: &str) -> Result<u32, Error> {
    let mut socket = UsbIpSocket::connect_host_and_port(host, UsbIpSocket::DEFAULT_PORT)
        .map_err(Error::NetworkIo)?;

    let rh_port = query_and_import(&mut socket, bus_id)?;

    tracing::info!("device imported with port: {rh_port}");

    record_connection(host, UsbIpSocket::DEFAULT_PORT, bus_id, rh_port)?;

    tracing::debug!("connection recorded");

    Ok(rh_port)
}

fn query_and_import(socket: &mut UsbIpSocket, bus_id: &str) -> Result<u32, Error> {
    let op_kind = OperationKind::Import;

    socket
        .send_request_header(op_kind)
        .map_err(Error::NetworkIo)?;
    socket
        .send_encoded(ImportRequest {
            bus_id: CharBuf::new(bus_id).ok_or(Error::BusIdTooLong)?,
        })
        .map_err(Error::NetworkIo)?;

    socket
        .recv_reply_header(op_kind)
        .map_err(Error::NetworkIo)??;
    let reply = socket
        .recv_encoded::<ImportReply>()
        .map_err(Error::NetworkIo)?;

    if reply
        .usb_device
        .bus_id
        .as_c_str()
        .is_none_or(|bid| bid.to_string_lossy() != bus_id)
    {
        return Err(Error::BusIdMismatch);
    }

    tracing::debug!(?reply);

    import_device(socket, &reply.usb_device)
}

fn import_device(socket: &mut UsbIpSocket, remote_device: &UsbDeviceInfo) -> Result<u32, Error> {
    let mut vhci_hdc = VhciHcd::open()?;

    tracing::debug!(?vhci_hdc);
    tracing::debug!(?remote_device);

    let speed = UsbSpeed::try_from(remote_device.speed).map_err(|_| Error::ProtocolError)?;

    loop {
        let rh_port = vhci_hdc.get_free_port(speed)?;

        tracing::debug!("using free port: {rh_port}");

        match vhci_hdc.attach_device(
            rh_port,
            socket.as_raw_fd(),
            remote_device.bus_num,
            remote_device.dev_num,
            remote_device.speed,
        ) {
            Ok(_) => return Ok(rh_port),
            Err(VhciHcdError::SysfsIo(e)) if e.kind() == ErrorKind::ResourceBusy => continue,
            Err(e) => return Err(e.into()),
        }
    }
}

/// Records the remote connection in a file like `/var/run/vhci_hcd/portX` to be
/// referenced by other processes without having to interface with the vhci_hcd
/// driver. This is done in the same way as the original implementation.
fn record_connection(host: &str, port: u16, bus_id: &str, rh_port: u32) -> Result<(), Error> {
    use std::{
        fs,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };

    /* ==== mkdir with permissions ==== */

    let state_path = Path::new(VHCI_STATE_PATH);

    match fs::create_dir(state_path) {
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            if !state_path.metadata().map_err(Error::FsIo)?.is_dir() {
                return Err(Error::FsStateNotADirectory);
            }
        }
        Err(e) => return Err(Error::FsIo(e)),
    }

    let mut perms = fs::metadata(state_path).map_err(Error::FsIo)?.permissions();
    perms.set_mode(0o700);
    fs::set_permissions(state_path, perms).map_err(Error::FsIo)?;

    /* ==== create the port file ==== */

    let port_path = state_path.join(format!("port{rh_port}"));

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o700)
        .open(port_path)
        .map_err(Error::FsIo)?;

    file.write_all(format!("{host} {port} {bus_id}\n").as_bytes())
        .map_err(Error::FsIo)?;

    Ok(())
}
