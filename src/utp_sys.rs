#![allow(non_camel_case_types, non_upper_case_globals, unused, unsafe_code)]
#![cfg_attr(feature = "cargo-clippy", allow(decimal_literal_representation, unreadable_literal,
    trivially_copy_pass_by_ref, const_static_lifetime, useless_transmute))]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use libc;
use nix::sys::socket::{sockaddr, sockaddr_in, sockaddr_in6, sockaddr_storage, InetAddr, SockAddr};
