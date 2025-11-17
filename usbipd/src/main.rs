use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, clap::Parser)]
struct Args {
    /// Bind to IPv4. Default is both
    #[arg(short = '4', long)]
    ipv4: bool,
    /// Bind to IPv6. Default is both
    #[arg(short = '6', long)]
    ipv6: bool,
    /// Run in device mode
    ///
    /// Rather than drive an attached device, create a virtual UDC to bind gadgets to
    #[arg(short = 'e', long)]
    device: bool,
    /// Run as a daemon process
    #[arg(short = 'D', long)]
    daemon: bool,
    /// Print debugging information
    #[arg(short = 'd', long)]
    debug: bool,
    /// Write process id to FILE
    ///
    /// If no FILE specified, use `/var/run/usbipd.pid`
    #[arg(short = 'P', long = "pid", name = "FILE")]
    pid_file: Option<Option<PathBuf>>,
    /// Listen on TCP/IP port PORT
    #[arg(short = 't', long = "tcp-port")]
    port: Option<u16>,
}

fn main() {
    let args = Args::parse();

    todo!("{args:?}");
}
