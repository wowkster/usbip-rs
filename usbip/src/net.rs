use std::{
    io::{self, Read, Write},
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    os::fd::{AsRawFd, RawFd},
};

use endian_codec::{DecodeBE, EncodeBE};
use socket2::{Domain, Socket, Type};

use crate::proto::{
    Direction, OperationError, OperationHeader, OperationKind, OperationStatus, USBIP_VERSION,
};

/// A TCP socket wrapper which is shared by the server and the client and
/// provides helper methods for common USB IP network operations
pub struct UsbIpSocket {
    inner: Socket,
}

impl UsbIpSocket {
    pub const DEFAULT_PORT: u16 = 3240;

    pub fn connect_host_and_port(host: &str, port: u16) -> io::Result<Self> {
        let addr = if let Ok(ip) = host.parse::<IpAddr>() {
            SocketAddr::new(ip, port)
        } else {
            // TODO: try all addresses (original impl does this)

            (host, port).to_socket_addrs()?.next().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "No addresses found")
            })?
        };

        Self::connect(addr)
    }

    pub fn connect(addr: SocketAddr) -> io::Result<Self> {
        let socket = Socket::new(Domain::for_address(addr), Type::STREAM, None)?;

        socket.set_tcp_nodelay(true)?;
        socket.set_keepalive(true)?;

        socket.connect(&addr.into())?;

        Ok(Self { inner: socket })
    }

    pub fn bind(_addr: SocketAddr) -> io::Result<Self> {
        todo!()
    }

    #[inline]
    pub fn send(&mut self, data: &[u8]) -> io::Result<()> {
        self.inner.write_all(data)
    }

    #[inline]
    pub fn recv(&mut self, data: &mut [u8]) -> io::Result<()> {
        self.inner.read_exact(data)
    }

    pub fn send_encoded<T: EncodeBE>(&mut self, data: T) -> io::Result<()>
    where
        [u8; T::PACKED_LEN]:,
    {
        let mut buffer = [0; T::PACKED_LEN];

        data.encode_as_be_bytes(&mut buffer);

        self.send(&buffer)
    }

    pub fn recv_encoded<T: DecodeBE>(&mut self) -> io::Result<T>
    where
        [u8; T::PACKED_LEN]:,
    {
        let mut buffer = [0; T::PACKED_LEN];

        self.recv(&mut buffer)?;

        Ok(T::decode_from_be_bytes(&buffer))
    }

    pub fn send_request_header(&mut self, kind: OperationKind) -> io::Result<()> {
        self.send_encoded(OperationHeader {
            version: USBIP_VERSION,
            code: Direction::Request as u16 | kind as u16,
            status: OperationStatus::Ok as _,
        })
    }

    pub fn send_response_header(
        &mut self,
        kind: OperationKind,
        status: OperationStatus,
    ) -> io::Result<()> {
        self.send_encoded(OperationHeader {
            version: USBIP_VERSION,
            code: Direction::Reply as u16 | kind as u16,
            status: status as _,
        })
    }

    pub fn recv_request_header(&mut self) -> io::Result<OperationHeader> {
        todo!()
    }

    // TODO: this interface is weird. lets use a global error type instead.
    pub fn recv_reply_header(
        &mut self,
        kind: OperationKind,
    ) -> io::Result<Result<(), OperationError>> {
        let header = self.recv_encoded::<OperationHeader>()?;

        if header.version != USBIP_VERSION {
            return Ok(Err(OperationError::VersionMismatch));
        }

        if Direction::from_code(header.code) != Direction::Reply {
            return Ok(Err(OperationError::DirectionMismatch));
        }

        match OperationKind::from_code(header.code) {
            Some(OperationKind::Unspecified) => {}
            k => {
                if k != Some(kind) {
                    return Ok(Err(OperationError::InvalidData));
                }
            }
        }

        Ok(Err(
            match OperationStatus::from_raw(header.status).unwrap_or(OperationStatus::Error) {
                OperationStatus::Ok => return Ok(Ok(())),
                OperationStatus::Failure => OperationError::RequestFailed,
                OperationStatus::DeviceBusy => OperationError::DeviceBusy,
                OperationStatus::DeviceError => OperationError::DeviceError,
                OperationStatus::NoSuchDevice => OperationError::NoSuchDevice,
                OperationStatus::Error => OperationError::Other,
            },
        ))
    }
}

impl AsRawFd for UsbIpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
