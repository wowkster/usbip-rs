use std::{
    io::{self, ErrorKind, Write},
    os::fd::AsRawFd,
    path::Path,
};

use crate::{
    client::VHCI_STATE_PATH, drivers::vhci_hcd::{UsbSpeed, VhciHcd, VhciHcdError}, net::UsbIpSocket, proto::{CharBuf, ImportReply, ImportRequest, OperationError, OperationKind, UsbDeviceInfo}
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Bus ID is too long")]
    BusIdTooLong,
    #[error("Bus ID is mismatch")]
    BusIdMismatch,
    #[error("Failed to parse PDU")]
    ProtocolError,
    #[error("Operation failed: {0:?}")]
    OperationError(#[from] OperationError),
    #[error("VHCI state path already exists, but is not a directory")]
    VhciHcdStateInvalid,
    #[error(transparent)]
    VhciHcdDriver(#[from] VhciHcdError),
}

pub fn attach_device(host: &str, bus_id: &str) -> Result<u32, Error> {
    let mut socket = UsbIpSocket::connect_host_and_port(host, UsbIpSocket::DEFAULT_PORT)?;

    let rh_port = query_and_import(&mut socket, bus_id)?;

    tracing::info!("device imported with port: {rh_port}");

    record_connection(host, UsbIpSocket::DEFAULT_PORT, bus_id, rh_port)?;

    tracing::debug!("connection recorded");

    Ok(rh_port)
}

fn query_and_import(socket: &mut UsbIpSocket, bus_id: &str) -> Result<u32, Error> {
    let op_kind = OperationKind::Import;

    socket.send_request_header(op_kind)?;
    socket.send_encoded(ImportRequest {
        bus_id: CharBuf::new(bus_id).ok_or(Error::BusIdTooLong)?,
    })?;

    socket.recv_reply_header(op_kind)??;
    let reply = socket.recv_encoded::<ImportReply>()?;

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
            Err(VhciHcdError::Io(e)) if e.kind() == ErrorKind::ResourceBusy => continue,
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
            if !state_path.metadata()?.is_dir() {
                return Err(Error::VhciHcdStateInvalid);
            }
        }
        Err(e) => return Err(e.into()),
    }

    let mut perms = fs::metadata(state_path)?.permissions();
    perms.set_mode(0o700);
    fs::set_permissions(state_path, perms)?;

    /* ==== create the port file ==== */

    let port_path = state_path.join(format!("port{rh_port}"));

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o700)
        .open(port_path)?;

    file.write_all(format!("{host} {port} {bus_id}\n").as_bytes())?;

    Ok(())
}
