#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

extern crate libc;
extern crate nix;

use nix::sys::socket::{sockaddr, sockaddr_in, sockaddr_in6, sockaddr_storage, InetAddr, SockAddr};
use std::net::UdpSocket;
use std::{env, io, mem};

struct UtpContext {
    ctx: *mut utp_context,
}

impl UtpContext {
    fn new() -> Self {
        let ctx = unsafe { utp_init(2) };

        UtpContext { ctx }
    }
}

impl Drop for UtpContext {
    fn drop(&mut self) {
        unsafe { utp_destroy(self.ctx) }
    }
}

fn main() -> io::Result<()> {
    let utp = UtpContext::new();

    if env::args().collect::<Vec<_>>().len() > 1 {
        let socket = UdpSocket::bind("127.0.0.1:34254")?;

        let mut buf = [0; 100];
        let (amt, src) = socket.recv_from(&mut buf)?;

        let src_sockaddr = SockAddr::new_inet(InetAddr::from_std(&src));
        let (sockaddr, socklen) = unsafe { src_sockaddr.as_ffi_pair() };

        let res = unsafe { utp_process_udp(utp.ctx, buf.as_ptr(), buf.len(), sockaddr, socklen) };
        println!("{}", res);
    } else {
        let socket = UdpSocket::bind("127.0.0.1:0")?;

        socket
            .send_to(&[0; 10], "127.0.0.1:34254")
            .expect("couldn't send data");
    }

    Ok(())
}
