pub enum ShiftType {
    LSL = 0,
    LSR = 1,
    ASR = 2,
    ROR = 3,
}

pub fn barrel_shift(
    shift_type: ShiftType,
    amount: u32,
    val: u32,
    current_carry: bool,
    immediate: bool,
) -> (u32, bool) {
    match shift_type {
        ShiftType::LSL => {
            if amount == 0 {
                (val, current_carry)
            } else if amount < 32 {
                (val << amount, (val >> (32 - amount)) & 1 != 0)
            } else if amount == 32 {
                (0, (val & 1) != 0)
            } else {
                (0, false)
            }
        }
        ShiftType::LSR => {
            if amount == 0 {
                if immediate {
                    // LSR #0 is LSR #32
                    (0, (val & 0x8000_0000) != 0)
                } else {
                    // Register LSR #0
                    (val, current_carry)
                }
            } else if amount < 32 {
                (val >> amount, (val >> (amount - 1)) & 1 != 0)
            } else if amount == 32 {
                (0, (val & 0x8000_0000) != 0)
            } else {
                (0, false)
            }
        }
        ShiftType::ASR => {
            if amount == 0 {
                if immediate {
                    // ASR #0 is ASR #32
                    if (val & 0x8000_0000) != 0 {
                        (0xFFFF_FFFF, true)
                    } else {
                        (0, false)
                    }
                } else {
                    // Register ASR #0
                    (val, current_carry)
                }
            } else if amount < 32 {
                (
                    (val as i32 >> amount) as u32,
                    (val >> (amount - 1)) & 1 != 0,
                )
            } else {
                // amount >= 32
                if (val & 0x8000_0000) != 0 {
                    (0xFFFF_FFFF, true)
                } else {
                    (0, false)
                }
            }
        }
        ShiftType::ROR => {
            if amount == 0 {
                if immediate {
                    // ROR #0 is RRX
                    let res = (val >> 1) | (if current_carry { 0x8000_0000 } else { 0 });
                    (res, (val & 1) != 0)
                } else {
                    // Register ROR #0
                    (val, current_carry)
                }
            } else {
                let amt = amount % 32;
                if amt == 0 {
                    (val, (val & 0x8000_0000) != 0)
                } else {
                    let res = val.rotate_right(amt);
                    (res, (res >> 31) != 0)
                }
            }
        }
    }
}

pub fn check_cond(cpsr: u32, cond: u32) -> bool {
    let n = (cpsr & 0x8000_0000) != 0;
    let z = (cpsr & 0x4000_0000) != 0;
    let c = (cpsr & 0x2000_0000) != 0;
    let v = (cpsr & 0x1000_0000) != 0;
    match cond {
        0x0 => z,
        0x1 => !z,
        0x2 => c,
        0x3 => !c,
        0x4 => n,
        0x5 => !n,
        0x6 => v,
        0x7 => !v,
        0x8 => c && !z,
        0x9 => !c || z,
        0xA => n == v,
        0xB => n != v,
        0xC => !z && (n == v),
        0xD => z || (n != v),
        0xE => true,
        0xF => false,
        _ => unreachable!(),
    }
}

pub fn get_flag(cpsr: u32, bit: u32) -> bool {
    (cpsr & (1 << bit)) != 0
}
pub fn set_flag(cpsr: &mut u32, bit: u32, value: bool) {
    if value {
        *cpsr |= 1 << bit;
    } else {
        *cpsr &= !(1 << bit);
    }
}
