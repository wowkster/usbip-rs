use clap::Parser;
use colored::Colorize;
use tracing_subscriber::filter::LevelFilter;
use usbip::{
    UsbSpeed,
    client::{
        attach::attach_device,
        detach::detach_device,
        list::{RemoteExportedDevice, list_remote_exported_devices},
        port::{ImportedDevice, list_imported_devices},
    },
    drivers::vhci::VhciDeviceStatus,
    server::{
        bind::bind_device,
        list_local::{LocalExportableDevice, list_local_exportable_devices},
        unbind::unbind_device,
    },
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
    // TODO: add a flag to switch between the old legacy interface (for existing
    // parsers) that exists for backwards compatibility and a new shiny one with
    // colors :). legacy mode will only output the same exact output in the
    // success case. errors should still be formatted in the old way.
    //
    // TODO: env variable to use legacy mode by default (use a cli flag to enable it normally)
    //
    // TODO: use baked usb ids database
    // TODO: use custom usb ids database path
}

#[derive(clap::Subcommand)]
enum Command {
    /// Attach a remote USB device
    Attach {
        // TODO: TCP port
        /// The machine with exported USB devices
        #[arg(short = 'r', long)]
        remote: String,
        /// Bus ID of the device on the remote host
        #[arg(short = 'b', long, conflicts_with = "device")]
        bus_id: Option<String>,
        /// ID of the virtual UDC on the remote host
        #[arg(short = 'd', long, conflicts_with = "bus_id")]
        device: Option<String>,
    },
    /// Detach a remote USB device
    Detach {
        // TODO: TCP port?
        /// Local vhci_hcd port the device is bound to
        #[arg(short = 'p', long)]
        port: u16,
    },
    /// List exportable or local USB devices
    List {
        // TODO: TCP port?
        /// List all exportable devices on a remote host
        #[arg(short = 'r', long, conflicts_with = "local", conflicts_with = "device")]
        remote: Option<String>,
        /// List the local USB devices which are eligible to be bound to usbip-host
        #[arg(
            short = 'l',
            long,
            conflicts_with = "remote",
            conflicts_with = "device"
        )]
        local: bool,

        /// List the local USB gadgets bound to usbip-vudc
        #[arg(short = 'd', long, conflicts_with = "local", conflicts_with = "remote")]
        device: bool,

        /// Prints the output in a parsable format (use --json-output instead for better results)
        #[arg(short = 'p', long)]
        parsable: bool,
    },
    /// Bind device to usbip_host.ko
    Bind {
        /// Local bus ID of the USB device
        #[arg(short = 'b', long)]
        bus_id: String,
    },
    /// Unbind device from usbip_host.ko
    Unbind {
        /// Local bus ID of the USB device (must already be bound to usbip-host)
        #[arg(short = 'b', long)]
        bus_id: String,
    },
    /// Show all imported USB devices
    Port,
}

fn main() {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(if args.debug {
            LevelFilter::TRACE
        } else {
            LevelFilter::OFF
        })
        .with_writer(std::io::stderr)
        .init();

    match args.command {
        Command::Attach {
            remote,
            bus_id,
            device,
        } => {
            // These are 2 different CLI arguments but the server actually
            // treats them the same so we dont make any disctinction here
            assert_ne!(bus_id.is_some(), device.is_some());
            let bus_id = bus_id.or(device).unwrap();

            match attach_device(&remote, &bus_id) {
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
                    eprintln!("{} {e}", "Error:".red());
                    std::process::exit(1);
                }
            }
        }
        Command::Detach { port } => match detach_device(port, true) {
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
                eprintln!("{} {e}", "Error:".red());
                std::process::exit(1);
            }
        },
        Command::List {
            remote,
            local,
            device,
            parsable,
        } => {
            assert_ne!(remote.is_some(), local);
            assert_ne!(local, device);

            if let Some(host) = remote {
                match list_remote_exported_devices(&host) {
                    Ok(devices) => {
                        if args.json_output {
                            println!("{}", serde_json::to_string(&devices).unwrap())
                        } else {
                            if devices.is_empty() {
                                return;
                            }

                            print_remote_exported_devices(&host, &devices);
                        }
                    }
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red());
                        std::process::exit(1);
                    }
                }
            } else if device {
                todo!("list vudc gadget devices")
            } else {
                match list_local_exportable_devices() {
                    Ok(devices) => {
                        if args.json_output {
                            println!("{}", serde_json::to_string(&devices).unwrap())
                        } else {
                            print_local_exportable_devices(&devices, parsable);
                        }
                    }
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red());
                        std::process::exit(1);
                    }
                }
            }
        }
        Command::Bind { bus_id } => match bind_device(&bus_id) {
            Ok(_) => {
                if args.json_output {
                    let v = serde_json::json!({});

                    println!("{}", serde_json::to_string(&v).unwrap())
                } else {
                    // TODO: what should this output be?
                    println!("Device with bus id {bus_id} bound successfully")
                }
            }
            Err(e) => {
                eprintln!("{} {e}", "Error:".red());
                std::process::exit(1);
            }
        },
        Command::Unbind { bus_id } => match unbind_device(&bus_id) {
            Ok(_) => {
                if args.json_output {
                    let v = serde_json::json!({});

                    println!("{}", serde_json::to_string(&v).unwrap())
                } else {
                    // TODO: what should this output be?
                    println!("Device with bus id {bus_id} unbound successfully")
                }
            }
            Err(e) => {
                eprintln!("{} {e}", "Error:".red());
                std::process::exit(1);
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
                eprintln!("{} {e}", "Error:".red());
                std::process::exit(1);
            }
        },
    }
}

