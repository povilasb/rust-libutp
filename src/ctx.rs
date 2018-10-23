//! uTP context related facilities.

#![allow(unsafe_code)]

use super::UtpError;
use callback::{get_user_data_from_args, UtpCallback, UtpCallbackArgs, UtpCallbackType};
use nix::sys::socket::{sockaddr, InetAddr, SockAddr};
use socket::{make_utp_socket, UtpSocket};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::net::SocketAddr;
use utp_sys::*;

/// To manipulate the user data held inside uTP context use `UtpContextRef` which is acquired with
/// `UtpContext::get_ref()`.
pub struct UtpContext<T> {
    ctx: *mut utp_context,
    _user_data_type: PhantomData<T>,
}

impl<T> UtpContext<T> {
    /// Construct uTP context with given user data.
    pub fn new(user_data: T) -> Self {
        let ctx = unsafe { utp_init(2) };

        // create user data on the heap and keep a pointer to it inside uTP context.
        // NOTE: don't forget to destroy this user data.
        // TODO(povilas): guard user data with mutex?
        let utp_user_data = Box::new(UtpUserData::new(user_data));
        unsafe {
            let _ = utp_context_set_userdata(ctx, Box::into_raw(utp_user_data) as *mut _);
        };

        init_callbacks::<T>(ctx);
        Self {
            ctx,
            _user_data_type: PhantomData,
        }
    }

    /// Returns reference to arbitrary user data stored in uTP context.
    pub fn user_data(&self) -> &T {
        &self.utp_user_data().data
    }

    /// Returns mutable reference to arbitrary user data stored in uTP context.
    pub fn user_data_mut(&mut self) -> &mut T {
        let user_data = get_user_data_mut::<UtpUserData<T>>(self.ctx)
            .expect("uTP user data must be always set.");
        &mut user_data.data
    }

    /// Sets some internal uTP context options.
    pub fn set_option(&mut self, opt: u32, val: i32) {
        unsafe {
            let _ = utp_context_set_option(self.ctx, opt as i32, val);
        };
    }

    /// Set uTP callback. The underlying libutp uses callbacks to react to asyncrhonous evens:
    /// on data read, on connection established, etc.
    pub fn set_callback(&mut self, cb_type: UtpCallbackType, cb: UtpCallback<T>) {
        let _ = self.utp_user_data_mut().callbacks.insert(cb_type, cb);
    }

    /// Feed UDP packet to underlying uTP library that will process it and react appropriately:
    /// e.g. terminate connection or call `UtpCallbackType::OnRead` callback, etc.
    pub fn process_udp(&self, packet: &[u8], sender_addr: SocketAddr) -> Result<(), UtpError> {
        let (sockaddr, socklen) = c_sock_addr(sender_addr);
        let res =
            unsafe { utp_process_udp(self.ctx, packet.as_ptr(), packet.len(), &sockaddr, socklen) };
        match res {
            1 => Ok(()),
            0 => Err(UtpError::IllegalPacket),
            result => Err(UtpError::UnexpectedResult(i64::from(result))),
        }
    }

    /// Enables or disables debug logging.
    pub fn set_debug_log(&mut self, debug_log: bool) {
        self.set_option(UTP_LOG_DEBUG, i32::from(debug_log));
    }

    /// Attempt to make a uTP connection to a given address.
    pub fn connect(&mut self, addr: SocketAddr) -> Result<UtpSocket, UtpError> {
        let (sockaddr, socklen) = c_sock_addr(addr);
        let (sock, res) = unsafe {
            let sock = utp_create_socket(self.ctx);
            let res = utp_connect(sock, &sockaddr, socklen);
            (sock, res)
        };
        match res {
            0 => Ok(make_utp_socket(sock)),
            // TODO(povilas): destroy socket handle on error. NOTE: currently there's no way to do
            // this in libutp.
            -1 => Err(UtpError::ConnectFailed),
            result => Err(UtpError::UnexpectedResult(i64::from(result))),
        }
    }

    /// Sends all deferred ACK packets.
    /// This method should be called when real UDP socket becomes unreadable - returns EWOULDBLOCK.
    pub fn ack_packets(&self) {
        unsafe {
            utp_issue_deferred_acks(self.ctx);
        }
    }

    /// Checks for timedout connections, ACK packets, reschedules lost packets, etc.
    /// Should be called every 500ms - recommendation from libutp.
    pub fn check_timeouts(&mut self) {
        unsafe { utp_check_timeouts(self.ctx) }
    }

    fn utp_user_data(&self) -> &UtpUserData<T> {
        get_user_data::<UtpUserData<T>>(self.ctx).expect("uTP user data must be always set.")
    }

