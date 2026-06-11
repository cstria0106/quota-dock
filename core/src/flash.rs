use std::borrow::Cow;
use std::fs;
use std::path::Path;

use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::Flasher;
use espflash::image_format::Segment;
use espflash::target::{Chip, DefaultProgressCallback};
use serialport::{FlowControl, SerialPortType, UsbPortInfo};

use crate::config::FlashInputs;

pub struct FlashImages<'a> {
    pub firmware: &'a [u8],
    pub bootloader: &'a [u8],
    pub partition_table: &'a [u8],
    pub offset: &'a str,
}

pub fn flash_firmware(inputs: &FlashInputs, port: &str, baud: u32) -> Result<(), String> {
    flash_bin(
        &inputs.firmware_bin,
        &inputs.bootloader_bin,
        &inputs.partition_table_bin,
        &inputs.offset,
        port,
        baud,
    )
}

pub fn flash_firmware_images(
    images: &FlashImages<'_>,
    port: &str,
    baud: u32,
) -> Result<(), String> {
    flash_image_bytes(
        images.firmware,
        images.bootloader,
        images.partition_table,
        images.offset,
        port,
        baud,
    )
}

pub fn flash_bin(
    firmware_bin: &Path,
    bootloader_bin: &Path,
    partition_table_bin: &Path,
    offset: &str,
    port: &str,
    baud: u32,
) -> Result<(), String> {
    if !firmware_bin.is_file() {
        return Err(format!(
            "firmware bin does not exist: {}",
            firmware_bin.display()
        ));
    }
    if !bootloader_bin.is_file() {
        return Err(format!(
            "bootloader bin does not exist: {}",
            bootloader_bin.display()
        ));
    }
    if !partition_table_bin.is_file() {
        return Err(format!(
            "partition table bin does not exist: {}",
            partition_table_bin.display()
        ));
    }

    let firmware = fs::read(firmware_bin)
        .map_err(|err| format!("read firmware bin {}: {err}", firmware_bin.display()))?;
    let bootloader = fs::read(bootloader_bin)
        .map_err(|err| format!("read bootloader bin {}: {err}", bootloader_bin.display()))?;
    let partition_table = fs::read(partition_table_bin).map_err(|err| {
        format!(
            "read partition table bin {}: {err}",
            partition_table_bin.display()
        )
    })?;

    flash_image_bytes(&firmware, &bootloader, &partition_table, offset, port, baud)
}

fn flash_image_bytes(
    firmware: &[u8],
    bootloader: &[u8],
    partition_table: &[u8],
    offset: &str,
    port: &str,
    baud: u32,
) -> Result<(), String> {
    if firmware.is_empty() {
        return Err("firmware image is empty".to_string());
    }
    if bootloader.is_empty() {
        return Err("bootloader image is empty".to_string());
    }
    if partition_table.is_empty() {
        return Err("partition table image is empty".to_string());
    }

    let segments = [
        Segment {
            addr: parse_u32(offset)?,
            data: Cow::Borrowed(firmware),
        },
        Segment {
            addr: 0x0,
            data: Cow::Borrowed(bootloader),
        },
        Segment {
            addr: 0x8000,
            data: Cow::Borrowed(partition_table),
        },
    ];
    let mut flasher = connect_flasher(port, baud)?;
    let mut progress = DefaultProgressCallback;
    flasher
        .write_bins_to_flash(&segments, &mut progress)
        .map_err(|err| format!("flash failed: {err}"))
}

pub fn reset_device(port: &str, baud: u32) -> Result<(), String> {
    let mut flasher = connect_flasher(port, baud)?;
    flasher
        .connection()
        .reset()
        .map_err(|err| format!("reset device: {err}"))
}

pub fn probe_esp32s3(port: &str, baud: u32) -> Result<(), String> {
    connect_flasher(port, baud).map(|_| ())
}

pub fn parse_u32(value: &str) -> Result<u32, String> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).map_err(|err| format!("invalid offset {value}: {err}"))
    } else {
        value
            .parse::<u32>()
            .map_err(|err| format!("invalid offset {value}: {err}"))
    }
}

fn connect_flasher(port: &str, baud: u32) -> Result<Flasher, String> {
    let port_info = serialport::available_ports()
        .map_err(|err| err.to_string())?
        .into_iter()
        .find(|info| info.port_name == port);
    let usb_info = match port_info.map(|info| info.port_type) {
        Some(SerialPortType::UsbPort(info)) => info,
        _ => UsbPortInfo {
            vid: 0,
            pid: 0,
            serial_number: None,
            manufacturer: None,
            product: None,
        },
    };
    let serial = serialport::new(port, 115_200)
        .flow_control(FlowControl::None)
        .open_native()
        .map_err(|err| format!("open serial {port}: {err}"))?;
    let connection = Connection::new(
        serial,
        usb_info,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
        baud,
    );
    Flasher::connect(
        connection,
        true,
        true,
        true,
        Some(Chip::Esp32s3),
        Some(baud),
    )
    .map_err(|err| format!("connect flasher: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_decimal_and_hex_offsets() {
        assert_eq!(parse_u32("65536"), Ok(65_536));
        assert_eq!(parse_u32("0x10000"), Ok(65_536));
    }
}
