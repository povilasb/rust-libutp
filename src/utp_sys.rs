#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unsafe_code, unused)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use nix::sys::socket::{sockaddr, sockaddr_in, sockaddr_in6, sockaddr_storage, InetAddr, SockAddr};
use libc;
