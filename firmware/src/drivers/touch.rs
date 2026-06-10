//! FT3168 capacitive touch driver for the Waveshare ESP32-S3-Touch-AMOLED-1.64 board.
//!
//! The controller is connected on I2C0 at address `0x38` with SDA on GPIO47 and
//! SCL on GPIO48. This module only initializes the controller and reads raw
//! screen-space touch points; gesture or UI behavior belongs in app code.

use core::ffi::c_int;

use crate::drivers::display::{EspErr, EspResult};

const I2C_PORT: c_int = 0;
const I2C_MODE_MASTER: c_int = 1;
const GPIO_PULLUP_ENABLE: bool = true;

const TOUCH_SDA: c_int = 47;
const TOUCH_SCL: c_int = 48;
const TOUCH_CLOCK_HZ: u32 = 300 * 1000;
const TOUCH_TIMEOUT_TICKS: u32 = 1000;

const FT3168_ADDRESS: u8 = 0x38;
const FT3168_DEVICE_MODE: u8 = 0x00;
const FT3168_TOUCH_COUNT: u8 = 0x02;
const FT3168_TOUCH1_XH: u8 = 0x03;

const LCD_H_RES: u16 = 280;
const LCD_V_RES: u16 = 456;

#[repr(C)]
struct I2cConfig {
    mode: c_int,
    sda_io_num: c_int,
    scl_io_num: c_int,
    sda_pullup_en: bool,
    scl_pullup_en: bool,
    master: I2cMasterConfig,
    clk_flags: u32,
}

#[repr(C)]
union I2cMasterConfig {
    master: I2cMaster,
    slave: I2cSlave,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct I2cMaster {
    clk_speed: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct I2cSlave {
    addr_10bit_en: u8,
    slave_addr: u16,
    maximum_speed: u32,
}

extern "C" {
    fn i2c_param_config(i2c_num: c_int, i2c_conf: *const I2cConfig) -> EspErr;
    fn i2c_driver_install(
        i2c_num: c_int,
        mode: c_int,
        slv_rx_buf_len: usize,
        slv_tx_buf_len: usize,
        intr_alloc_flags: c_int,
    ) -> EspErr;
    fn i2c_master_write_to_device(
        i2c_num: c_int,
        device_address: u8,
        write_buffer: *const u8,
        write_size: usize,
        ticks_to_wait: u32,
    ) -> EspErr;
    fn i2c_master_write_read_device(
        i2c_num: c_int,
        device_address: u8,
        write_buffer: *const u8,
        write_size: usize,
        read_buffer: *mut u8,
        read_size: usize,
        ticks_to_wait: u32,
    ) -> EspErr;
}

pub struct Ft3168;

#[derive(Clone, Copy, Debug)]
pub struct TouchPoint {
    pub x: u16,
    pub y: u16,
}

impl Ft3168 {
    pub fn new() -> EspResult<Self> {
        let config = I2cConfig {
            mode: I2C_MODE_MASTER,
            sda_io_num: TOUCH_SDA,
            scl_io_num: TOUCH_SCL,
            sda_pullup_en: GPIO_PULLUP_ENABLE,
            scl_pullup_en: GPIO_PULLUP_ENABLE,
            master: I2cMasterConfig {
                master: I2cMaster {
                    clk_speed: TOUCH_CLOCK_HZ,
                },
            },
            clk_flags: 0,
        };

        check(unsafe { i2c_param_config(I2C_PORT, &config) })?;
        check(unsafe { i2c_driver_install(I2C_PORT, I2C_MODE_MASTER, 0, 0, 0) })?;

        let normal_mode = [FT3168_DEVICE_MODE, 0x00];
        check(unsafe {
            i2c_master_write_to_device(
                I2C_PORT,
                FT3168_ADDRESS,
                normal_mode.as_ptr(),
                normal_mode.len(),
                TOUCH_TIMEOUT_TICKS,
            )
        })?;

        Ok(Self)
    }

    pub fn read_point(&self) -> EspResult<Option<TouchPoint>> {
        let mut count = [0_u8; 1];
        self.read_register(FT3168_TOUCH_COUNT, &mut count)?;

        if count[0] == 0 {
            return Ok(None);
        }

        let mut data = [0_u8; 4];
        self.read_register(FT3168_TOUCH1_XH, &mut data)?;

        let x = (((data[0] as u16 & 0x0f) << 8) | data[1] as u16).min(LCD_H_RES);
        let y = (((data[2] as u16 & 0x0f) << 8) | data[3] as u16).min(LCD_V_RES);
        Ok(Some(TouchPoint { x, y }))
    }

    fn read_register(&self, register: u8, buffer: &mut [u8]) -> EspResult {
        check(unsafe {
            i2c_master_write_read_device(
                I2C_PORT,
                FT3168_ADDRESS,
                &register,
                1,
                buffer.as_mut_ptr(),
                buffer.len(),
                TOUCH_TIMEOUT_TICKS,
            )
        })
    }
}

fn check(err: EspErr) -> EspResult {
    if err == 0 {
        Ok(())
    } else {
        Err(err)
    }
}
