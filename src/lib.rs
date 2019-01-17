//! This crate provides safe libutp Rust bindings.

#![forbid(
    exceeding_bitshifts,
    mutable_transmutes,
    no_mangle_const_items,
    unknown_crate_types,
    warnings
)]
#![deny(
    deprecated,
    improper_ctypes,
    missing_docs,
    non_shorthand_field_patterns,
    overflowing_literals,
    plugin_as_library,
    stable_features,
    unconditional_recursion,
    unknown_lints,
    unsafe_code,
    unused,
    unused_allocation,
    unused_attributes,
    unused_comparisons,
    unused_features,
    unused_parens,
    while_true
)]
#![warn(
    trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_results
)]
#![allow(
    box_pointers,
    missing_copy_implementations,
    missing_debug_implementations,
    variant_size_differences
)]

#[macro_use]
extern crate quick_error;

mod callback;
mod ctx;
mod error;
mod socket;

pub use crate::callback::{UtpCallback, UtpCallbackArgs, UtpCallbackType};
pub use crate::ctx::UtpContext;
pub use crate::error::UtpError;
pub use crate::socket::UtpSocket;

use libutp_sys::*;

/// uTP connection state
#[derive(Debug, PartialEq)]
#[repr(u32)]
pub enum UtpState {
    /// socket has reveived syn-ack (notification only for outgoing connection completion)
    /// this implies writability
    Connected = UTP_STATE_CONNECT,

    /// socket is able to send more data
    Writable = UTP_STATE_WRITABLE,

    /// connection closed
    ConnectionClosed = UTP_STATE_EOF,

    /// socket is being destroyed, meaning all data has been sent if possible.
    /// it is not valid to refer to the socket after this state change occurs
    Destroying = UTP_STATE_DESTROYING,
}

// TODO(povilas): wrap utp context options:
//
// UTP_LOG_NORMAL,
// UTP_LOG_MTU,
// UTP_LOG_DEBUG,
// UTP_SNDBUF,
// UTP_RCVBUF,
// UTP_TARGET_DELAY,
