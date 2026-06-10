use core::ffi::c_uint;

extern "C" {
    fn esp_rom_delay_us(us: c_uint);
}

pub fn sleep_ms(ms: u32) {
    unsafe { esp_rom_delay_us(ms.saturating_mul(1000)) };
}