fn print_imported_devices(devices: &[ImportedDevice]) {
    println!("Imported USB devices");
    println!("====================");

    for device in devices {
        let info = &device.local_device_info;

        print!("Port {:02}: <", device.port);

        match device.status {
            // TODO: impl printing for unused and initializing ports if we allow outputting those
            VhciDeviceStatus::NotConnected | VhciDeviceStatus::NotAssigned => unreachable!(),
            VhciDeviceStatus::Used => print!("Port in Use"),
            VhciDeviceStatus::Error => print!("Port Error"),
        }

        print!("> at ");

        match info.speed {
            UsbSpeed::Unknown => print!("Unknown Speed"),
            UsbSpeed::Low => print!("Low Speed(1.5Mbps)"),
            UsbSpeed::Full => print!("Full Speed(12Mbps)"),
            UsbSpeed::High => print!("High Speed(480Mbps)"),
            UsbSpeed::Wireless => print!("Wireless"),
            UsbSpeed::Super => print!("Super Speed(5000Mbps)"),
            // not in the original impl since it was stanrdized after that code
            // was written, but probably good to have
            UsbSpeed::SuperPlus => print!("Super Speed Plus(10000Mbps)"),
        }

        println!();

        print!("       ");

        if let Some(vendor) = &device.vendor {
            print!("{vendor}");
        } else {
            print!("unknown vendor");
        }

        print!(" : ");

        if let Some(product) = &device.product {
            print!("{product}");
        } else {
            print!("unknown product");
        }

        println!(" ({:04x}:{:04x})", info.id_vendor, info.id_product);

        print!("{:>10} -> ", info.bus_id);

        if let Some(url) = &device.url {
            print!("{}", url);
        } else {
            print!("unknown host, remote port and remote busid");
        }

        println!();

        println!(
            "{:>10} -> remote bus/dev {:03}/{:03}",
            "", device.remote_bus_num, device.remote_dev_num
        );
    }
}

fn print_remote_exported_devices(host: &str, devices: &[RemoteExportedDevice]) {
    println!("Exportable USB devices");
    println!("======================");

    println!(" - {}", host);

    for device in devices {
        let info = &device.remote_device_info;

        print!("{:>11}: ", info.bus_id,);

        if let Some(vendor) = &device.vendor {
            print!("{vendor}");
        } else {
            print!("unknown vendor");
        }

        print!(" : ");

        if let Some(product) = &device.product {
            print!("{product}");
        } else {
            print!("unknown product");
        }

        println!(" ({:04x}:{:04x})", info.id_vendor, info.id_product);

        println!("{:>11}: {}", "", info.sys_path);

        print!("{:>11}: ", "");

        if info.b_device_class == 0 && info.b_device_sub_class == 0 && info.b_device_protocol == 0 {
            print!("(Defined at Interface level)");
        } else {
            if let Some(class) = &device.class {
                print!("{class}");
            } else {
                print!("unknown class");
            }

            print!(" / ");

            if let Some(sub_class) = &device.sub_class {
                print!("{sub_class}");
            } else {
                print!("unknown subclass");
            }

            print!(" / ");

            if let Some(protocol) = &device.protocol {
                print!("{protocol}");
            } else {
                print!("unknown protocol");
            }
        }

        println!(
            " ({:02x}/{:02x}/{:02x})",
            info.b_device_class, info.b_device_sub_class, info.b_device_protocol
        );

        for (i, iface) in device.interfaces.iter().enumerate() {
            print!("{:>11}: {:>2} - ", "", i);

            if let Some(class) = &iface.class {
                print!("{class}");
            } else {
                print!("unknown class");
            }

            print!(" / ");

            if let Some(sub_class) = &iface.sub_class {
                print!("{sub_class}");
            } else {
                print!("unknown subclass");
            }

            print!(" / ");

            if let Some(protocol) = &iface.protocol {
                print!("{protocol}");
            } else {
                print!("unknown protocol");
            }

            println!(
                " ({:02x}/{:02x}/{:02x})",
                iface.b_interface_class, iface.b_interface_sub_class, iface.b_interface_protocol
            );
        }

        println!();
    }
}

fn print_local_exportable_devices(devices: &[LocalExportableDevice], parsable: bool) {
    for device in devices {
        if parsable {
            print!(
                "busid={}#usbid={:04x}:{:04x}#",
                device.device_info.bus_id,
                device.device_info.id_vendor,
                device.device_info.id_product
            );
        } else {
            println!(
                " - busid {} ({:04x}:{:04x})",
                device.device_info.bus_id,
                device.device_info.id_vendor,
                device.device_info.id_product
            );

            print!("   ");

            if let Some(vendor) = &device.vendor {
                print!("{vendor}");
            } else {
                print!("unknown vendor");
            }

            print!(" : ");

            if let Some(product) = &device.product {
                print!("{product}");
            } else {
                print!("unknown product");
            }

            println!(
                " ({:04x}:{:04x})",
                device.device_info.id_vendor, device.device_info.id_product
            );
        }

        println!();
    }
}
