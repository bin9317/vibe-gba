use super::registers::PpuRegisters;

fn write_byte_to_u16(reg: &mut u16, byte: u32, value: u8) {
    let shift = (byte * 8) as u16;
    let mask = !(0x00FFu16 << shift);
    *reg = (*reg & mask) | ((value as u16) << shift);
}

fn write_byte_to_i16(reg: &mut i16, byte: u32, value: u8) {
    let mut raw = *reg as u16;
    write_byte_to_u16(&mut raw, byte, value);
    *reg = raw as i16;
}

fn sign_extend_affine_reference(value: u32) -> i32 {
    if (value & 0x0800_0000) != 0 {
        (value | 0xF000_0000) as i32
    } else {
        (value & 0x0FFF_FFFF) as i32
    }
}

fn write_affine_reference_byte(latch: &mut i32, byte: u32, value: u8) -> Option<i32> {
    let shift = byte * 8;
    let mut raw = *latch as u32;
    let mask = !(0x00FFu32 << shift);
    raw = (raw & mask) | ((value as u32) << shift);

    if byte == 3 {
        let signed = sign_extend_affine_reference(raw);
        *latch = signed;
        Some(signed)
    } else {
        *latch = raw as i32;
        None
    }
}

fn write_affine_reference_word(latch: &mut i32, value: u32) -> i32 {
    let signed = sign_extend_affine_reference(value);
    *latch = signed;
    signed
}

pub fn read_register_byte(regs: &PpuRegisters, offset: u32) -> Option<u8> {
    Some(match offset {
        0x0000 => (regs.dispcnt & 0xFF) as u8,
        0x0001 => (regs.dispcnt >> 8) as u8,
        0x0004 => (regs.dispstat & 0xFF) as u8,
        0x0005 => (regs.dispstat >> 8) as u8,
        0x0006 => (regs.vcount & 0xFF) as u8,
        0x0007 => (regs.vcount >> 8) as u8,
        0x0008 => (regs.bg0cnt & 0xFF) as u8,
        0x0009 => ((regs.bg0cnt & 0xDFFF) >> 8) as u8,
        0x000A => (regs.bg1cnt & 0xFF) as u8,
        0x000B => ((regs.bg1cnt & 0xDFFF) >> 8) as u8,
        0x000C => (regs.bg2cnt & 0xFF) as u8,
        0x000D => (regs.bg2cnt >> 8) as u8,
        0x000E => (regs.bg3cnt & 0xFF) as u8,
        0x000F => (regs.bg3cnt >> 8) as u8,
        0x0048 => (regs.winin & 0x3F3F & 0x00FF) as u8,
        0x0049 => ((regs.winin & 0x3F3F) >> 8) as u8,
        0x004A => (regs.winout & 0x3F3F & 0x00FF) as u8,
        0x004B => ((regs.winout & 0x3F3F) >> 8) as u8,
        0x004C => (regs.mosaic & 0xFF) as u8,
        0x004D => (regs.mosaic >> 8) as u8,
        0x0050 => (regs.bldcnt & 0xFF) as u8,
        0x0051 => ((regs.bldcnt & 0x3FFF) >> 8) as u8,
        0x0052 => (regs.bldalpha & 0x1F1F & 0x00FF) as u8,
        0x0053 => ((regs.bldalpha & 0x1F1F) >> 8) as u8,
        0x0054 => (regs.bldy & 0x1F) as u8,
        0x0055 => 0,
        _ => return None,
    })
}

