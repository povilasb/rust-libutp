//! This uTP client example sends what you enter to stdin to the server.

extern crate utp;
#[macro_use]
extern crate net_literals;

use std::io::{self, Write};
use std::net::UdpSocket;
use std::thread;
use utp::{UtpCallbackType, UtpContext};

fn send_messages(mut utp: UtpContext<UdpSocket>) -> io::Result<()> {
    let sock = utp
        .connect(addr!("127.0.0.1:1234"))
        .expect("Failed to make uTP connection");

    loop {
        print!("\r> ");
        io::stdout().flush().unwrap();
        let msg = readln()?;

        let res = sock.send(&msg.into_bytes()[..]);
        println!("[send result] {}", res);
    }
}

fn main() -> io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let utp = make_utp_ctx(socket);
    send_messages(utp)?;
    Ok(())
}

fn make_utp_ctx(socket: UdpSocket) -> UtpContext<UdpSocket> {
    let mut utp: UtpContext<UdpSocket> = UtpContext::new(socket);
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
    utp.set_callback(
        UtpCallbackType::OnStateChange,
        Box::new(|args| {
            println!("state: {:?}", args.state());
            0
        }),
    );
    utp
}

fn readln() -> io::Result<String> {
    let mut ln = String::new();
    io::stdin().read_line(&mut ln)?;
    Ok(String::from(ln.trim()))
}
