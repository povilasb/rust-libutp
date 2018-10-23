extern crate utp;
#[macro_use]
extern crate net_literals;
#[macro_use]
extern crate unwrap;
extern crate mio;
extern crate mio_extras;
extern crate rand;

use mio::net::UdpSocket;
use mio::{Events, Poll, PollOpt, Ready, Token};
use mio_extras::channel::{channel as async_channel, Sender as AsyncSender};
use mio_extras::timer::Timer;
use rand::RngCore;
use std::sync::Arc;
use std::time::Duration;
use utp::{UtpCallbackType, UtpContext, UtpError, UtpState};

mod connect {
    use super::*;

    #[test]
    fn client_receives_connected_state() {
        const SERVER_SOCKET_TOKEN: Token = Token(0);
        const CLIENT_SOCKET_TOKEN: Token = Token(1);
        const CONNECTED_RX_TOKEN: Token = Token(2);
        let (connected_tx, connected_rx) = async_channel();

        let server_udp_socket = Arc::new(unwrap!(UdpSocket::bind(&addr!("127.0.0.1:0"))));
        let server_addr = unwrap!(server_udp_socket.local_addr());
        let server_utp = make_utp_ctx(Arc::clone(&server_udp_socket), None, None);

        let client_udp_socket = Arc::new(unwrap!(UdpSocket::bind(&addr!("0.0.0.0:0"))));
        let mut client_utp = make_utp_ctx(Arc::clone(&client_udp_socket), Some(connected_tx), None);
        let _client_utp_socket = unwrap!(client_utp.connect(server_addr));

        let evloop = unwrap!(Poll::new());
        unwrap!(evloop.register(
            &server_udp_socket,
            SERVER_SOCKET_TOKEN,
            Ready::readable(),
            PollOpt::level(),
        ));
        unwrap!(evloop.register(
            &client_udp_socket,
            CLIENT_SOCKET_TOKEN,
            Ready::readable(),
            PollOpt::level(),
        ));
        unwrap!(evloop.register(
            &connected_rx,
            CONNECTED_RX_TOKEN,
            Ready::readable(),
            PollOpt::level(),
        ));

        let mut events = Events::with_capacity(16);
        'main_loop: loop {
            unwrap!(evloop.poll(&mut events, None));
            for ev in events.iter() {
                match ev.token() {
                    SERVER_SOCKET_TOKEN => handle_udp_packet(&server_udp_socket, &server_utp),
                    CLIENT_SOCKET_TOKEN => handle_udp_packet(&client_udp_socket, &client_utp),
                    CONNECTED_RX_TOKEN => break 'main_loop, // that's what we were waiting for
                    _ => panic!("Unexpected event"),
                }
            }
        }
    }

    #[test]
    fn two_clients_issueing_connect_are_able_to_connect_with_each_other() {
        const CLIENT1_SOCKET_TOKEN: Token = Token(0);
        const CLIENT2_SOCKET_TOKEN: Token = Token(1);
        const CONNECTED_RX_TOKEN: Token = Token(2);
        let (connected_tx, connected_rx) = async_channel();

        let udp_socket1 = Arc::new(unwrap!(UdpSocket::bind(&addr!("127.0.0.1:0"))));
        let addr1 = unwrap!(udp_socket1.local_addr());
        let mut utp1 = make_utp_ctx(Arc::clone(&udp_socket1), Some(connected_tx.clone()), None);

        let udp_socket2 = Arc::new(unwrap!(UdpSocket::bind(&addr!("127.0.0.1:0"))));
        let addr2 = unwrap!(udp_socket2.local_addr());
        let mut utp2 = make_utp_ctx(Arc::clone(&udp_socket2), Some(connected_tx), None);

        let _utp_socket1 = unwrap!(utp1.connect(addr2));
        let _utp_socket2 = unwrap!(utp2.connect(addr1));

        let evloop = unwrap!(Poll::new());
        unwrap!(evloop.register(
            &udp_socket1,
            CLIENT1_SOCKET_TOKEN,
            Ready::readable(),
            PollOpt::level(),
        ));
        unwrap!(evloop.register(
            &udp_socket2,
            CLIENT2_SOCKET_TOKEN,
            Ready::readable(),
            PollOpt::level(),
        ));
        unwrap!(evloop.register(
            &connected_rx,
            CONNECTED_RX_TOKEN,
            Ready::readable(),
            PollOpt::level(),
        ));

        let mut established_conns = 0;

        let mut events = Events::with_capacity(16);
        while established_conns < 2 {
            unwrap!(evloop.poll(&mut events, None));
            for ev in events.iter() {
                match ev.token() {
                    CLIENT1_SOCKET_TOKEN => handle_udp_packet(&udp_socket1, &utp1),
                    CLIENT2_SOCKET_TOKEN => handle_udp_packet(&udp_socket2, &utp2),
                    CONNECTED_RX_TOKEN => established_conns += 1,
                    _ => panic!("Unexpected event"),
                }
            }
        }
    }
}

