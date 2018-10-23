//! This uTP client example sends what you enter to stdin to the server.

extern crate utp;
#[macro_use]
extern crate net_literals;
#[macro_use]
extern crate unwrap;

use std::io::{self, Write};
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use utp::{UtpCallbackType, UtpContext};

/// Our app event.
#[derive(Debug)]
enum AppEvent {
    /// Read a line from stdin.
    Stdin(String),
    /// Received UDP packet.
    UdpPacket((Vec<u8>, SocketAddr)),
}

fn main() -> io::Result<()> {
    println!("Type messages and hit ENTER to send to the server. Type '/q' to exit.");
    let udp_socket = Arc::new(UdpSocket::bind("0.0.0.0:0")?);
    let utp = make_utp_ctx(Arc::clone(&udp_socket));

    let (events_tx, events_rx) = mpsc::channel();

    spawn_stdin_reader(events_tx.clone());
    spawn_udp_reader(udp_socket, events_tx);
    run_evloop(events_rx, utp);

    Ok(())
}

fn run_evloop(events_rx: mpsc::Receiver<AppEvent>, mut utp: UtpContext<Arc<UdpSocket>>) {
    let utp_socket = utp
        .connect(addr!("127.0.0.1:1234"))
        .expect("Failed to make uTP connection");

    loop {
        let event = unwrap!(events_rx.recv());
        match event {
            AppEvent::UdpPacket((packet, sender_addr)) => {
                unwrap!(utp.process_udp(&packet[..], sender_addr));
            }
            AppEvent::Stdin(line) => {
                if line == "/q".to_owned() {
                    break;
                }
                let _ = unwrap!(utp_socket.send(&line.into_bytes()[..]));
            }
        }
    }
}

fn spawn_stdin_reader(events_tx: mpsc::Sender<AppEvent>) {
    thread::spawn(move || loop {
        print!("\r> ");
        unwrap!(io::stdout().flush());
        let msg = unwrap!(readln());
        unwrap!(events_tx.send(AppEvent::Stdin(msg)));
    });
}

fn spawn_udp_reader(socket: Arc<UdpSocket>, events_tx: mpsc::Sender<AppEvent>) {
    thread::spawn(move || {
        let mut buf = vec![0; 4096];
        loop {
            let (bytes_received, client_addr) = unwrap!(socket.recv_from(&mut buf));
            unwrap!(events_tx.send(AppEvent::UdpPacket((
                buf[..bytes_received].to_vec(),
                client_addr
            ))));
        }
    });
}

fn make_utp_ctx(socket: Arc<UdpSocket>) -> UtpContext<Arc<UdpSocket>> {
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
