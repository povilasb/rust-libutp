//! This example is just like ucat from libutp: it can operate as a server or a client.
//! The client can read input data from pipe and will forward it over uTP stream.

extern crate clap;
extern crate env_logger;
extern crate utp;
#[macro_use]
extern crate unwrap;
#[macro_use]
extern crate log;
extern crate mio;
extern crate mio_extras;
extern crate nix;
#[macro_use]
extern crate net_literals;

use clap::{App, Arg};
use mio::net::UdpSocket;
use mio::unix::EventedFd;
use mio::{Events, Poll, PollOpt, Ready, Token};
use mio_extras::channel::{
    channel as async_channel, Receiver as AsyncReceiver, Sender as AsyncSender,
};
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::os::unix::io::RawFd;
use std::sync::Arc;
use utp::{UtpCallbackArgs, UtpCallbackType, UtpContext, UtpError, UtpSocket, UtpState};

#[derive(Debug)]
struct CliArgs {
    listen_mode: bool,
    port: Option<u16>,
    buffer_size: usize,
    target_addr: Option<SocketAddr>,
}

fn main() -> io::Result<()> {
    env_logger::init();

    let args = match parse_cli_args() {
        Ok(args) => args,
        Err(e) => e.exit(),
    };

    if let Some(listen_port) = args.port {
        run_server(listen_port, args.buffer_size)?;
    } else {
        let mut client = UtpClient::new(args.buffer_size)?;
        client.run(unwrap!(args.target_addr))?;
    }

    Ok(())
}

struct MioEventHandlers {
    connected_rx: AsyncReceiver<()>,
    utp_writable_rx: AsyncReceiver<()>,
}

struct ClientData {
    udp_socket: Arc<UdpSocket>,
    connected_tx: AsyncSender<()>,
    utp_writable_tx: AsyncSender<()>,
}

impl ClientData {
    fn new(udp_socket: Arc<UdpSocket>) -> (Self, MioEventHandlers) {
        let (connected_tx, connected_rx) = async_channel();
        let (utp_writable_tx, utp_writable_rx) = async_channel();
        (
            Self {
                udp_socket,
                connected_tx,
                utp_writable_tx,
            },
            MioEventHandlers {
                connected_rx,
                utp_writable_rx,
            },
        )
    }
}

/// mio event tokens
const STDIN_TOKEN: Token = Token(0);
const SOCKET_TOKEN: Token = Token(1);
const CONNECTED_RX_TOKEN: Token = Token(2);
const UTP_WRITABLE_TOKEN: Token = Token(3);

struct UtpClient {
    evloop: Poll,
    udp_socket: Arc<UdpSocket>,
    utp: UtpContext<ClientData>,
    event_handlers: MioEventHandlers,
    buf: Vec<u8>,
    buf_bytes_sent: usize,
    buf_bytes_read: usize,
    evented_stdin: EventedFd<'static>,
}

impl UtpClient {
    fn new(buffer_size: usize) -> io::Result<Self> {
        let evloop = Poll::new()?;

        let udp_socket = Arc::new(UdpSocket::bind(&addr!("0.0.0.0:0"))?);
        let (client_data, event_handlers) = ClientData::new(Arc::clone(&udp_socket));
        let utp = make_client_utp_ctx(client_data);

        let mut buf: Vec<u8> = Vec::with_capacity(buffer_size);
        unsafe { buf.set_len(buffer_size) }

        const STDIN_FD: RawFd = 0;
        let evented_stdin = EventedFd(&STDIN_FD);

        Ok(Self {
            evloop,
            udp_socket,
            utp,
            event_handlers,
            buf,
            buf_bytes_read: 0,
            buf_bytes_sent: 0,
            evented_stdin,
        })
    }

    /// Runs main event loop.
    fn run(&mut self, server_addr: SocketAddr) -> io::Result<()> {
        self.register_events()?;
        let utp_socket = unwrap!(self.utp.connect(server_addr));

        let mut events = Events::with_capacity(1024);
        loop {
            self.evloop.poll(&mut events, None)?;
            for ev in events.iter() {
                self.handle_event(ev, &utp_socket)?;
            }
            // TODO(povilas): do this every 500 ms V
            self.utp.check_timeouts();
        }
    }

