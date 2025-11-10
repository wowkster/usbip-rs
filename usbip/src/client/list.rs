use crate::drivers::vhci_hcd::UsbSpeed;

#[derive(Debug, thiserror::Error)]
pub enum Error {}

#[derive(Debug, serde::Serialize)]
pub struct ExportedDevice {
    pub remote_host: String,
    pub remote_port: u16,

    pub url: String,

    pub remote_sys_path: String,
    pub remote_bus_id: String,

    pub remote_bus_num: u16,
    pub remote_dev_num: u16,

    pub manufacturer_display: Option<String>,
    pub product_display: Option<String>,

    pub manufacturer: String,
    pub product: String,

    pub speed: UsbSpeed,

    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,

    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,
    pub b_configuration_value: u8,
    pub b_num_configurations: u8,
    pub b_num_interfaces: u8,
}

pub fn list_exported_devices(host: &str) -> Result<Vec<ExportedDevice>, Error> {
    todo!()
}
