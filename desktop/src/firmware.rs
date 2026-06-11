use quota_dock_core::config::DEFAULT_APP_OFFSET;
use quota_dock_core::flash::{flash_firmware_images, FlashImages};

const APP_BIN: &[u8] = include_bytes!("../../firmware/target/flash/app.bin");
const BOOTLOADER_BIN: &[u8] = include_bytes!("../../firmware/target/flash/bootloader.bin");
const PARTITION_TABLE_BIN: &[u8] =
    include_bytes!("../../firmware/target/flash/partition-table.bin");

#[derive(Clone, Copy, Debug)]
pub struct BundledFirmware {
    pub app_bytes: usize,
    pub bootloader_bytes: usize,
    pub partition_table_bytes: usize,
    pub offset: &'static str,
    pub version: &'static str,
    pub hash: &'static str,
}

pub fn bundled_firmware() -> BundledFirmware {
    BundledFirmware {
        app_bytes: APP_BIN.len(),
        bootloader_bytes: BOOTLOADER_BIN.len(),
        partition_table_bytes: PARTITION_TABLE_BIN.len(),
        offset: DEFAULT_APP_OFFSET,
        version: env!("QUOTA_DOCK_FIRMWARE_VERSION"),
        hash: env!("QUOTA_DOCK_FIRMWARE_HASH"),
    }
}

pub fn flash_bundled_firmware(port: &str, baud: u32) -> Result<(), String> {
    let images = FlashImages {
        firmware: APP_BIN,
        bootloader: BOOTLOADER_BIN,
        partition_table: PARTITION_TABLE_BIN,
        offset: DEFAULT_APP_OFFSET,
    };
    flash_firmware_images(&images, port, baud)
}
