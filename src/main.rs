#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

extern crate libc;
extern crate nix;

use libc::c_char;
use nix::sys::socket::{sockaddr, sockaddr_in, sockaddr_in6, sockaddr_storage, InetAddr, SockAddr};
use std::ffi::CStr;
use std::net::{SocketAddr, UdpSocket};
use std::{env, io, slice};

#[repr(u32)]
enum UtpCallbackType {
    OnFirewall = UTP_ON_FIREWALL,
    OnAccept = UTP_ON_ACCEPT,
    OnConnect = UTP_ON_CONNECT,
    OnError = UTP_ON_ERROR,
    OnRead = UTP_ON_READ,
    OnOverhead = UTP_ON_OVERHEAD_STATISTICS,
    OnStateChange = UTP_ON_STATE_CHANGE,
    GetReadBufferSize = UTP_GET_READ_BUFFER_SIZE,
    OnDelaySample = UTP_ON_DELAY_SAMPLE,
    GetUdpMtu = UTP_GET_UDP_MTU,
    GetUdpOverhead = UTP_GET_UDP_OVERHEAD,
    GetMiliseconds = UTP_GET_MILLISECONDS,
    GetMicroseconds = UTP_GET_MICROSECONDS,
    GetRandom = UTP_GET_RANDOM,
    Log = UTP_LOG,
    Sendto = UTP_SENDTO,
}

#[derive(Debug)]
#[repr(u32)]
enum UtpState {
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

struct UtpContext {
    ctx: *mut utp_context,
}

impl UtpContext {
    fn new() -> Self {
        let ctx = unsafe { utp_init(2) };
        UtpContext { ctx }
    }

    fn context_set_option(&self, opt: u32, val: i32) {
        unsafe { utp_context_set_option(self.ctx, opt as i32, val) };
    }

    fn set_callback(&mut self, cb_type: UtpCallbackType, cb: utp_callback_t) {
        unsafe { utp_set_callback(self.ctx, cb_type as i32, cb) }
    }
}

impl Drop for UtpContext {
    fn drop(&mut self) {
        unsafe { utp_destroy(self.ctx) }
    }
}

/// Gives a more Rust'ish interface to callback arguments. Each libutp callback receives this
/// structure.
struct UtpCallbackArgs {
    inner: *mut utp_callback_arguments,
}

impl UtpCallbackArgs {
    /// Wraps libutp callback arguments to a more Rust'ish interface.
    pub fn wrap(inner: *mut utp_callback_arguments) -> Self {
        Self { inner }
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
            std::mem::transmute(state)
        }
    }

    /// Returns immutable slice to the buffer used for a specific callback, say `on_read`.
    pub fn buf(&self) -> &[u8] {
        unsafe {
            let buf = (*self.inner).buf;
            let buf_len = (*self.inner).len;
            std::slice::from_raw_parts(buf, buf_len)
        }
    }
}

unsafe extern "C" fn callback_on_read(_arg1: *mut utp_callback_arguments) -> uint64 {
    println!("read something");
    0
}

unsafe extern "C" fn callback_sendto(_arg1: *mut utp_callback_arguments) -> uint64 {
    let sock: &UdpSocket =
        unsafe { &*(utp_context_get_userdata((*_arg1).context) as *const UdpSocket) };

    let sockaddr_in = (*_arg1).args1.address;
    let sockaddr = SockAddr::from_libc_sockaddr(sockaddr_in).unwrap();

    let args = UtpCallbackArgs::wrap(_arg1);
    println!("sendto: {:?}", args.address());

    if let SockAddr::Inet(inet) = sockaddr {
        sock.send_to(args.buf(), inet.to_std());
    }

    0
}

unsafe extern "C" fn callback_on_error(_arg1: *mut utp_callback_arguments) -> uint64 {
    println!("error");
    0
}

unsafe extern "C" fn callback_on_accept(_arg1: *mut utp_callback_arguments) -> uint64 {
    let args = UtpCallbackArgs::wrap(_arg1);
    println!("on_accept: {:?}", args.address());
    0
}

unsafe extern "C" fn callback_on_state_change(_arg1: *mut utp_callback_arguments) -> uint64 {
    let args = UtpCallbackArgs::wrap(_arg1);
    println!("state: {:?}", args.state());
    0
}

/// During this call `buf` argument is 0 terminated string, `len` is undefined.
unsafe extern "C" fn callback_log(_arg1: *mut utp_callback_arguments) -> uint64 {
    let log = CStr::from_ptr((*_arg1).buf as *const c_char).to_string_lossy();
    println!("{}", log);
    0
}

fn client(utp: UtpContext) -> io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    unsafe { utp_context_set_userdata(utp.ctx, &socket as *const _ as *mut _) };

    let sock = unsafe { utp_create_socket(utp.ctx) };
    let dst_sockaddr = SockAddr::new_inet(InetAddr::from_std(&"127.0.0.1:34254".parse().unwrap()));
    let (sockaddr, socklen) = unsafe { dst_sockaddr.as_ffi_pair() };

    let res = unsafe { utp_connect(sock, sockaddr, socklen) };
    println!("conn_res {}", res);

    let buf = vec![1, 2, 3];
    let res = unsafe { utp_write(sock, buf.as_ptr() as *mut _, buf.len()) };
    println!("res {}", res);

    Ok(())
}

fn server(mut utp: UtpContext) -> io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:34254")?;

    unsafe { utp_context_set_userdata(utp.ctx, &socket as *const _ as *mut _) };
    utp.set_callback(UtpCallbackType::OnAccept, Some(callback_on_accept));

    loop {
        let mut buf = [0; 100];
        let (amt, src) = socket.recv_from(&mut buf)?;

        let src_sockaddr = SockAddr::new_inet(InetAddr::from_std(&src));
        let (sockaddr, socklen) = unsafe { src_sockaddr.as_ffi_pair() };

        let res = unsafe { utp_process_udp(utp.ctx, buf.as_ptr(), amt, sockaddr, socklen) };
        println!("{}", res);
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let mut utp = UtpContext::new();

    utp.context_set_option(UTP_LOG_DEBUG, 1);

    utp.set_callback(UtpCallbackType::Log, Some(callback_log));
    utp.set_callback(UtpCallbackType::OnError, Some(callback_on_error));
    utp.set_callback(UtpCallbackType::OnRead, Some(callback_on_read));
    utp.set_callback(UtpCallbackType::Sendto, Some(callback_sendto));
    utp.set_callback(
        UtpCallbackType::OnStateChange,
        Some(callback_on_state_change),
    );

    if env::args().collect::<Vec<_>>().len() > 1 {
        server(utp)?;
    } else {
        client(utp)?;
    }

    Ok(())
}
