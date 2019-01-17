//! This example demonstrate how to use multiple uTP connections over single UDP socket

extern crate utp;
#[macro_use]
extern crate net_literals;
#[macro_use]
extern crate unwrap;

use std::io;
use std::net::UdpSocket;
use std::rc::Rc;
use utp::{UtpCallbackType, UtpContext, UtpState};

fn main() -> io::Result<()> {
    let udp_socket = Rc::new(UdpSocket::bind("0.0.0.0:0")?);
    let mut utp = make_utp_ctx(Rc::clone(&udp_socket));
    let _utp_socket1 = unwrap!(utp.connect(addr!("127.0.0.1:4000")));
    let _utp_socket2 = unwrap!(utp.connect(addr!("127.0.0.1:5000")));

    // Keep parsing incomig UDP datagrams
    let mut buf = vec![0; 4096];
    loop {
        let (bytes_received, client_addr) = unwrap!(udp_socket.recv_from(&mut buf));
        unwrap!(utp.process_udp(&buf[..bytes_received], client_addr));
    }
}

fn make_utp_ctx(socket: Rc<UdpSocket>) -> UtpContext<Rc<UdpSocket>> {
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
        UtpCallbackType::Sendto,
        Box::new(|args| {
            if let Some(addr) = args.address() {
                println!("send_to: {}", addr);
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
            if let UtpState::Connected = args.state() {
                unwrap!(args.socket().send(b"hello"));
            }
            0
        }),
    );
    utp
}