    fn on_stdin(&mut self, utp_socket: &UtpSocket) {
        // read more data into input buffer beyond where it was read last time
        let bytes_read = unwrap!(nix::unistd::read(0, &mut self.buf[self.buf_bytes_read..]));
        if bytes_read > 0 {
            self.buf_bytes_read += bytes_read;
        } else if bytes_read == 0 && self.buf_bytes_read < self.buf.len() {
            println!("stdin EOF");
            unwrap!(self.evloop.deregister(&self.evented_stdin));
        }
        self.flush_input_buffer(utp_socket);
    }

    fn handle_event(&mut self, event: mio::Event, utp_socket: &UtpSocket) -> io::Result<()> {
        // we don't need big socket buffer, cause client doesn't expect data packets
        let mut sock_buf = [0; 4096];

        match event.token() {
            UTP_WRITABLE_TOKEN => {
                unwrap!(self.event_handlers.utp_writable_rx.try_recv());
                self.flush_input_buffer(&utp_socket);
            }
            SOCKET_TOKEN => handle_udp(&self.udp_socket, &mut sock_buf[..], &self.utp)?,
            CONNECTED_RX_TOKEN => {
                // We're only interested in stdin, when we are connected with the server
                self.evloop.register(
                    &self.evented_stdin,
                    STDIN_TOKEN,
                    Ready::readable(),
                    PollOpt::level(),
                )?;
            }
            STDIN_TOKEN => self.on_stdin(utp_socket),
            _ => panic!("Unexpected mio token polled"),
        }

        Ok(())
    }

    fn register_events(&self) -> io::Result<()> {
        self.evloop.register(
            &self.udp_socket,
            SOCKET_TOKEN,
            Ready::readable(), // I just assume that UDP socket is always writable
            PollOpt::edge(),
        )?;
        self.evloop.register(
            &self.event_handlers.connected_rx,
            CONNECTED_RX_TOKEN,
            Ready::readable(),
            PollOpt::edge(),
        )?;
        self.evloop.register(
            &self.event_handlers.utp_writable_rx,
            UTP_WRITABLE_TOKEN,
            Ready::readable(),
            PollOpt::level(),
        )?;
        Ok(())
    }

    fn flush_input_buffer(&mut self, utp_socket: &UtpSocket) {
        loop {
            match utp_socket.send(&self.buf[self.buf_bytes_sent..self.buf_bytes_read]) {
                Ok(bytes_sent) => self.buf_bytes_sent += bytes_sent,
                Err(UtpError::WouldBlock) => break,
                Err(UtpError::SendFailed) => panic!("uTP socket send failed"),
                Err(UtpError::UnexpectedResult(res)) => {
                    panic!("Unknown send return value: {}", res)
                }
            }
        }
        if self.buf_bytes_sent == self.buf_bytes_read {
            // resent the counters because everything has been flushed
            self.buf_bytes_sent = 0;
            self.buf_bytes_read = 0;
        }
    }
}

fn make_client_utp_ctx(data: ClientData) -> UtpContext<ClientData> {
    let mut utp = make_utp_ctx(data);
    utp.set_callback(
        UtpCallbackType::Sendto,
        Box::new(|args| {
            let addr = unwrap!(args.address());
            let client_data = args.user_data();
            client_data.udp_socket.send_to(args.buf(), &addr).unwrap();
            0
        }),
    );
    utp.set_callback(
        UtpCallbackType::OnStateChange,
        Box::new(|args| {
            debug!("state: {:?}", args.state());
            match args.state() {
                UtpState::Connected => {
                    unwrap!(args.user_data().connected_tx.send(()));
                }
                UtpState::Writable => {
                    unwrap!(args.user_data().utp_writable_tx.send(()));
                }
                _ => {}
            }
            0
        }),
    );
    utp
}

/// Runs the server that receives uTP packets and prints them to stdout.
fn run_server(listen_port: u16, buffer_size: usize) -> io::Result<()> {
    let socket = UdpSocket::bind(&SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::new(0, 0, 0, 0),
        listen_port,
    )))?;
    let utp = make_server_utp_ctx(socket); // UDP socket must be accessible from uTP callbacks
    let socket = utp.user_data();
    let mut buf: Vec<u8> = Vec::with_capacity(buffer_size);
    unsafe { buf.set_len(buffer_size) }

    let evloop = Poll::new()?;
    evloop.register(
        socket,
        SOCKET_TOKEN,
        Ready::readable(), // I just assume that UDP socket is always writable
        PollOpt::edge(),
    )?;

    let mut events = Events::with_capacity(1024);
    loop {
        evloop.poll(&mut events, None)?;
        for ev in events.iter() {
            match ev.token() {
                SOCKET_TOKEN => handle_udp(&socket, &mut buf[..], &utp)?,
                _ => panic!("Unexpected mio token polled"),
            }
        }
    }
}

