//! SH8601 AMOLED display driver for the Waveshare ESP32-S3-Touch-AMOLED-1.64 board.
//!
//! This module owns the low-level QSPI panel setup, reset sequence, brightness
//! command, and RGB565 row transfers. App-level drawing effects should live
//! outside this driver and feed pixel rows through `Sh8601::draw_rows`.

use core::ffi::{c_int, c_void};
use core::mem::MaybeUninit;
use core::ptr::{self, NonNull};
use core::slice;

use crate::time::sleep_ms;

const LCD_HOST: c_int = 1;
const SPI_DMA_CH_AUTO: c_int = 3;
const MALLOC_CAP_DMA: u32 = 1 << 3;

const LCD_BIT_PER_PIXEL: usize = 16;
const LCD_CS: c_int = 9;
const LCD_PCLK: c_int = 10;
const LCD_DATA0: c_int = 11;
const LCD_DATA1: c_int = 12;
const LCD_DATA2: c_int = 13;
const LCD_DATA3: c_int = 14;
const LCD_RST: c_int = 21;

pub const LCD_H_RES: usize = 280;
pub const LCD_V_RES: usize = 456;
const LCD_X_OFFSET: i32 = 0x14;
const ROWS_PER_CHUNK: usize = 24;

const ESP_OK: EspErr = 0;
const ESP_ERR_NO_MEM: EspErr = 0x101;

const GPIO_MODE_OUTPUT: c_int = 1 << 1;

const LCD_CMD_CASET: i32 = 0x2A;
const LCD_CMD_RASET: i32 = 0x2B;
const LCD_CMD_RAMWR: i32 = 0x2C;
const LCD_CMD_MADCTL: i32 = 0x36;
const LCD_CMD_COLMOD: i32 = 0x3A;
const LCD_CMD_NOP: i32 = 0x00;

pub type EspErr = i32;
pub type EspResult<T = ()> = Result<T, EspErr>;

type PanelIoHandle = *mut c_void;

pub fn disable_panel() -> EspResult {
    check(unsafe { gpio_reset_pin(LCD_RST) })?;
    check(unsafe { gpio_set_direction(LCD_RST, GPIO_MODE_OUTPUT) })?;
    check(unsafe { gpio_set_level(LCD_RST, 0) })
}

#[repr(C)]
struct SpiBusConfig {
    iocfg: [c_int; 9],
    data_io_default_level: bool,
    max_transfer_sz: c_int,
    flags: u32,
    isr_cpu_id: c_int,
    intr_flags: c_int,
}

#[repr(C)]
struct LcdPanelIoSpiConfig {
    cs_gpio_num: c_int,
    dc_gpio_num: c_int,
    spi_mode: c_int,
    pclk_hz: u32,
    trans_queue_depth: usize,
    on_color_trans_done: Option<extern "C" fn(PanelIoHandle, *mut c_void, *mut c_void) -> bool>,
    user_ctx: *mut c_void,
    lcd_cmd_bits: c_int,
    lcd_param_bits: c_int,
    cs_ena_pretrans: u8,
    cs_ena_posttrans: u8,
    flags: u32,
}

extern "C" {
    fn spi_bus_initialize(
        host_id: c_int,
        bus_config: *const SpiBusConfig,
        dma_chan: c_int,
    ) -> EspErr;
    fn esp_lcd_new_panel_io_spi(
        bus: c_int,
        io_config: *const LcdPanelIoSpiConfig,
        ret_io: *mut PanelIoHandle,
    ) -> EspErr;
    fn esp_lcd_panel_io_tx_param(
        io: PanelIoHandle,
        lcd_cmd: c_int,
        param: *const c_void,
        param_size: usize,
    ) -> EspErr;
    fn esp_lcd_panel_io_tx_color(
        io: PanelIoHandle,
        lcd_cmd: c_int,
        color: *const c_void,
        color_size: usize,
    ) -> EspErr;
    fn heap_caps_malloc(size: usize, caps: u32) -> *mut c_void;
    fn heap_caps_free(ptr: *mut c_void);
    fn gpio_reset_pin(gpio_num: c_int) -> EspErr;
    fn gpio_set_direction(gpio_num: c_int, mode: c_int) -> EspErr;
    fn gpio_set_level(gpio_num: c_int, level: u32) -> EspErr;
}

