//! Helper functions for performing udev hwdb queries

pub(crate) fn get_device_display_strings(
    #[cfg(feature = "runtime-hwdb")] hwdb: &udev::Hwdb,
    vendor_id: u16,
    product_id: u16,
) -> (Option<String>, Option<String>) {
    #[cfg(feature = "runtime-hwdb")]
    let (vendor, product) = {
        // TODO: add an option to fall back to baked hwdb?

        let results: Vec<_> = hwdb
            .query(format!("usb:v{vendor_id:04X}p{product_id:04X}*"))
            .collect();

        let mut vendor = results
            .iter()
            .find(|e| e.name().to_string_lossy() == "ID_VENDOR_FROM_DATABASE")
            .map(|e| e.value().to_string_lossy().to_string());
        let mut product = results
            .iter()
            .find(|e| e.name().to_string_lossy() == "ID_MODEL_FROM_DATABASE")
            .map(|e| e.value().to_string_lossy().to_string());

        (vendor, product)
    };

    #[cfg(feature = "baked-hwdb")]
    let (vendor, product) = {
        let mut vendor = None;
        let mut product = None;

        for v in usb_ids::Vendors::iter() {
            if v.id() == vendor_id {
                vendor = Some(v.name().to_string());

                for d in v.devices() {
                    if d.id() == product_id {
                        product = Some(d.name().to_string());
                        break;
                    }
                }

                break;
            }
        }

        (vendor, product)
    };

    (vendor, product)
}

pub(crate) fn get_class_display_strings(
    #[cfg(feature = "runtime-hwdb")] hwdb: &udev::Hwdb,
    class: u8,
    sub_class: u8,
    protocol: u8,
) -> (Option<String>, Option<String>, Option<String>) {
    #[cfg(feature = "runtime-hwdb")]
    let (class, sub_class, protocol) = {
        // TODO: investigate using interface level queries first and then
        // falling back to device level if none are found. We should check what
        // lsusb does here.

        // TODO: add an option to fall back to baked hwdb

        let results: Vec<_> = hwdb
            .query(format!(
                "usb:v*p*d*dc{class:02X}dsc{sub_class:02X}dp{protocol:02X}*"
            ))
            .collect();

        let mut class = results
            .iter()
            .find(|e| e.name().to_string_lossy() == "ID_USB_CLASS_FROM_DATABASE")
            .map(|e| e.value().to_string_lossy().to_string());
        let mut sub_class = results
            .iter()
            .find(|e| e.name().to_string_lossy() == "ID_USB_SUBCLASS_FROM_DATABASE")
            .map(|e| e.value().to_string_lossy().to_string());
        let mut protocol = results
            .iter()
            .find(|e| e.name().to_string_lossy() == "ID_USB_PROTOCOL_FROM_DATABASE")
            .map(|e| e.value().to_string_lossy().to_string());

        (class, sub_class, protocol)
    };

    #[cfg(feature = "baked-hwdb")]
    let (class, sub_class, protocol) = {
        let mut class_display = None;
        let mut sub_class_display = None;
        let mut protocol_display = None;

        for c in usb_ids::Classes::iter() {
            if c.id() == class {
                class_display = Some(c.name().to_string());

                for s in c.sub_classes() {
                    if s.id() == sub_class {
                        sub_class_display = Some(s.name().to_string());

                        for p in s.protocols() {
                            if p.id() == protocol {
                                protocol_display = Some(p.name().to_string());
                                break;
                            }
                        }

                        break;
                    }
                }

                break;
            }
        }

        (class_display, sub_class_display, protocol_display)
    };

    (class, sub_class, protocol)
}