fn handle_udp<T>(socket: &UdpSocket, buf: &mut [u8], utp: &UtpContext<T>) -> io::Result<()> {
    loop {
        match socket.recv_from(buf) {
            Ok((bytes_read, sender_addr)) => {
                let res = utp.process_udp(&buf[..bytes_read], sender_addr);
                assert_eq!(res, 1);
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    utp.ack_packets();
                    break;
                } else {
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

fn make_server_utp_ctx(data: UdpSocket) -> UtpContext<UdpSocket> {
    let mut utp = make_utp_ctx(data);

    utp.set_callback(
        UtpCallbackType::OnRead,
        Box::new(move |mut args| {
            // TODO(povilas): use some wrapper for file descriptor
            unwrap!(nix::unistd::write(1, args.buf()));
            args.ack_data();
            0
        }),
    );
    utp.set_callback(
        UtpCallbackType::OnAccept,
        Box::new(|args: UtpCallbackArgs<UdpSocket>| {
            info!("new connection: {:?}", args.address());
            0
        }),
    );
    utp.set_callback(
        UtpCallbackType::Sendto,
        Box::new(|args| {
            if let Some(addr) = args.address() {
                let sock = args.user_data();
                sock.send_to(args.buf(), &addr).unwrap();
            }
            0
        }),
    );
    utp
}

fn make_utp_ctx<T>(data: T) -> UtpContext<T> {
    let mut utp = UtpContext::new(data);
    // utp.set_debug_log(true);
    utp.set_callback(
        UtpCallbackType::Log,
        Box::new(|args| {
            let log_msg = args.buf_as_string();
            trace!("{}", log_msg);
            0
        }),
    );
    utp.set_callback(
        UtpCallbackType::OnError,
        Box::new(|args| {
            error!("{}", args.buf_as_string());
            0
        }),
    );
    utp.set_callback(
        UtpCallbackType::OnStateChange,
        Box::new(|args| {
            debug!("state: {:?}", args.state());
            0
        }),
    );
    utp
}

fn parse_cli_args() -> Result<CliArgs, clap::Error> {
    let matches = App::new("uTP cat which sends stdin over uTP stream")
        .about("Send data from stdin over uTP stream.")
        .arg(Arg::with_name("listen_mode").short("l").help("Listen mode"))
        .arg(
            Arg::with_name("port")
                .short("p")
                .value_name("LISTEN_PORT")
                .help("Local port")
                .takes_value(true),
        ).arg(
            Arg::with_name("buffer_size")
                .short("B")
                .value_name("BUFFER_SIZE")
                .help(
                    "Buffer size for incoming data. Default is 65487 bytes - max uTP data length.",
                ).takes_value(true),
        ).arg(
            Arg::with_name("dst_ip")
                .value_name("DESTINATION_IP")
                .help("Destination IP. If specified ucat operates in client mode.")
                .takes_value(true),
        ).arg(
            Arg::with_name("dst_port")
                .value_name("DESTINATION_PORT")
                .help("Destination port")
                .takes_value(true),
        ).get_matches();

    let listen_mode = matches.is_present("listen_mode");
    let port = matches
        .value_of("port")
        .map(|port_str| port_str.parse::<u16>().expect("Invalid port number"));
    if listen_mode && port.is_none() {
        return Err(clap::Error::with_description(
            "You must specify port number in listen mode",
            clap::ErrorKind::MissingRequiredArgument,
        ));
    }
    let buffer_size = matches
        .value_of("buffer_size")
        .map(|size_str| size_str.parse::<usize>().expect("Invalid buffer size"))
        .unwrap_or(65487);

    let target_addr = if let Some(dst_ip) = matches.value_of("dst_ip") {
        if let Some(dst_port) = matches.value_of("dst_port") {
            let port = dst_port.parse::<u16>().expect("Invalid port number");
            let dst_addr = SocketAddr::V4(SocketAddrV4::new(unwrap!(dst_ip.parse()), port));
            Some(dst_addr)
        } else {
            return Err(clap::Error::with_description(
                "You must also pecify port number when destination IP is given.",
                clap::ErrorKind::MissingRequiredArgument,
            ));
        }
    } else {
        None
    };

    Ok(CliArgs {
        listen_mode,
        port,
        buffer_size,
        target_addr,
    })
}
