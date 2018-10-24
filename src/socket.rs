//! uTP socket related facilietis.

#![allow(unsafe_code)]

use super::UtpError;
use libutp_sys::*;
use std::net::Shutdown;

const MAX_SIZE: isize = isize::max_value();

/// Handle to virtual uTP socket that is not connected with a real socket.
/// Note, `UtpSocket` has no read, you will receive `CallbackType::OnRead` when data arrives.
pub struct UtpSocket {
    inner: *mut utp_socket,
}

impl UtpSocket {
    /// Write some data to uTP socket and return the result.
    /// Partial write is possible - uTP might not accept all the given buffer. In such case it's
    /// up to you to make sure the rest of the data is sent.
    pub fn send(&self, buf: &[u8]) -> Result<usize, UtpError> {
        let res = unsafe { utp_write(self.inner, buf.as_ptr() as *mut _, buf.len()) };
        match res {
            -1 => Err(UtpError::SendFailed),
            0 => Err(UtpError::WouldBlock),
            bytes_sent @ 1...MAX_SIZE => Ok(bytes_sent as usize),
            unknown => Err(UtpError::UnexpectedResult(unknown as i64)),
        }
    }

    /// Shutdown reads and/or writes on the socket.
    pub fn shutdown(&self, how: Shutdown) {
        let how = match how {
            Shutdown::Read => SHUT_RD,
            Shutdown::Write => SHUT_WR,
            Shutdown::Both => SHUT_RDWR,
        } as i32;
        unsafe {
            utp_shutdown(self.inner, how);
        }
    }

    // TODO(povilas): implement user data cause each socket can have it's own user data just like
    // uTP context
}

pub fn make_utp_socket(inner: *mut utp_socket) -> UtpSocket {
    UtpSocket { inner }
}

impl Drop for UtpSocket {
    fn drop(&mut self) {
        unsafe {
            utp_close(self.inner);
        }
    }
}
