//! This uTP client example sends what you enter to stdin to the server.

extern crate utp;
#[macro_use]
extern crate net_literals;

use std::io::{self, Write};
use std::net::UdpSocket;
use utp::{UtpCallbackArgs, UtpCallbackType, UtpContext};

fn send_messages(mut utp: UtpContext<Option<UdpSocket>>) -> io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    *utp.user_data_mut() = Some(socket);

    loop {
        print!("\r> ");
        io::stdout().flush().unwrap();
        let msg = readln()?;

        let sock = utp
            .connect(addr!("127.0.0.1:1234"))
            .expect("Failed to make uTP connection");
        let res = sock.send(&msg.into_bytes()[..]);
        println!("[send result] {}", res);
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

    send_messages(utp)?;
    Ok(())
}

fn readln() -> io::Result<String> {
    let mut ln = String::new();
    io::stdin().read_line(&mut ln)?;
    Ok(String::from(ln.trim()))
}
