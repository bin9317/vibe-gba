use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct PpuRegisters {
    pub dispcnt: u16,
    pub dispstat: u16,
    pub vcount: u16,
    pub bg0cnt: u16,
    pub bg1cnt: u16,
    pub bg2cnt: u16,
    pub bg3cnt: u16,
    pub bg0hofs: u16,
    pub bg0vofs: u16,
    pub bg1hofs: u16,
    pub bg1vofs: u16,
    pub bg2hofs: u16,
    pub bg2vofs: u16,
    pub bg3hofs: u16,
    pub bg3vofs: u16,

    // Affine registers for BG2
    pub bg2pa: i16,
    pub bg2pb: i16,
    pub bg2pc: i16,
    pub bg2pd: i16,
    pub bg2x: i32,
    pub bg2y: i32,

    // Affine registers for BG3
    pub bg3pa: i16,
    pub bg3pb: i16,
    pub bg3pc: i16,
    pub bg3pd: i16,
    pub bg3x: i32,
    pub bg3y: i32,

    // Internal latch registers for Affine BGs (reloaded at VBlank)
    pub bg2x_latch: i32,
    pub bg2y_latch: i32,
    pub bg3x_latch: i32,
    pub bg3y_latch: i32,

    // Window registers
    pub win0h: u16,
    pub win1h: u16,
    pub win0v: u16,
    pub win1v: u16,
    pub winin: u16,
    pub winout: u16,

    // Mosaic
    pub mosaic: u16,

    // Special Effects
    pub bldcnt: u16,
    pub bldalpha: u16,
    pub bldy: u16,
}

impl Default for PpuRegisters {
    fn default() -> Self {
        Self::new()
    }
}

impl PpuRegisters {
    pub fn new() -> Self {
        Self {
            dispcnt: 0,
            dispstat: 0,
            vcount: 0,
            bg0cnt: 0,
            bg1cnt: 0,
            bg2cnt: 0,
            bg3cnt: 0,
            bg0hofs: 0,
            bg0vofs: 0,
            bg1hofs: 0,
            bg1vofs: 0,
            bg2hofs: 0,
            bg2vofs: 0,
            bg3hofs: 0,
            bg3vofs: 0,
            bg2pa: 0x0100,
            bg2pb: 0,
            bg2pc: 0,
            bg2pd: 0x0100,
            bg2x: 0,
            bg2y: 0,
            bg3pa: 0x0100,
            bg3pb: 0,
            bg3pc: 0,
            bg3pd: 0x0100,
            bg3x: 0,
            bg3y: 0,
            bg2x_latch: 0,
            bg2y_latch: 0,
            bg3x_latch: 0,
            bg3y_latch: 0,
            win0h: 0,
            win1h: 0,
            win0v: 0,
            win1v: 0,
            winin: 0,
            winout: 0,
            mosaic: 0,
            bldcnt: 0,
            bldalpha: 0,
            bldy: 0,
        }
    }
}
