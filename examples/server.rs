//! This uTP server example simply receives data from anybody and prints it to the stdout.

extern crate utp;

use std::io;
use std::net::UdpSocket;
use utp::{UtpCallbackArgs, UtpCallbackType, UtpContext};

fn handle_connections(utp: UtpContext<UdpSocket>) -> io::Result<()> {
    let socket = utp.user_data();
    loop {
        let mut buf = [0; 4096];
        let (bytes_received, sender_addr) = socket.recv_from(&mut buf)?;
        let res = utp.process_udp(&buf[..bytes_received], sender_addr);
        println!("[utp_process]: {}", res);
    }
}

fn main() -> io::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:1234")?;
    let utp = make_utp_ctx(socket);
    handle_connections(utp)?;
    Ok(())
}

fn make_utp_ctx(socket: UdpSocket) -> UtpContext<UdpSocket> {
    let mut utp = UtpContext::new(socket);
    utp.set_debug_log(true);
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
        Box::new(|_| {
            println!("read something");
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
            println!("sendto: {:?}", args.address());
            if let Some(addr) = args.address() {
                let sock = args.user_data();
                sock.send_to(args.buf(), addr).unwrap();
            }
            0
        }),
    );
    utp
}
