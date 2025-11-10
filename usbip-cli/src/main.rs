use clap::Parser;
use tracing_subscriber::filter::LevelFilter;
use usbip::{
    client::{
        ExportedDevice, ImportedDevice, attach_device, detach_device, list_exported_devices,
        list_imported_devices,
    },
    drivers::vhci_hcd::{UsbSpeed, VhciDeviceStatus},
};

#[derive(clap::Parser)]
#[clap(name = "usbip")]
struct Args {
    #[clap(subcommand)]
    command: Command,
    #[arg(short = 'd', long)]
    debug: bool,
    #[arg(short = 'j', long)]
    json_output: bool,
    // TODO: hide all log output
    // TODO: use baked usb ids database
    // TODO: use custom usb ids database path
}

#[derive(clap::Subcommand)]
enum Command {
    Attach {
        #[arg(short = 'r', long)]
        remote_host: String,
        // TODO: TCP port
        #[arg(short = 'b', long)]
        bus_id: String,
    },
    Detach {
        #[arg(short = 'p', long)]
        port: u16,
        // TODO: TCP port?
    },
    Port,
    List {
        #[arg(short = 'r', long, group = "list_mode")]
        remote_host: Option<String>,
        // TODO: TCP port? (use flatten for corrext grouping)
        #[arg(short = 'l', long, group = "list_mode")]
        local: bool,
    },
}

fn main() {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(if args.debug {
            LevelFilter::TRACE
        } else {
            LevelFilter::WARN
        })
        .with_writer(std::io::stderr)
        .init();

    match args.command {
        Command::Attach {
            remote_host,
            bus_id,
        } => match attach_device(&remote_host, &bus_id) {
            Ok(port) => {
                if args.json_output {
                    let v = serde_json::json!({
                        "port": port
                    });

                    println!("{}", serde_json::to_string(&v).unwrap())
                } else {
                    println!("Device attached successfuly to port {port}")
                }
            }
            Err(e) => {
                eprintln!("Failed to attach device: {e}")
            }
        },
        Command::Detach { port } => match detach_device(port) {
            Ok(_) => {
                if args.json_output {
                    let v = serde_json::json!({
                        "port": port
                    });

                    println!("{}", serde_json::to_string(&v).unwrap())
                } else {
                    println!("Device detached successfully from port {port}")
                }
            }
            Err(e) => {
                eprintln!("Failed to detach device: {e}")
            }
        },
        Command::Port => match list_imported_devices() {
            Ok(devices) => {
                if args.json_output {
                    println!("{}", serde_json::to_string(&devices).unwrap())
                } else {
                    print_imported_devices(&devices);
                }
            }
            Err(e) => {
                eprintln!("Failed to list devices: {e}")
            }
        },
        Command::List { remote_host, local } => {
            assert_ne!(remote_host.is_some(), local);

            if let Some(host) = remote_host {
                match list_exported_devices(&host) {
                    Ok(devices) => {
                        if args.json_output {
                            println!("{}", serde_json::to_string(&devices).unwrap())
                        } else {
                            print_exported_devices(&devices);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to list exported devices: {e}")
                    }
                }
            } else {
                todo!("list local devices");
            }
        }
    }
}

fn print_imported_devices(devices: &[ImportedDevice]) {
    for device in devices {
        // Line 1

        print!("Port {:02}: <", device.port);

        match device.status {
            // TODO: impl printing for unused and initializing ports if we allow outputting those
            VhciDeviceStatus::NotConnected | VhciDeviceStatus::NotAssigned => unreachable!(),
            VhciDeviceStatus::Used => print!("Port in Use"),
            VhciDeviceStatus::Error => print!("Port Error"),
        }

        print!("> at ");

        match device.speed {
            UsbSpeed::Unknown => print!("Unknown Speed"),
            UsbSpeed::Low => print!("Low Speed (1.5Mbps)"),
            UsbSpeed::Full => print!("Full Speed (12Mbps)"),
            UsbSpeed::High => print!("High Speed (480Mbps)"),
            UsbSpeed::Wireless => print!("Wireless"),
            UsbSpeed::Super => print!("Super Speed (5000Mbps)"),
        }

        println!();

        // Line 2

        print!("       ");

        if let Some(display) = &device.manufacturer_display {
            print!("{}", display)
        } else {
            print!("{}", device.manufacturer)
        }

        print!(" : ");

        if let Some(display) = &device.product_display {
            print!("{}", display)
        } else {
            print!("{}", device.product)
        }

        println!(" ({:04X}:{:04X})", device.id_vendor, device.id_product);

        // Line 3

        print!("{:>10} -> ", device.local_bus_id);

        if let Some(url) = &device.url {
            print!("{}", url)
        } else {
            print!("unknown host, remote port and remote busid")
        }

        println!();

        // Line 4

        println!(
            "{:>10} -> remote bus/dev {:03}/{:03}",
            "", device.remote_bus_num, device.remote_dev_num
        );
    }
}

fn print_exported_devices(devices: &[ExportedDevice]) {
    todo!()
}
