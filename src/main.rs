extern crate libc;
extern crate nix;
#[macro_use]
extern crate net_literals;

mod utp;

use std::net::UdpSocket;
use std::{env, io};
use utp::{UtpCallbackArgs, UtpCallbackType, UtpContext};

fn client(mut utp: UtpContext<Option<UdpSocket>>) -> io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    *utp.user_data_mut() = Some(socket);

    let sock = utp
        .connect(addr!("127.0.0.1:34254"))
        .expect("Failed to make uTP connection");
    let res = sock.send(&vec![1, 2, 3]);
    println!("client send: {}", res);

    Ok(())
}

fn server(mut utp: UtpContext<Option<UdpSocket>>) -> io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:34254")?;
    *utp.user_data_mut() = Some(socket);

    utp.set_callback(
        UtpCallbackType::OnAccept,
        Box::new(|args: UtpCallbackArgs<Option<UdpSocket>>| {
            println!("on_accept: {:?}", args.address());
            0
        }),
    );

    let socket = utp.user_data().as_ref().unwrap();
    loop {
        let mut buf = [0; 100];
        let (bytes_received, sender_addr) = socket.recv_from(&mut buf)?;
        let res = utp.process_udp(&buf[..bytes_received], sender_addr);
        println!("utp_process: {}", res);
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let mut utp: UtpContext<Option<UdpSocket>> = UtpContext::new(None);
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
        UtpCallbackType::Sendto,
        Box::new(|args| {
            println!("sendto: {:?}", args.address());
            if let Some(addr) = args.address() {
                if let Some(ref sock) = args.user_data() {
                    sock.send_to(args.buf(), addr).unwrap();
                }
            }
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

    if env::args().collect::<Vec<_>>().len() > 1 {
        server(utp)?;
    } else {
        client(utp)?;
    }

    Ok(())
}