pub fn write_register_byte(regs: &mut PpuRegisters, offset: u32, value: u8) -> bool {
    match offset {
        0x0000 | 0x0001 => write_byte_to_u16(&mut regs.dispcnt, offset & 1, value),
        0x0004 => {
            // DISPSTAT low-byte writes only affect IRQ enable bits 3-5.
            // The VCOUNT compare value lives in the high byte and must survive
            // writes to 0x0400_0004.
            let mask = 0x00F8;
            regs.dispstat = (regs.dispstat & !mask) | ((value as u16) & mask);
        }
        0x0005 => {
            let mask = 0xFF;
            regs.dispstat = (regs.dispstat & !(mask << 8)) | (((value as u16) & mask) << 8);
        }
        0x0008 | 0x0009 => write_byte_to_u16(&mut regs.bg0cnt, offset - 0x0008, value),
        0x000A | 0x000B => write_byte_to_u16(&mut regs.bg1cnt, offset - 0x000A, value),
        0x000C | 0x000D => write_byte_to_u16(&mut regs.bg2cnt, offset - 0x000C, value),
        0x000E | 0x000F => write_byte_to_u16(&mut regs.bg3cnt, offset - 0x000E, value),
        0x0010 | 0x0011 => write_byte_to_u16(&mut regs.bg0hofs, offset - 0x0010, value),
        0x0012 | 0x0013 => write_byte_to_u16(&mut regs.bg0vofs, offset - 0x0012, value),
        0x0014 | 0x0015 => write_byte_to_u16(&mut regs.bg1hofs, offset - 0x0014, value),
        0x0016 | 0x0017 => write_byte_to_u16(&mut regs.bg1vofs, offset - 0x0016, value),
        0x0018 | 0x0019 => write_byte_to_u16(&mut regs.bg2hofs, offset - 0x0018, value),
        0x001A | 0x001B => write_byte_to_u16(&mut regs.bg2vofs, offset - 0x001A, value),
        0x001C | 0x001D => write_byte_to_u16(&mut regs.bg3hofs, offset - 0x001C, value),
        0x001E | 0x001F => write_byte_to_u16(&mut regs.bg3vofs, offset - 0x001E, value),
        0x0020 | 0x0021 => write_byte_to_i16(&mut regs.bg2pa, offset - 0x0020, value),
        0x0022 | 0x0023 => write_byte_to_i16(&mut regs.bg2pb, offset - 0x0022, value),
        0x0024 | 0x0025 => write_byte_to_i16(&mut regs.bg2pc, offset - 0x0024, value),
        0x0026 | 0x0027 => write_byte_to_i16(&mut regs.bg2pd, offset - 0x0026, value),
        0x0028..=0x002B => {
            if let Some(signed) =
                write_affine_reference_byte(&mut regs.bg2x_latch, offset - 0x0028, value)
            {
                regs.bg2x = signed;
            }
        }
        0x002C..=0x002F => {
            if let Some(signed) =
                write_affine_reference_byte(&mut regs.bg2y_latch, offset - 0x002C, value)
            {
                regs.bg2y = signed;
            }
        }
        0x0038..=0x003B => {
            if let Some(signed) =
                write_affine_reference_byte(&mut regs.bg3x_latch, offset - 0x0038, value)
            {
                regs.bg3x = signed;
            }
        }
        0x003C..=0x003F => {
            if let Some(signed) =
                write_affine_reference_byte(&mut regs.bg3y_latch, offset - 0x003C, value)
            {
                regs.bg3y = signed;
            }
        }
        0x0040 | 0x0041 => write_byte_to_u16(&mut regs.win0h, offset - 0x0040, value),
        0x0042 | 0x0043 => write_byte_to_u16(&mut regs.win1h, offset - 0x0042, value),
        0x0044 | 0x0045 => write_byte_to_u16(&mut regs.win0v, offset - 0x0044, value),
        0x0046 | 0x0047 => write_byte_to_u16(&mut regs.win1v, offset - 0x0046, value),
        0x0048 | 0x0049 => write_byte_to_u16(&mut regs.winin, offset - 0x0048, value),
        0x004A | 0x004B => write_byte_to_u16(&mut regs.winout, offset - 0x004A, value),
        0x004C | 0x004D => write_byte_to_u16(&mut regs.mosaic, offset - 0x004C, value),
        0x0050 | 0x0051 => write_byte_to_u16(&mut regs.bldcnt, offset - 0x0050, value),
        0x0052 | 0x0053 => write_byte_to_u16(&mut regs.bldalpha, offset - 0x0052, value),
        0x0054 | 0x0055 => write_byte_to_u16(&mut regs.bldy, offset - 0x0054, value),
        _ => return false,
    }

    true
}

pub fn write_register_word(regs: &mut PpuRegisters, offset: u32, value: u32) -> bool {
    match offset {
        0x0000 => regs.dispcnt = (value & 0xFFFF) as u16,
        0x0004 => {
            let mask = 0xFFF8;
            regs.dispstat = (regs.dispstat & !mask) | ((value as u16) & mask);
        }
        0x0008 => {
            regs.bg0cnt = (value & 0xFFFF) as u16;
            regs.bg1cnt = (value >> 16) as u16;
        }
        0x000C => {
            regs.bg2cnt = (value & 0xFFFF) as u16;
            regs.bg3cnt = (value >> 16) as u16;
        }
        0x0010 => {
            regs.bg0hofs = (value & 0xFFFF) as u16;
            regs.bg0vofs = (value >> 16) as u16;
        }
        0x0014 => {
            regs.bg1hofs = (value & 0xFFFF) as u16;
            regs.bg1vofs = (value >> 16) as u16;
        }
        0x0018 => {
            regs.bg2hofs = (value & 0xFFFF) as u16;
            regs.bg2vofs = (value >> 16) as u16;
        }
        0x001C => {
            regs.bg3hofs = (value & 0xFFFF) as u16;
            regs.bg3vofs = (value >> 16) as u16;
        }
        0x0020 => {
            regs.bg2pa = (value & 0xFFFF) as i16;
            regs.bg2pb = (value >> 16) as i16;
        }
        0x0024 => {
            regs.bg2pc = (value & 0xFFFF) as i16;
            regs.bg2pd = (value >> 16) as i16;
        }
        0x0028 => regs.bg2x = write_affine_reference_word(&mut regs.bg2x_latch, value),
        0x002C => regs.bg2y = write_affine_reference_word(&mut regs.bg2y_latch, value),
        0x0030 => {
            regs.bg3pa = (value & 0xFFFF) as i16;
            regs.bg3pb = (value >> 16) as i16;
        }
        0x0034 => {
            regs.bg3pc = (value & 0xFFFF) as i16;
            regs.bg3pd = (value >> 16) as i16;
        }
        0x0038 => regs.bg3x = write_affine_reference_word(&mut regs.bg3x_latch, value),
        0x003C => regs.bg3y = write_affine_reference_word(&mut regs.bg3y_latch, value),
        0x0040 => {
            regs.win0h = (value & 0xFFFF) as u16;
            regs.win1h = (value >> 16) as u16;
        }
        0x0044 => {
            regs.win0v = (value & 0xFFFF) as u16;
            regs.win1v = (value >> 16) as u16;
        }
        0x0048 => {
            regs.winin = (value & 0xFFFF) as u16;
            regs.winout = (value >> 16) as u16;
        }
        0x004C => regs.mosaic = (value & 0xFFFF) as u16,
        0x0050 => {
            regs.bldcnt = (value & 0xFFFF) as u16;
            regs.bldalpha = (value >> 16) as u16;
        }
        0x0054 => regs.bldy = (value & 0xFFFF) as u16,
        _ => return false,
    }

    true
}