pub struct Sh8601 {
    io: PanelIoHandle,
}

impl Sh8601 {
    pub fn new() -> EspResult<Self> {
        let buscfg = SpiBusConfig {
            iocfg: [
                LCD_DATA0, LCD_DATA1, LCD_PCLK, LCD_DATA2, LCD_DATA3, -1, -1, -1, -1,
            ],
            data_io_default_level: false,
            max_transfer_sz: (LCD_H_RES * LCD_V_RES * LCD_BIT_PER_PIXEL / 8) as c_int,
            flags: 0,
            isr_cpu_id: 0,
            intr_flags: 0,
        };
        check(unsafe { spi_bus_initialize(LCD_HOST, &buscfg, SPI_DMA_CH_AUTO) })?;

        let io_config = LcdPanelIoSpiConfig {
            cs_gpio_num: LCD_CS,
            dc_gpio_num: -1,
            spi_mode: 0,
            pclk_hz: 40 * 1000 * 1000,
            trans_queue_depth: 10,
            on_color_trans_done: None,
            user_ctx: ptr::null_mut(),
            lcd_cmd_bits: 32,
            lcd_param_bits: 8,
            cs_ena_pretrans: 0,
            cs_ena_posttrans: 0,
            flags: 1 << 4,
        };
        let mut io = MaybeUninit::<PanelIoHandle>::uninit();
        check(unsafe { esp_lcd_new_panel_io_spi(LCD_HOST, &io_config, io.as_mut_ptr()) })?;

        let panel = Self {
            io: unsafe { io.assume_init() },
        };
        panel.reset()?;
        panel.init()?;
        panel.display_on()?;
        Ok(panel)
    }

    pub fn set_brightness(&self, brightness: u8) -> EspResult {
        let command = qspi_command(0x51);
        self.tx_param_raw(command, &[brightness])
    }

    pub fn draw_area(
        &self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        mut fill_rows: impl FnMut(&mut [u16], usize, usize, usize, usize),
    ) -> EspResult {
        if width == 0 || height == 0 {
            return Ok(());
        }

        let x = x.min(LCD_H_RES);
        let y = y.min(LCD_V_RES);
        let width = width.min(LCD_H_RES.saturating_sub(x));
        let height = height.min(LCD_V_RES.saturating_sub(y));
        if width == 0 || height == 0 {
            return Ok(());
        }

        let mut buffer = DmaPixelBuffer::new(width * ROWS_PER_CHUNK)?;
        for row_start in (y..y + height).step_by(ROWS_PER_CHUNK) {
            let rows = ROWS_PER_CHUNK.min(y + height - row_start);
            let len = width * rows;
            fill_rows(&mut buffer.as_mut_slice()[..len], x, row_start, width, rows);
            self.draw_bitmap_area(
                x as i32,
                row_start as i32,
                width as i32,
                rows as i32,
                buffer.as_slice()[..len].as_ptr(),
            )?;
        }

        Ok(())
    }

    fn reset(&self) -> EspResult {
        check(unsafe { gpio_reset_pin(LCD_RST) })?;
        check(unsafe { gpio_set_direction(LCD_RST, GPIO_MODE_OUTPUT) })?;
        check(unsafe { gpio_set_level(LCD_RST, 0) })?;
        sleep_ms(10);
        check(unsafe { gpio_set_level(LCD_RST, 1) })?;
        sleep_ms(150);
        Ok(())
    }

    fn init(&self) -> EspResult {
        self.tx_param(LCD_CMD_MADCTL, &[0x00])?;
        self.tx_param(LCD_CMD_COLMOD, &[0x55])?;

        for command in INIT_COMMANDS {
            self.tx_param(command.cmd, command.data)?;
            sleep_ms(command.delay_ms);
        }

        Ok(())
    }

