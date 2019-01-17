//! This uTP server example simply receives data from anybody and prints it to the stdout.
//!
//! The default listen port is 1234. In addition, you can specify port number as a first argument:
//!
//! ```
//! $ cargo run --example server -- 5000
//! ```

extern crate utp;
#[macro_use]
extern crate unwrap;

use std::env;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use utp::{UtpCallbackArgs, UtpCallbackType, UtpContext};

const DEFAULT_PORT: u16 = 1234;

fn handle_connections(utp: UtpContext<UdpSocket>) -> io::Result<()> {
    let socket = utp.user_data();
    loop {
        let mut buf = [0; 4096];
        let (bytes_received, sender_addr) = socket.recv_from(&mut buf)?;
        unwrap!(utp.process_udp(&buf[..bytes_received], sender_addr));
        utp.ack_packets();
    }
}

fn main() -> io::Result<()> {
    let port = get_port();
    let socket = UdpSocket::bind(ipv4_addr(port))?;
    let utp = make_utp_ctx(socket);
    println!("Listening for connections on port: {}", port);
    handle_connections(utp)?;
    Ok(())
}

fn get_port() -> u16 {
    match env::args().nth(1) {
        Some(arg1) => arg1.parse::<u16>().unwrap_or(DEFAULT_PORT),
        None => DEFAULT_PORT,
    }
}

fn make_utp_ctx(socket: UdpSocket) -> UtpContext<UdpSocket> {
    let mut utp = UtpContext::new(socket);
    // utp.set_debug_log(true);
    utp.set_callback(
        UtpCallbackType::Log,
        Box::new(|args| {
            let log_msg = args.buf_as_string();
            println!("{}", log_msg);
            0
        }),
    );
    utp.set_callback(
        UtpCallbackType::OnError,
        Box::new(|_| {
            println!("error");
            0
        }),
    );
    utp.set_callback(
        UtpCallbackType::OnRead,
        Box::new(|mut args| {
            let msg = unwrap!(String::from_utf8(args.buf().to_vec()));
            println!("received: {}", msg);
            args.ack_data();
            0
        }),
    );
    utp.set_callback(
        UtpCallbackType::OnStateChange,
        Box::new(|args| {
            println!("state: {:?}", args.state());
            0
        }),
    );
    utp.set_callback(
        UtpCallbackType::OnAccept,
        Box::new(|args: UtpCallbackArgs<UdpSocket>| {
            println!("on_accept: {:?}", args.address());
            0
        }),
    );
    utp.set_callback(
        UtpCallbackType::Sendto,
        Box::new(|args| {
            println!("sendto: {:?} {} bytes", args.address(), args.buf().len());
            if let Some(addr) = args.address() {
                let sock = args.user_data();
                sock.send_to(args.buf(), addr).unwrap();
            }
            0
        }),
    );
    utp
}

/// A convevience method to build IPv4 address with a port number.
pub fn ipv4_addr(port: u16) -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port))
}
