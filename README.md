# usbip-rs

A complete Rust rewrite of the Linux USB/IP userspace stack including:

- `usbip` CLI tool (vhci_hcd/usbip_host)
- `usbipd` server daemon (usbip_host)

> [!WARNING]
> This project is still actively in development, so a lot about the library and CLI interfaces are subject to change

This rewrite aims to improve the user and developer experience when interacting with the usbip kernel modules (vhci_hcd and usbip_host) in several ways:

- Rust lib crate to fulfill all the original CLI functions
- New CLI features
    - Nicer argument parsing using [`clap`](https://crates.io/crates/clap)
    - Improved output format that is easier to read and shows more data
    - JSON output mode for easy parsing
    - Significantly improved error messages
    - Better debug logging using tracing
- Legacy CLI output mode for backwards compatability with the original CLI interface
- Better performance (does not read in hwdb on startup)
- More secure (implemented in 100% safe rust using `#![forbid(unsafe_code)]`)
- Fixes some bugs in the original implemenation (like not being able to address more than 256 virtual device ports)

## Planned Features

- Async library API
- VUDC integration
- New userspace network protocol (authentication? might not be worth it without new kernel drivers too)