    fn display_on(&self) -> EspResult {
        self.tx_param(0x29, &[])?;
        sleep_ms(10);
        Ok(())
    }

    fn draw_bitmap_area(
        &self,
        x: i32,
        y: i32,
        width: i32,
        rows: i32,
        pixels: *const u16,
    ) -> EspResult {
        let x_start = LCD_X_OFFSET + x;
        let x_end = x_start + width;
        let y_start = y;
        let y_end = y + rows;

        self.tx_param(
            LCD_CMD_CASET,
            &[
                (x_start >> 8) as u8,
                x_start as u8,
                ((x_end - 1) >> 8) as u8,
                (x_end - 1) as u8,
            ],
        )?;
        self.tx_param(
            LCD_CMD_RASET,
            &[
                (y_start >> 8) as u8,
                y_start as u8,
                ((y_end - 1) >> 8) as u8,
                (y_end - 1) as u8,
            ],
        )?;

        let len = width as usize * rows as usize * size_of::<u16>();
        self.tx_color(LCD_CMD_RAMWR, pixels.cast(), len)?;
        self.wait_color_transfer()
    }

    fn tx_param(&self, command: i32, data: &[u8]) -> EspResult {
        self.tx_param_raw(qspi_command(command), data)
    }

    fn wait_color_transfer(&self) -> EspResult {
        self.tx_param(LCD_CMD_NOP, &[])
    }

    fn tx_param_raw(&self, command: i32, data: &[u8]) -> EspResult {
        check(unsafe {
            esp_lcd_panel_io_tx_param(self.io, command, data.as_ptr().cast(), data.len())
        })
    }

    fn tx_color(&self, command: i32, pixels: *const c_void, len: usize) -> EspResult {
        check(unsafe {
            esp_lcd_panel_io_tx_color(self.io, qspi_color_command(command), pixels, len)
        })
    }
}

struct InitCommand {
    cmd: i32,
    data: &'static [u8],
    delay_ms: u32,
}

const INIT_COMMANDS: &[InitCommand] = &[
    InitCommand {
        cmd: 0x11,
        data: &[],
        delay_ms: 80,
    },
    InitCommand {
        cmd: 0xC4,
        data: &[0x80],
        delay_ms: 0,
    },
    InitCommand {
        cmd: 0x35,
        data: &[0x00],
        delay_ms: 0,
    },
    InitCommand {
        cmd: 0x53,
        data: &[0x20],
        delay_ms: 1,
    },
    InitCommand {
        cmd: 0x63,
        data: &[0xFF],
        delay_ms: 1,
    },
    InitCommand {
        cmd: 0x51,
        data: &[0x00],
        delay_ms: 1,
    },
];

struct DmaPixelBuffer {
    pixels: NonNull<u16>,
    len: usize,
}

impl DmaPixelBuffer {
    fn new(len: usize) -> EspResult<Self> {
        let pixels =
            unsafe { heap_caps_malloc(len * size_of::<u16>(), MALLOC_CAP_DMA) }.cast::<u16>();
        let Some(pixels) = NonNull::new(pixels) else {
            return Err(ESP_ERR_NO_MEM);
        };

        Ok(Self { pixels, len })
    }

    fn as_slice(&self) -> &[u16] {
        unsafe { slice::from_raw_parts(self.pixels.as_ptr(), self.len) }
    }

    fn as_mut_slice(&mut self) -> &mut [u16] {
        unsafe { slice::from_raw_parts_mut(self.pixels.as_ptr(), self.len) }
    }
}

impl Drop for DmaPixelBuffer {
    fn drop(&mut self) {
        unsafe { heap_caps_free(self.pixels.as_ptr().cast()) };
    }
}

fn qspi_command(command: i32) -> i32 {
    (0x02 << 24) | ((command & 0xff) << 8)
}

fn qspi_color_command(command: i32) -> i32 {
    (0x32 << 24) | ((command & 0xff) << 8)
}

fn check(err: EspErr) -> EspResult {
    if err == ESP_OK {
        Ok(())
    } else {
        Err(err)
    }
}
