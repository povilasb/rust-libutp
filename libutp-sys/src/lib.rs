//! This crate provides unsafe libutp Rust bindings.

#![allow(non_camel_case_types, non_upper_case_globals)]
#![cfg_attr(
    feature = "cargo-clippy",
    allow(
        decimal_literal_representation,
        unreadable_literal,
        trivially_copy_pass_by_ref,
        const_static_lifetime,
        useless_transmute,
        new_without_default_derive
    )
)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

extern crate libc;
#[cfg(not(windows))]
extern crate nix;
#[cfg(windows)]
extern crate winapi;

#[cfg(not(windows))]
use nix::sys::socket::{sockaddr, sockaddr_in, sockaddr_in6, sockaddr_storage};
#[cfg(windows)]
use winapi::shared::ws2def::{
    SOCKADDR as sockaddr, SOCKADDR_IN as sockaddr_in, SOCKADDR_IN6_LH as sockaddr_in6,
    SOCKADDR_STORAGE as sockaddr_storage,
};
