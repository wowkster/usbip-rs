use clap::Parser;
use colored::Colorize;
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
        #[arg(short = 'r', long)]
        remote_host: String,
        // TODO: TCP port
        #[arg(short = 'b', long)]
        bus_id: String,
    },
    /// Detach a remote USB device
    Detach {
        #[arg(short = 'p', long)]
        port: u16,
        // TODO: TCP port?
    },
    /// List exportable or local USB devices
    #[group(required = true, multiple = false)]
    List {
        #[arg(short = 'r', long, group = "list_mode")]
        remote_host: Option<String>,
        // TODO: TCP port? (use flatten for corrext grouping)
        #[arg(short = 'l', long, group = "list_mode")]
        local: bool,
    },
    /// Show imported USB devices
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
                eprintln!("{} {e}", "Error:".red());
                std::process::exit(1);
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
                eprintln!("{} {e}", "Error:".red());
                std::process::exit(1);
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
                            if devices.is_empty() {
                                return;
                            }

                            print_exported_devices(&host, &devices);
                        }
                    }
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red());
                        std::process::exit(1);
                    }
                }
            } else {
                // TODO: nice colored error otuput (even in legacy mode)
                todo!("list local devices");
            }
        }
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
            UsbSpeed::Low => print!("Low Speed(1.5Mbps)"),
            UsbSpeed::Full => print!("Full Speed(12Mbps)"),
            UsbSpeed::High => print!("High Speed(480Mbps)"),
            UsbSpeed::Wireless => print!("Wireless"),
            UsbSpeed::Super => print!("Super Speed(5000Mbps)"),
        }

        println!();

        print!("       ");

        if let Some(vendor) = &device.vendor_display {
            print!("{vendor}");
        } else {
            print!("unknown vendor");
        }

        print!(" : ");

        if let Some(product) = &device.product_display {
            print!("{product}");
        } else {
            print!("unknown product");
        }

        println!(" ({:04x}:{:04x})", device.id_vendor, device.id_product);

        print!("{:>10} -> ", device.local_bus_id);

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

fn print_exported_devices(host: &str, devices: &[ExportedDevice]) {
    println!("Exportable USB devices");
    println!("======================");

    println!(" - {}", host);

    for device in devices {
        print!("{:>11}: ", device.bus_id,);

        if let Some(vendor) = &device.vendor_display {
            print!("{vendor}");
        } else {
            print!("unknown vendor");
        }

        print!(" : ");

        if let Some(product) = &device.product_display {
            print!("{product}");
        } else {
            print!("unknown product");
        }

        println!(" ({:04x}:{:04x})", device.id_vendor, device.id_product);

        println!("{:>11}: {}", "", device.sys_path);

        print!("{:>11}: ", "");

        if device.b_device_class == 0
            && device.b_device_sub_class == 0
            && device.b_device_protocol == 0
        {
            print!("(Defined at Interface level)");
        } else {
            if let Some(class) = &device.class_display {
                print!("{class}");
            } else {
                print!("unknown class");
            }

            print!(" / ");

            if let Some(sub_class) = &device.sub_class_display {
                print!("{sub_class}");
            } else {
                print!("unknown subclass");
            }

            print!(" / ");

            if let Some(protocol) = &device.protocol_display {
                print!("{protocol}");
            } else {
                print!("unknown protocol");
            }
        }

        println!(
            " ({:02x}/{:02x}/{:02x})",
            device.b_device_class, device.b_device_sub_class, device.b_device_protocol
        );

        for (i, iface) in device.interfaces.iter().enumerate() {
            print!("{:>11}: {:>2} - ", "", i);

            if let Some(class) = &iface.class_display {
                print!("{class}");
            } else {
                print!("unknown class");
            }

            print!(" / ");

            if let Some(sub_class) = &iface.sub_class_display {
                print!("{sub_class}");
            } else {
                print!("unknown subclass");
            }

            print!(" / ");

            if let Some(protocol) = &iface.protocol_display {
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
