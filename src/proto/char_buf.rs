use std::{
    ffi::{CStr, OsStr},
    os::unix::ffi::OsStrExt,
};

use endian_codec::{DecodeBE, EncodeBE, PackedSize};

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
