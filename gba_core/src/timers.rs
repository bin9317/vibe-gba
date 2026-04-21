use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Timer {
    pub reload: u16,
    pub cnt: u16,
    pub internal_value: f64, // To handle different frequencies
    pub current_value: u16,
    #[serde(default)]
    pub startup_delay: u8, // 2-cycle delay after enable before counting starts
}

impl Timer {
    pub fn new() -> Self {
        Self {
            reload: 0,
            cnt: 0,
            internal_value: 0.0,
            current_value: 0,
            startup_delay: 0,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Timers {
    pub timers: [Timer; 4],
}

impl Timers {
    pub fn new() -> Self {
        Self {
            timers: [Timer::new(), Timer::new(), Timer::new(), Timer::new()],
        }
    }

    pub fn read_register_byte(&self, offset: u32) -> u8 {
        let i = ((offset - 0x0100) / 4) as usize;
        let reg = (offset - 0x0100) % 4;
        match reg {
            0 => (self.timers[i].current_value & 0xFF) as u8,
            1 => (self.timers[i].current_value >> 8) as u8,
            2 => (self.timers[i].cnt & 0xFF) as u8,
            3 => (self.timers[i].cnt >> 8) as u8,
            _ => 0,
        }
    }

    pub fn write_register_byte(&mut self, offset: u32, value: u8) {
        let i = ((offset - 0x0100) / 4) as usize;
        let reg = (offset - 0x0100) % 4;
        match reg {
            0 => self.timers[i].reload = (self.timers[i].reload & 0xFF00) | (value as u16),
            1 => self.timers[i].reload = (self.timers[i].reload & 0x00FF) | ((value as u16) << 8),
            2 => {
                let old_en = (self.timers[i].cnt & 0x80) != 0;
                self.timers[i].cnt = (self.timers[i].cnt & 0xFF00) | (value as u16);
                let new_en = (self.timers[i].cnt & 0x80) != 0;
                if !old_en && new_en {
                    self.timers[i].current_value = self.timers[i].reload;
                    self.timers[i].internal_value = 0.0;
                    self.timers[i].startup_delay = 2;
                }
            }
            3 => self.timers[i].cnt = (self.timers[i].cnt & 0x00FF) | ((value as u16) << 8),
            _ => {}
        }
    }
}