    fn utp_user_data_mut(&mut self) -> &mut UtpUserData<T> {
        get_user_data_mut::<UtpUserData<T>>(self.ctx).expect("uTP user data must be always set.")
    }
}

/// Retrieve previously stored Rust object from `UtpContext`.
/// Returns `None`, if no object was stored.
/// Note that you must make sure the type `T` is correct.
pub fn get_user_data<'a, T>(ctx: *mut utp_context) -> Option<&'a T> {
    unsafe {
        let data = utp_context_get_userdata(ctx) as *const T;
        if data.is_null() {
            None
        } else {
            Some(&*data)
        }
    }
}

/// Same as `get_user_data` except returns a mutable reference.
fn get_user_data_mut<'a, T>(ctx: *mut utp_context) -> Option<&'a mut T> {
    unsafe {
        let data = utp_context_get_userdata(ctx) as *mut T;
        if data.is_null() {
            None
        } else {
            Some(&mut *data)
        }
    }
}

impl<T> Drop for UtpContext<T> {
    fn drop(&mut self) {
        unsafe {
            let user_data_ptr = utp_context_get_userdata(self.ctx) as *mut UtpUserData<T>;
            let _ = Box::from_raw(user_data_ptr); // this will make sure UserData is dropped properly.
            utp_destroy(self.ctx);
        }
    }
}

/// Initialize all possible uTP callbacks.
/// Each uTP callback will call appropriate Rust function defined in `UserData`.
fn init_callbacks<T>(ctx: *mut utp_context) {
    macro_rules! set_callback {
        ($cb_type:expr) => {{
            unsafe extern "C" fn c_utp_callback<T>(raw_args: *mut utp_callback_arguments) -> uint64 {
                let args: UtpCallbackArgs<T> = UtpCallbackArgs::wrap(raw_args);
                let args2 = UtpCallbackArgs::wrap(raw_args);
                (*get_user_data_from_args(&args).callbacks[&$cb_type])(args2)
            }
            unsafe { utp_set_callback(ctx, $cb_type as i32, Some(c_utp_callback::<T>)) }
        }};
    }

    // TODO(povilas): uncomment once implemented, cause if `GetUdpMtu` is implemented unproperly,
    // it craches. Other callbacks coud crash too.

    // set_callback!(UtpCallbackType::OnFirewall);
    // set_callback!(UtpCallbackType::OnConnect);
    set_callback!(UtpCallbackType::OnAccept);
    set_callback!(UtpCallbackType::OnError);
    set_callback!(UtpCallbackType::OnRead);
    // set_callback!(UtpCallbackType::OnOverheadStatistics);
    set_callback!(UtpCallbackType::OnStateChange);
    // set_callback!(UtpCallbackType::GetReadBufferSize);
    // set_callback!(UtpCallbackType::OnDelaySample);
    // set_callback!(UtpCallbackType::GetUdpMtu);
    // set_callback!(UtpCallbackType::GetUdpOverhead);
    // set_callback!(UtpCallbackType::GetMiliseconds);
    // set_callback!(UtpCallbackType::GetMicroseconds);
    // set_callback!(UtpCallbackType::GetRandom);
    set_callback!(UtpCallbackType::Log);
    set_callback!(UtpCallbackType::Sendto);
}

/// Converts Rust socket address into corresponding C data type.
fn c_sock_addr(addr: SocketAddr) -> (sockaddr, u32) {
    let sockaddr = SockAddr::new_inet(InetAddr::from_std(&addr));
    let (sockaddr, socklen) = unsafe { sockaddr.as_ffi_pair() };
    (*sockaddr, socklen)
}

/// libutp is capable of holding arbitrary user data. We will use this structure to hold our
/// context.
pub struct UtpUserData<T> {
    data: T,
    callbacks: HashMap<UtpCallbackType, UtpCallback<T>>,
}

impl<T> UtpUserData<T> {
    fn new(data: T) -> Self {
        // no operation - a.k.a do nothing default callbacks do nothing.
        let nop = Box::new(|_| 0);
        let mut callbacks: HashMap<UtpCallbackType, UtpCallback<T>> = HashMap::new();
        let _ = callbacks.insert(UtpCallbackType::OnFirewall, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::OnAccept, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::OnConnect, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::OnError, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::OnRead, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::OnOverheadStatistics, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::OnStateChange, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::GetReadBufferSize, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::OnDelaySample, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::GetUdpMtu, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::GetUdpOverhead, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::GetMiliseconds, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::GetMicroseconds, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::GetRandom, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::Log, nop.clone());
        let _ = callbacks.insert(UtpCallbackType::Sendto, nop);

        Self { data, callbacks }
    }

    /// Returns reference to user data.
    pub fn data(&self) -> &T {
        &self.data
    }
}
