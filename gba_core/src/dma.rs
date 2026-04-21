use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct DmaChannel {
    pub src: u32,
    pub dst: u32,
    pub count: u16,
    pub cnt: u16,
    pub internal_src: u32,
    pub internal_dst: u32,
    pub internal_count: u32,
}

impl DmaChannel {
    pub fn new() -> Self {
        Self {
            src: 0,
            dst: 0,
            count: 0,
            cnt: 0,
            internal_src: 0,
            internal_dst: 0,
            internal_count: 0,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Dma {
    pub channels: [DmaChannel; 4],
}

impl Dma {
    pub fn new() -> Self {
        Self {
            channels: [
                DmaChannel::new(),
                DmaChannel::new(),
                DmaChannel::new(),
                DmaChannel::new(),
            ],
        }
    }

    pub fn read_register_byte(&self, offset: u32) -> u8 {
        let ch = ((offset - 0x00B0) / 12) as usize;
        let reg = (offset - 0x00B0) % 12;
        match reg {
            0x8 | 0x9 => 0,
            0xA | 0xB => {
                let mask = if ch == 3 { 0xFFE0 } else { 0xF7E0 };
                let value = self.channels[ch].cnt & mask;
                if reg == 0xA {
                    (value & 0xFF) as u8
                } else {
                    (value >> 8) as u8
                }
            }
            _ => 0,
        }
    }

    pub fn write_register_byte(&mut self, offset: u32, value: u8) -> Option<usize> {
        let ch = ((offset - 0x00B0) / 12) as usize;
        let reg = (offset - 0x00B0) % 12;
        match reg {
            0x0 => self.channels[ch].src = (self.channels[ch].src & 0xFFFFFF00) | (value as u32),
            0x1 => {
                self.channels[ch].src = (self.channels[ch].src & 0xFFFF00FF) | ((value as u32) << 8)
            }
            0x2 => {
                self.channels[ch].src =
                    (self.channels[ch].src & 0xFF00FFFF) | ((value as u32) << 16)
            }
            0x3 => {
                self.channels[ch].src =
                    (self.channels[ch].src & 0x00FFFFFF) | ((value as u32) << 24)
            }
            0x4 => self.channels[ch].dst = (self.channels[ch].dst & 0xFFFFFF00) | (value as u32),
            0x5 => {
                self.channels[ch].dst = (self.channels[ch].dst & 0xFFFF00FF) | ((value as u32) << 8)
            }
            0x6 => {
                self.channels[ch].dst =
                    (self.channels[ch].dst & 0xFF00FFFF) | ((value as u32) << 16)
            }
            0x7 => {
                self.channels[ch].dst =
                    (self.channels[ch].dst & 0x00FFFFFF) | ((value as u32) << 24)
            }
            0x8 => self.channels[ch].count = (self.channels[ch].count & 0xFF00) | (value as u16),
            0x9 => {
                self.channels[ch].count = (self.channels[ch].count & 0x00FF) | ((value as u16) << 8)
            }
            0xA => self.channels[ch].cnt = (self.channels[ch].cnt & 0xFF00) | (value as u16),
            0xB => {
                let old_en = (self.channels[ch].cnt & 0x8000) != 0;
                self.channels[ch].cnt = (self.channels[ch].cnt & 0x00FF) | ((value as u16) << 8);
                let new_en = (self.channels[ch].cnt & 0x8000) != 0;
                if !old_en && new_en {
                    return Some(ch);
                }
            }
            _ => {}
        }

        None
    }
}
