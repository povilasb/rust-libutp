//! uTP callback related facilities.

#![allow(unsafe_code)]

use super::UtpState;
use crate::ctx::{get_user_data, UtpUserData};
use crate::socket::{make_utp_socket, UtpSocket};
use libc;
use libutp_sys::*;
use nix::sys::socket::SockAddr;
use std::ffi::CStr;
use std::io;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::{mem, slice};

/// Identifies uTP callback.
#[derive(Hash, Eq, PartialEq)]
#[repr(u32)]
pub enum UtpCallbackType {
    /// With this callback you can allow/reject connections based on some criteria.
    OnFirewall = UTP_ON_FIREWALL,
    /// Called when new incoming connection was accepted.
    OnAccept = UTP_ON_ACCEPT,
    /// Called when successfully connectedto uTP server.
    OnConnect = UTP_ON_CONNECT,
    /// Called if any error happened.
    OnError = UTP_ON_ERROR,
    /// Called when uTP data packet received.
    OnRead = UTP_ON_READ,
    /// This callback allows to collect statistics for misc packets: connect, data, ack, etc.
    /// Overhead is UDP + uTP header.
    OnOverheadStatistics = UTP_ON_OVERHEAD_STATISTICS,
    /// Called when uTP state changes.
    OnStateChange = UTP_ON_STATE_CHANGE,
    /// This one is very important for congestion control. It asks for remaining receive buffer
    /// size - how much more bytes can uTP receive.
    GetReadBufferSize = UTP_GET_READ_BUFFER_SIZE,
    /// uTP tracks delay between peers. You can use this callback to get those delay samples
    /// every time they are recalculated.
    OnDelaySample = UTP_ON_DELAY_SAMPLE,
    /// Allows to provide the initial UDP maximum transfer unit size for the uTP library.
    GetUdpMtu = UTP_GET_UDP_MTU,
    /// Allows to specify UDP header size - overhead for uTP data.
    GetUdpOverhead = UTP_GET_UDP_OVERHEAD,
    /// We must give current time in milliseconds when uTP asks.
    GetMiliseconds = UTP_GET_MILLISECONDS,
    /// We must give current time in microseconds when uTP asks.
    GetMicroseconds = UTP_GET_MICROSECONDS,
    /// Give some random number to uTP.
    GetRandom = UTP_GET_RANDOM,
    /// Each log message results in this callback. Do with the message what you want.
    Log = UTP_LOG,
    /// When uTP libray has some uTP packets ready, this callback is called with a raw packet
    /// representation. Then you can send this packet over UDP.
    Sendto = UTP_SENDTO,
}

/// Function type that will be called when some uTP event happens.
pub type UtpCallback<T> = Box<Fn(UtpCallbackArgs<T>) -> u64>;

/// Gives a more Rust'ish interface to callback arguments. Each libutp callback receives this
/// structure.
pub struct UtpCallbackArgs<T> {
    inner: *mut utp_callback_arguments,
    _user_data_type: PhantomData<T>,
}

impl<T> UtpCallbackArgs<T> {
    /// Wraps libutp callback arguments to a more Rust'ish interface.
    pub fn wrap(inner: *mut utp_callback_arguments) -> Self {
        Self {
            inner,
            _user_data_type: PhantomData,
        }
    }

    /// Returns socket address, if it's IPv4 or IPv4. Otherwise `None` is returned.
    pub fn address(&self) -> Option<SocketAddr> {
        let addr_opt = unsafe {
            let addr = (*self.inner).args1.address;
            SockAddr::from_libc_sockaddr(addr)
        };
        match addr_opt {
            Some(SockAddr::Inet(addr)) => Some(addr.to_std()),
            _ => None,
        }
    }

    /// Returns connection state.
    pub fn state(&self) -> UtpState {
        unsafe {
            let state = (*self.inner).args1.state;
            mem::transmute(state)
        }
    }

    /// Returns immutable slice to the buffer used for a specific callback, say `on_read`.
    pub fn buf(&self) -> &[u8] {
        unsafe {
            let buf = (*self.inner).buf;
            let buf_len = (*self.inner).len;
            slice::from_raw_parts(buf, buf_len)
        }
    }

    /// Returns user data associated with the uTP context which is accessible from the uTP
    /// callback arguments.
    pub fn user_data(&self) -> &T {
        &get_user_data_from_args(self).data()
    }

    /// Acknowledges received data.
    /// This function must be called from `OnRead` callback otherwise received data won't
    /// be acknowledged.
    pub fn ack_data(&mut self) {
        unsafe { utp_read_drained((*self.inner).socket) }
    }

    /// Returns owned uTP socket object.
    /// Socket will be closed, when this object is dropped.
    // TODO(povilas): figure out the way to return non-owned (reference to) uTP socket.
    pub fn socket(&self) -> UtpSocket {
        unsafe { make_utp_socket((*self.inner).socket) }
    }

    /// In some cases (e.g. logging), `buf` argument holds a C style, 0 terminated, string.
    /// This function converts such string into Rust `String`.
    pub fn buf_as_string(&self) -> String {
        unsafe {
            CStr::from_ptr((*self.inner).buf as *const libc::c_char)
                .to_string_lossy()
                .into_owned()
        }
    }

    /// Returns error that was passed to `OnError` callback.
    /// Should only be used from `OnError` callback.
    pub fn error(&self) -> io::Error {
        let err_code = unsafe { (*self.inner).args1.error_code } as u32;
        match err_code {
            UTP_ECONNREFUSED => io::ErrorKind::ConnectionRefused.into(),
            UTP_ECONNRESET => io::ErrorKind::ConnectionReset.into(),
            UTP_ETIMEDOUT => io::ErrorKind::TimedOut.into(),
            _ => io::ErrorKind::Other.into(),
        }
    }
}

/// Returns pointer to user data which callback arguments point to.
pub fn get_user_data_from_args<T>(args: &UtpCallbackArgs<T>) -> &UtpUserData<T> {
    unsafe {
        get_user_data::<UtpUserData<T>>((*args.inner).context)
            .expect("User data must be always set.")
    }
}
