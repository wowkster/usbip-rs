use std::{
    io::{self, ErrorKind},
    os::fd::AsRawFd,
};

use crate::{
    UsbDeviceInfo, UsbDeviceInfoValidationError,
    drivers::vhci::{
        Error as VhciHcdError, VhciHcd,
        state::{ConnectionRecord, FsStateError, save_connection_record},
    },
    net::UsbIpSocket,
    proto::{
        ImportReply, ImportRequest, OperationError, OperationKind, SYSFS_BUS_ID_SIZE,
        char_buf::CharBuf,
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

    #[error("Failed to parse PDU: {0}")]
    Protocol(#[from] UsbDeviceInfoValidationError),
    #[error("usbip network operation failed ({0})")]
    Operation(#[from] OperationError),
    #[error(transparent)]
    VhciHcdDriver(#[from] VhciHcdError),

    #[error(transparent)]
    FsState(#[from] FsStateError),
}

pub fn attach_device(host: &str, bus_id: &str) -> Result<u32, Error> {
    let mut socket = UsbIpSocket::connect_host_and_port(host, UsbIpSocket::DEFAULT_PORT)
        .map_err(Error::NetworkIo)?;

    let rh_port = query_and_import(&mut socket, bus_id)?;

    tracing::info!("device imported with port: {rh_port}");

    save_connection_record(
        rh_port,
        ConnectionRecord {
            host: host.into(),
            port: UsbIpSocket::DEFAULT_PORT,
            bus_id: bus_id.into(),
        },
    )?;

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

    import_device(socket, &reply.usb_device.try_into()?)
}

fn import_device(socket: &mut UsbIpSocket, remote_device: &UsbDeviceInfo) -> Result<u32, Error> {
    let mut vhci_hdc = VhciHcd::open()?;

    tracing::debug!(?vhci_hdc);
    tracing::debug!(?remote_device);

    loop {
        let rh_port = vhci_hdc.get_free_port(remote_device.speed)?;

        tracing::debug!("using free port: {rh_port}");

        match vhci_hdc.attach_device(
            rh_port,
            socket.as_raw_fd(),
            remote_device.bus_num,
            remote_device.dev_num,
            remote_device.speed as _,
        ) {
            Ok(_) => return Ok(rh_port),
            Err(VhciHcdError::SysfsIo(e)) if e.kind() == ErrorKind::ResourceBusy => continue,
            Err(e) => return Err(e.into()),
        }
    }
}
