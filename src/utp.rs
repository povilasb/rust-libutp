#![allow(unsafe_code)]

use libc;
use nix::sys::socket::{sockaddr, InetAddr, SockAddr};
use std::collections::HashMap;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::net::{Shutdown, SocketAddr};
use std::{mem, slice};

use utp_sys::*;

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

/// uTP connection state
#[derive(Debug, PartialEq)]
#[repr(u32)]
pub enum UtpState {
    /// socket has reveived syn-ack (notification only for outgoing connection completion)
    /// this implies writability
    Connected = UTP_STATE_CONNECT,

    /// socket is able to send more data
    Writable = UTP_STATE_WRITABLE,

    /// connection closed
    ConnectionClosed = UTP_STATE_EOF,

    /// socket is being destroyed, meaning all data has been sent if possible.
    /// it is not valid to refer to the socket after this state change occurs
    Destroying = UTP_STATE_DESTROYING,
}

// TODO(povilas): wrap utp context options:
//
// UTP_LOG_NORMAL,
// UTP_LOG_MTU,
// UTP_LOG_DEBUG,
// UTP_SNDBUF,
// UTP_RCVBUF,
// UTP_TARGET_DELAY,

/// Function type that will be called when some uTP event happens.
pub type UtpCallback<T> = Box<Fn(UtpCallbackArgs<T>) -> u64>;

/// libutp is capable of holding arbitrary user data. We will use this structure to hold our
/// context.
struct UtpUserData<T> {
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
}

/// Handle to virtual uTP socket that is not connected with a real socket.
/// Note, `UtpSocket` has no read, you will receive `CallbackType::OnRead` when data arrives.
pub struct UtpSocket {
    inner: *mut utp_socket,
}

impl UtpSocket {
    fn new(inner: *mut utp_socket) -> Self {
        Self { inner }
    }

    /// Write some data to uTP socket and return the result.
    // TODO(povilas): wrap isize to Result
    pub fn send(&self, buf: &[u8]) -> isize {
        unsafe { utp_write(self.inner, buf.as_ptr() as *mut _, buf.len()) }
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

impl Drop for UtpSocket {
    fn drop(&mut self) {
        unsafe {
            utp_close(self.inner);
        }
    }
}

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
    // TODO(povilas): return proper Rust result instead of i32
    pub fn process_udp(&self, packet: &[u8], sender_addr: SocketAddr) -> i32 {
        let (sockaddr, socklen) = c_sock_addr(sender_addr);
        unsafe { utp_process_udp(self.ctx, packet.as_ptr(), packet.len(), &sockaddr, socklen) }
    }

    /// Enables or disables debug logging.
    pub fn set_debug_log(&mut self, debug_log: bool) {
        self.set_option(UTP_LOG_DEBUG, i32::from(debug_log));
    }

    // TODO(povilas): return proper Rust error instead of i32
    /// Attempt to make a uTP connection to a given address.
    pub fn connect(&mut self, addr: SocketAddr) -> Result<UtpSocket, i32> {
        let (sockaddr, socklen) = c_sock_addr(addr);
        let (sock, res) = unsafe {
            let sock = utp_create_socket(self.ctx);
            let res = utp_connect(sock, &sockaddr, socklen);
            (sock, res)
        };
        if res == 0 {
            Ok(UtpSocket::new(sock))
        } else {
            // TODO(povilas): destroy socket handle on error. NOTE: currently there's no way to do
            // this in libutp.
            Err(res)
        }
    }

    /// Sends all deferred ACK packets.
    /// This method should be called when real UDP socket becomes unreadable - returns EWOULDBLOCK.
    pub fn ack_packets(&self) {
        unsafe {
            utp_issue_deferred_acks(self.ctx);
        }
    }

    /// Checks for timedout connections. Should be called every 500ms.
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

/// Converts Rust socket address into corresponding C data type.
fn c_sock_addr(addr: SocketAddr) -> (sockaddr, u32) {
    let sockaddr = SockAddr::new_inet(InetAddr::from_std(&addr));
    let (sockaddr, socklen) = unsafe { sockaddr.as_ffi_pair() };
    (*sockaddr, socklen)
}

/// Initialize all possible uTP callbacks.
/// Each uTP callback will call appropriate Rust function defined in `UserData`.
fn init_callbacks<T>(ctx: *mut utp_context) {
    macro_rules! set_callback {
        ($cb_type:expr) => {{
            unsafe extern "C" fn c_callback<T>(raw_args: *mut utp_callback_arguments) -> uint64 {
                let args: UtpCallbackArgs<T> = UtpCallbackArgs::wrap(raw_args);
                let args2 = UtpCallbackArgs::wrap(raw_args);
                (*args.utp_user_data().callbacks[&$cb_type])(args2)
            }
            unsafe { utp_set_callback(ctx, $cb_type as i32, Some(c_callback::<T>)) }
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
        &self.utp_user_data().data
    }

    /// Acknowledges received data.
    /// This function must be called from `OnRead` callback otherwise received data won't
    /// be acknowledged.
    pub fn ack_data(&mut self) {
        unsafe { utp_read_drained((*self.inner).socket) }
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

    fn utp_user_data(&self) -> &UtpUserData<T> {
        unsafe {
            get_user_data::<UtpUserData<T>>((*self.inner).context)
                .expect("User data must be always set.")
        }
    }
}

/// Retrieve previously stored Rust object from `UtpContext`.
/// Returns `None`, if no object was stored.
/// Note that you must make sure the type `T` is correct.
fn get_user_data<'a, T>(ctx: *mut utp_context) -> Option<&'a T> {
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
