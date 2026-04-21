use crate::bus::{
    REGION_EWRAM, REGION_IO, REGION_IWRAM, REGION_OAM, REGION_PALETTE, REGION_ROM_WS0, REGION_SRAM,
    REGION_SRAM_MIRROR, REGION_VRAM,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessKind {
    OpcodeFetch,
    DataRead,
    DataWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessWidth {
    Byte,
    Halfword,
    Word,
}

impl AccessWidth {
    pub fn is_32bit(self) -> bool {
        matches!(self, Self::Word)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessDescriptor {
    pub addr: u32,
    pub kind: AccessKind,
    pub width: AccessWidth,
    pub sequential: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessTiming {
    pub cycles: u32,
    pub uses_gamepak_bus: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessTimingConfig {
    pub waitcnt: u16,
    pub internal_memory_control: u32,
    pub dispcnt: u16,
    pub current_scanline: u16,
}

pub fn lookup_access_timing(config: AccessTimingConfig, desc: AccessDescriptor) -> AccessTiming {
    let region = desc.addr >> 24;
    let is_32bit = desc.width.is_32bit();
    let cycles = match region {
        0x00 => 1,
        REGION_EWRAM => {
            let wram_wait = (config.internal_memory_control >> 24) & 0xF;
            let s = if wram_wait == 0xE { 2 } else { 3 };
            if is_32bit { s + s } else { s }
        }
        REGION_IWRAM => 1,
        REGION_IO => 1,
        REGION_PALETTE | REGION_VRAM => {
            let mut c = if is_32bit { 2 } else { 1 };
            if (config.dispcnt & 0x0080) == 0 && config.current_scanline < 160 {
                c += if is_32bit { 2 } else { 1 };
            }
            c
        }
        REGION_OAM => {
            let mut c = 1;
            if (config.dispcnt & 0x0080) == 0 && config.current_scanline < 160 {
                c += 1;
            }
            c
        }
        REGION_ROM_WS0..=0x0D => gamepak_cycles(config.waitcnt, region, is_32bit, desc.sequential),
        REGION_SRAM | REGION_SRAM_MIRROR => match config.waitcnt & 0x3 {
            0 => 4,
            1 => 3,
            2 => 2,
            _ => 8,
        },
        _ => 1,
    };

    let uses_gamepak_bus = matches!(
        region,
        REGION_ROM_WS0..=0x0D | REGION_SRAM | REGION_SRAM_MIRROR
    );

    let _ = desc.kind;

    AccessTiming {
        cycles,
        uses_gamepak_bus,
    }
}

pub fn gamepak_wait_components(waitcnt: u16, region: u32) -> (u32, u32) {
    let ws_idx = (region - REGION_ROM_WS0) / 2;
    match ws_idx {
        0 => (
            match (waitcnt >> 2) & 0x3 {
                0 => 4,
                1 => 3,
                2 => 2,
                _ => 8,
            },
            if (waitcnt & 0x10) != 0 { 1 } else { 2 },
        ),
        1 => (
            match (waitcnt >> 5) & 0x3 {
                0 => 4,
                1 => 3,
                2 => 2,
                _ => 8,
            },
            if (waitcnt & 0x80) != 0 { 1 } else { 4 },
        ),
        _ => (
            match (waitcnt >> 8) & 0x3 {
                0 => 4,
                1 => 3,
                2 => 2,
                _ => 8,
            },
            if (waitcnt & 0x0400) != 0 { 1 } else { 8 },
        ),
    }
}

fn gamepak_cycles(waitcnt: u16, region: u32, is_32bit: bool, sequential: bool) -> u32 {
    let (n_wait, s_wait) = gamepak_wait_components(waitcnt, region);
    let n = n_wait + 1;
    let s = s_wait + 1;
    if is_32bit {
        if sequential { s + s } else { n + s }
    } else if sequential {
        s
    } else {
        n
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AccessDescriptor, AccessKind, AccessTimingConfig, AccessWidth, lookup_access_timing,
    };

    fn base_config() -> AccessTimingConfig {
        AccessTimingConfig {
            waitcnt: 0,
            internal_memory_control: 0,
            dispcnt: 0x0080,
            current_scanline: 0,
        }
    }

    #[test]
    fn ws0_rom_halfword_uses_non_sequential_cost_by_default() {
        let timing = lookup_access_timing(
            base_config(),
            AccessDescriptor {
                addr: 0x0800_0000,
                kind: AccessKind::OpcodeFetch,
                width: AccessWidth::Halfword,
                sequential: false,
            },
        );

        assert_eq!(timing.cycles, 5);
        assert!(timing.uses_gamepak_bus);
    }

    #[test]
    fn ws0_rom_halfword_uses_sequential_cost_when_requested() {
        let timing = lookup_access_timing(
            base_config(),
            AccessDescriptor {
                addr: 0x0800_0002,
                kind: AccessKind::OpcodeFetch,
                width: AccessWidth::Halfword,
                sequential: true,
            },
        );

        assert_eq!(timing.cycles, 3);
        assert!(timing.uses_gamepak_bus);
    }

    #[test]
    fn ewram_word_uses_configured_wait_control() {
        let mut config = base_config();
        config.internal_memory_control = 0x0E00_0000;

        let timing = lookup_access_timing(
            config,
            AccessDescriptor {
                addr: 0x0200_0000,
                kind: AccessKind::DataRead,
                width: AccessWidth::Word,
                sequential: false,
            },
        );

        assert_eq!(timing.cycles, 4);
        assert!(!timing.uses_gamepak_bus);
    }
}
