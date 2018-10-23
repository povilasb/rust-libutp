//! uTP errors

// NOTE, this code must not be in the same module that imports utp_sys, otherwise bindgen produced
// code somehow conflicts with quick_error.

// TODO(povilas): split this error into multiple: SendError, etc.
// Otherwise we have to handle all irrelevant cases, e.g. when `UtpSocket::send()` is called
// `ConnectFailed` is impossible.
quick_error! {
    /// Will cover all uTP errors.
    #[derive(Debug, PartialEq)]
    pub enum UtpError {
        /// Failure to write data to uTP socket. The reason is unknown because the underlying C library
        /// doesn't expose more info.
        SendFailed {
            display("Failed to send data over uTP socket")
        }
        /// Failure to connect with remote peer.
        ConnectFailed {
            display("Failed to establish uTP connection")
        }
        /// 0 bytes were writen to uTP socket which means that we should wait until the socket gets
        /// writable again.
        WouldBlock {
            display("uTP socket is not capable of accepting more outgoing data, try later")
        }
        /// Call to libutp returned the unexpected value which we can't interpret.
        UnexpectedResult(result: i64) {
            display("Unknown result from underlying libutp: {}", result)
        }
        /// Given UDP packet was illegal uTP packet.
        IllegalPacket {
            display("UDP packet was not legal uTP packet")
        }
    }
}
