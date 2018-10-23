# Rust libutp bindings

|Crate|Documentation|Linux/OS X|Windows|
|:---:|:-----------:|:--------:|-------|
| [![](http://meritbadge.herokuapp.com/rust-libutp)](https://crates.io/crates/rust-libutp) | [![Documentation](https://docs.rs/rust-libutp/badge.svg)](https://docs.rs/rust-libutp) | [![Build Status](https://travis-ci.org/povilasb/rust-libutp.svg?branch=master)](https://travis-ci.org/povilasb/rust-libutp) | [![Build status](https://ci.appveyor.com/api/projects/status/ajw6ab26p86jdac4/branch/master?svg=true)](https://ci.appveyor.com/project/MaidSafe-QA/rust-libutp/branch/master)

rust-libutp wraps [libutp](https://github.com/bittorrent/libutp) and exposes a safe Rust API.
It's a highly work in progress and an experimental crate.

uTP is a reliable (like TCP) data transport protocol built on top of UDP protocol.
See the [specs](http://www.bittorrent.org/beps/bep_0029.html).
It uses [LEDBAT](https://tools.ietf.org/html/rfc6817) congestion control algorithm
which yields to TCP traffic and makes sure uTP traffic does not exceed 100ms
delay. Otherwise, libutp will slow down.
