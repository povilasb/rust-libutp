//! This uTP server example simply receives data from anybody and prints it to the stdout.

extern crate utp;

use std::io;
use std::net::UdpSocket;
use utp::{UtpCallbackArgs, UtpCallbackType, UtpContext};

fn handle_connections(mut utp: UtpContext<Option<UdpSocket>>) -> io::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:1234")?;
    *utp.user_data_mut() = Some(socket);

    let socket = utp.user_data().as_ref().unwrap();
    loop {
        let mut buf = [0; 4096];
        let (bytes_received, sender_addr) = socket.recv_from(&mut buf)?;
        let res = utp.process_udp(&buf[..bytes_received], sender_addr);
        println!("[utp_process]: {}", res);
    }
}

fn main() -> io::Result<()> {
    let mut utp: UtpContext<Option<UdpSocket>> = UtpContext::new(None);
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
        Box::new(|args: UtpCallbackArgs<Option<UdpSocket>>| {
            println!("on_accept: {:?}", args.address());
            0
        }),
    );

    handle_connections(utp)?;
    Ok(())
}