fn exchange_data(byte_count: usize) {
    const SERVER_SOCKET_TOKEN: Token = Token(0);
    const CLIENT_SOCKET_TOKEN: Token = Token(1);
    const CLIENT_WRITABLE_RX_TOKEN: Token = Token(2);
    const DATA_RX_TOKEN: Token = Token(3);
    const TIMEOUT_TOKEN: Token = Token(4);
    let (writable_tx, writable_rx) = async_channel();
    let (received_data_tx, received_data_rx) = async_channel();

    let server_udp_socket = Arc::new(unwrap!(UdpSocket::bind(&addr!("127.0.0.1:0"))));
    let server_addr = unwrap!(server_udp_socket.local_addr());
    let mut server_utp = make_utp_ctx(Arc::clone(&server_udp_socket), None, Some(received_data_tx));

    let client_udp_socket = Arc::new(unwrap!(UdpSocket::bind(&addr!("0.0.0.0:0"))));
    let mut client_utp = make_utp_ctx(Arc::clone(&client_udp_socket), Some(writable_tx), None);
    let client_utp_socket = unwrap!(client_utp.connect(server_addr));

    let mut timer = Timer::default();
    timer.set_timeout(Duration::from_millis(500), ());

    let evloop = unwrap!(Poll::new());
    unwrap!(evloop.register(
        &server_udp_socket,
        SERVER_SOCKET_TOKEN,
        Ready::readable(),
        PollOpt::level(),
    ));
    unwrap!(evloop.register(
        &client_udp_socket,
        CLIENT_SOCKET_TOKEN,
        Ready::readable(),
        PollOpt::level(),
    ));
    unwrap!(evloop.register(
        &writable_rx,
        CLIENT_WRITABLE_RX_TOKEN,
        Ready::readable(),
        PollOpt::level(),
    ));
    unwrap!(evloop.register(
        &received_data_rx,
        DATA_RX_TOKEN,
        Ready::readable(),
        PollOpt::level(),
    ));
    unwrap!(evloop.register(&timer, TIMEOUT_TOKEN, Ready::readable(), PollOpt::edge(),));

    let out_data = random_vec(byte_count);
    let mut bytes_sent = 0;
    let mut in_data = Vec::with_capacity(out_data.len());

    let mut events = Events::with_capacity(16);
    'main_loop: loop {
        unwrap!(evloop.poll(&mut events, None));
        for ev in events.iter() {
            match ev.token() {
                SERVER_SOCKET_TOKEN => handle_udp_packet(&server_udp_socket, &server_utp),
                CLIENT_SOCKET_TOKEN => handle_udp_packet(&client_udp_socket, &client_utp),
                CLIENT_WRITABLE_RX_TOKEN => {
                    unwrap!(writable_rx.try_recv());
                    match client_utp_socket.send(&out_data[bytes_sent..]) {
                        Ok(count) => bytes_sent += count,
                        Err(UtpError::WouldBlock) => {}
                        e => panic!("UtpSocket::send() failed: {:?}", e),
                    }
                }
                DATA_RX_TOKEN => {
                    let buf = unwrap!(received_data_rx.try_recv());
                    in_data.extend_from_slice(&buf[..]);
                    if in_data.len() == out_data.len() {
                        assert_eq!(&in_data[..], &out_data[..]);
                        break 'main_loop;
                    }
                }
                TIMEOUT_TOKEN => {
                    client_utp.check_timeouts();
                    server_utp.check_timeouts();
                    timer.set_timeout(Duration::from_millis(500), ());
                }
                _ => panic!("Unexpected event"),
            }
        }
    }
}

#[test]
fn transfer_data_eth_mtu_size() {
    exchange_data(1350);
}

#[test]
fn transfer_data_over_eth_mtu_size() {
    exchange_data(4300);
}

#[test]
fn transfer_data_over_udp_datagram_size() {
    exchange_data(1024 * 1024 * 2); // 2 MB
}

fn handle_udp_packet<T>(sock: &UdpSocket, utp: &UtpContext<T>) {
    // NOTE, if `buf.len()` will be smaller than the packet sent, the rest data will be discarded.
    // Anyway, that shouldn't happen since libutp sends datagrams of ~1400 bytes.
    let mut buf = vec![0; 4096];
    let (bytes_read, sender_addr) = unwrap!(sock.recv_from(&mut buf));
    unwrap!(utp.process_udp(&buf[..bytes_read], sender_addr));
    utp.ack_packets();
}

fn make_utp_ctx(
    socket: Arc<UdpSocket>,
    connected_tx: Option<AsyncSender<()>>,
    received_data_tx: Option<AsyncSender<Vec<u8>>>,
) -> UtpContext<Arc<UdpSocket>> {
    let mut utp = UtpContext::new(socket);
    utp.set_callback(UtpCallbackType::OnError, Box::new(|_| panic!("uTP error")));
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
    utp.set_callback(
        UtpCallbackType::OnRead,
        Box::new(move |mut args| {
            if let Some(ref tx) = received_data_tx {
                unwrap!(tx.send(args.buf().to_vec()));
            }
            args.ack_data();
            0
        }),
    );
    utp.set_callback(
        UtpCallbackType::OnStateChange,
        Box::new(move |args| {
            if args.state() == UtpState::Connected || args.state() == UtpState::Writable {
                if let Some(ref tx) = connected_tx {
                    unwrap!(tx.send(()));
                }
            }
            0
        }),
    );
    utp
}

#[allow(unsafe_code)]
pub fn random_vec(size: usize) -> Vec<u8> {
    let mut ret = Vec::with_capacity(size);
    unsafe { ret.set_len(size) };
    rand::thread_rng().fill_bytes(&mut ret[..]);
    ret
}
