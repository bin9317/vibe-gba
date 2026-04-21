use super::{
    Cpu,
    alu::{ShiftType, check_cond},
};
use crate::bus::Bus;

fn multiply_internal_cycles(rs: u32) -> u32 {
    if (rs & 0xFFFF_FF00) == 0 || (rs & 0xFFFF_FF00) == 0xFFFF_FF00 {
        1
    } else if (rs & 0xFFFF_0000) == 0 || (rs & 0xFFFF_0000) == 0xFFFF_0000 {
        2
    } else if (rs & 0xFF00_0000) == 0 || (rs & 0xFF00_0000) == 0xFF00_0000 {
        3
    } else {
        4
    }
}

pub fn execute_thumb(cpu: &mut Cpu, instr: u16, bus: &mut Bus) {
    let opcode = (instr >> 11) & 0x1F;
    match opcode {
        0b00000..=0b00010 => execute_thumb_move_shifted(cpu, instr),
        0b00011 => execute_thumb_add_sub(cpu, instr),
        0b00100..=0b00111 => execute_thumb_imm_op(cpu, instr),
        0b01000 => {
            if (instr & 0x0400) == 0 {
                execute_thumb_alu_op(cpu, instr, bus);
            } else {
                execute_thumb_hi_reg_bx(cpu, instr, bus);
            }
        }
        0b01001 => execute_thumb_ldr_pc(cpu, instr, bus),
        0b01010..=0b01011 => execute_thumb_load_store_reg(cpu, instr, bus),
        0b01100..=0b01111 => execute_thumb_load_store_imm(cpu, instr, bus),
        0b10000..=0b10001 => execute_thumb_load_store_half(cpu, instr, bus),
        0b10010..=0b10011 => execute_thumb_load_store_sp(cpu, instr, bus),
        0b10100..=0b10101 => execute_thumb_load_addr(cpu, instr),
        0b10110..=0b10111 => {
            if (instr & 0x0F00) == 0x0000 {
                execute_thumb_add_sp(cpu, instr);
            } else if (instr & 0x0600) == 0x0400 {
                execute_thumb_push_pop(cpu, instr, bus);
            } else { /* Undef */
            }
        }
        0b11000..=0b11001 => execute_thumb_ldm_stm(cpu, instr, bus),
        0b11010..=0b11011 => {
            if (instr & 0x0F00) == 0x0F00 {
                execute_thumb_swi(cpu, instr, bus);
            } else {
                execute_thumb_cond_branch(cpu, instr, bus);
            }
        }
        0b11100 => execute_thumb_uncond_branch(cpu, instr, bus),
        0b11110..=0b11111 => execute_thumb_bl(cpu, instr, bus),
        _ => {}
    }
}

fn set_flags_nz(cpu: &mut Cpu, res: u32) {
    super::alu::set_flag(&mut cpu.cpsr, 31, (res & 0x8000_0000) != 0);
    super::alu::set_flag(&mut cpu.cpsr, 30, res == 0);
}

fn execute_thumb_move_shifted(cpu: &mut Cpu, instr: u16) {
    let op = (instr >> 11) & 0x3;
    let imm = ((instr >> 6) & 0x1F) as u32;
    let rs = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;
    let val = cpu.registers[rs];
    let carry = super::alu::get_flag(cpu.cpsr, 29);
    let (res, new_c) = match op {
        0 => super::alu::barrel_shift(ShiftType::LSL, imm, val, carry, true),
        1 => super::alu::barrel_shift(ShiftType::LSR, imm, val, carry, true),
        2 => super::alu::barrel_shift(ShiftType::ASR, imm, val, carry, true),
        _ => (val, carry),
    };
    cpu.registers[rd] = res;
    set_flags_nz(cpu, res);
    super::alu::set_flag(&mut cpu.cpsr, 29, new_c);
}

fn execute_thumb_add_sub(cpu: &mut Cpu, instr: u16) {
    let is_imm = (instr & 0x0400) != 0;
    let is_sub = (instr & 0x0200) != 0;
    let imm_or_rs = ((instr >> 6) & 0x7) as usize;
    let rn = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;
    let op1 = cpu.registers[rn];
    let op2 = if is_imm {
        imm_or_rs as u32
    } else {
        cpu.registers[imm_or_rs]
    };
    let res: u32;
    if is_sub {
        let (r, b) = op1.overflowing_sub(op2);
        res = r;
        super::alu::set_flag(&mut cpu.cpsr, 29, !b);
        super::alu::set_flag(
            &mut cpu.cpsr,
            28,
            ((op1 ^ op2) & (op1 ^ res) & 0x8000_0000) != 0,
        );
    } else {
        let (r, c) = op1.overflowing_add(op2);
        res = r;
        super::alu::set_flag(&mut cpu.cpsr, 29, c);
        super::alu::set_flag(
            &mut cpu.cpsr,
            28,
            (!(op1 ^ op2) & (op1 ^ res) & 0x8000_0000) != 0,
        );
    }
    cpu.registers[rd] = res;
    set_flags_nz(cpu, res);
}

fn execute_thumb_imm_op(cpu: &mut Cpu, instr: u16) {
    let op = (instr >> 11) & 0x3;
    let rd = ((instr >> 8) & 0x7) as usize;
    let imm = (instr & 0xFF) as u32;
    let op1 = cpu.registers[rd];
    match op {
        0 => {
            cpu.registers[rd] = imm;
            set_flags_nz(cpu, imm);
        } // MOV
        1 => {
            // CMP
            let (res, b) = op1.overflowing_sub(imm);
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, !b);
            super::alu::set_flag(
                &mut cpu.cpsr,
                28,
                ((op1 ^ imm) & (op1 ^ res) & 0x8000_0000) != 0,
            );
        }
        2 => {
            // ADD
            let (res, c) = op1.overflowing_add(imm);
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, c);
            super::alu::set_flag(
                &mut cpu.cpsr,
                28,
                (!(op1 ^ imm) & (op1 ^ res) & 0x8000_0000) != 0,
            );
        }
        3 => {
            // SUB
            let (res, b) = op1.overflowing_sub(imm);
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, !b);
            super::alu::set_flag(
                &mut cpu.cpsr,
                28,
                ((op1 ^ imm) & (op1 ^ res) & 0x8000_0000) != 0,
            );
        }
        _ => {}
    }
}

fn execute_thumb_alu_op(cpu: &mut Cpu, instr: u16, bus: &mut Bus) {
    let op = (instr >> 6) & 0xF;
    let rs = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;
    let op1 = cpu.registers[rd];
    let op2 = cpu.registers[rs];
    let carry = super::alu::get_flag(cpu.cpsr, 29);
    match op {
        0x0 => {
            let res = op1 & op2;
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
        } // AND
        0x1 => {
            let res = op1 ^ op2;
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
        } // EOR
        0x2 => {
            // LSL
            let (res, c) = super::alu::barrel_shift(ShiftType::LSL, op2 & 0xFF, op1, carry, false);
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, c);
            cpu.clock_internal(bus, 1); // 1I cycle for register shift
        }
        0x3 => {
            // LSR
            let (res, c) = super::alu::barrel_shift(ShiftType::LSR, op2 & 0xFF, op1, carry, false);
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, c);
            cpu.clock_internal(bus, 1); // 1I cycle for register shift
        }
        0x4 => {
            // ASR
            let (res, c) = super::alu::barrel_shift(ShiftType::ASR, op2 & 0xFF, op1, carry, false);
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, c);
            cpu.clock_internal(bus, 1); // 1I cycle for register shift
        }
        0x5 => {
            // ADC
            let c_in = if carry { 1 } else { 0 };
            let (r1, c1) = op1.overflowing_add(op2);
            let (res, c2) = r1.overflowing_add(c_in);
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, c1 || c2);
            super::alu::set_flag(
                &mut cpu.cpsr,
                28,
                (!(op1 ^ op2) & (op1 ^ res) & 0x8000_0000) != 0,
            );
        }
        0x6 => {
            // SBC
            let c_in = if carry { 1 } else { 0 };
            let (r1, b1) = op1.overflowing_sub(op2);
            let (res, b2) = r1.overflowing_sub(1 - c_in);
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, !(b1 || b2));
            super::alu::set_flag(
                &mut cpu.cpsr,
                28,
                ((op1 ^ op2) & (op1 ^ res) & 0x8000_0000) != 0,
            );
        }
        0x7 => {
            // ROR
            let (res, c) = super::alu::barrel_shift(ShiftType::ROR, op2 & 0xFF, op1, carry, false);
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, c);
            cpu.clock_internal(bus, 1); // 1I cycle for register shift
        }
        0x8 => {
            let res = op1 & op2;
            set_flags_nz(cpu, res);
        } // TST
        0x9 => {
            // NEG
            let (res, b) = 0u32.overflowing_sub(op2);
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, !b);
            super::alu::set_flag(&mut cpu.cpsr, 28, (op2 & res & 0x8000_0000) != 0);
        }
        0xA => {
            // CMP
            let (res, b) = op1.overflowing_sub(op2);
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, !b);
            super::alu::set_flag(
                &mut cpu.cpsr,
                28,
                ((op1 ^ op2) & (op1 ^ res) & 0x8000_0000) != 0,
            );
        }
        0xB => {
            // CMN
            let (res, c) = op1.overflowing_add(op2);
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, c);
            super::alu::set_flag(
                &mut cpu.cpsr,
                28,
                (!(op1 ^ op2) & (op1 ^ res) & 0x8000_0000) != 0,
            );
        }
        0xC => {
            let res = op1 | op2;
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
        } // ORR
        0xD => {
            // MUL
            let m = multiply_internal_cycles(op2);
            // Fetch timing is modeled separately; keep only the variable MUL cycles here.
            cpu.clock_internal(bus, m);
            if bus.code_in_gamepak() {
                cpu.clock_rom_execution_penalty(bus, 2);
            }
            let res = op1.wrapping_mul(op2);
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, false); // Carry undefined
        }
        0xE => {
            let res = op1 & (!op2);
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
        } // BIC
        0xF => {
            let res = !op2;
            cpu.registers[rd] = res;
            set_flags_nz(cpu, res);
        } // MVN
        _ => {}
    }
}

fn execute_thumb_hi_reg_bx(cpu: &mut Cpu, instr: u16, _bus: &mut Bus) {
    let op = (instr >> 8) & 0x3;
    let h1 = (instr >> 7) & 1;
    let h2 = (instr >> 6) & 1;
    let rs = (((instr >> 3) & 0x7) + (h2 << 3)) as usize;
    let rd = ((instr & 0x7) + (h1 << 3)) as usize;
    let val_s = if rs == 15 {
        cpu.registers[15].wrapping_add(2)
    } else {
        cpu.registers[rs]
    };
    let val_d = if rd == 15 {
        cpu.registers[15].wrapping_add(2)
    } else {
        cpu.registers[rd]
    };
    match op {
        0x0 => {
            cpu.registers[rd] = val_d.wrapping_add(val_s);
            if rd == 15 {
                cpu.registers[15] &= !1;
                cpu.invalidate_pipeline();
            }
        } // ADD
        0x1 => {
            // CMP
            let (res, b) = val_d.overflowing_sub(val_s);
            set_flags_nz(cpu, res);
            super::alu::set_flag(&mut cpu.cpsr, 29, !b);
            super::alu::set_flag(
                &mut cpu.cpsr,
                28,
                ((val_d ^ val_s) & (val_d ^ res) & 0x8000_0000) != 0,
            );
        }
        0x2 => {
            cpu.registers[rd] = val_s;
            if rd == 15 {
                cpu.registers[15] &= !1;
                cpu.invalidate_pipeline();
            }
        } // MOV
        0x3 => {
            // BX
            let new_pc;
            if (val_s & 1) != 0 {
                cpu.set_cpsr(cpu.cpsr | 0x20);
                new_pc = val_s & !1;
            } else {
                cpu.set_cpsr(cpu.cpsr & !0x20);
                new_pc = val_s & !3;
            }
            cpu.registers[15] = new_pc;
            cpu.invalidate_pipeline();
        }
        _ => {}
    }
}

fn execute_thumb_ldr_pc(cpu: &mut Cpu, instr: u16, bus: &mut Bus) {
    let rd = ((instr >> 8) & 0x7) as usize;
    let imm = (instr & 0xFF) as u32;
    let pc = (cpu.registers[15].wrapping_add(2)) & !3; // PC read is (PC+4) masked
    cpu.registers[rd] = bus.read32(pc.wrapping_add(imm << 2));
    cpu.clock_internal(bus, 1); // 1I cycle
}

fn execute_thumb_load_store_reg(cpu: &mut Cpu, instr: u16, bus: &mut Bus) {
    let op = (instr >> 9) & 0x7;
    let ro = ((instr >> 6) & 0x7) as usize;
    let rb = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;
    let addr = cpu.registers[rb].wrapping_add(cpu.registers[ro]);
    match op {
        0 => {
            bus.write32(addr & !3, cpu.registers[rd]);
            if bus.code_in_gamepak() {
                cpu.clock_rom_execution_penalty_after_memory(bus, 2);
            }
        } // STR
        1 => {
            bus.write16(addr, cpu.registers[rd] as u16);
            if bus.code_in_gamepak() {
                cpu.clock_rom_execution_penalty_after_memory(bus, 2);
            }
        } // STRH
        2 => {
            bus.write8(addr, cpu.registers[rd] as u8);
            if bus.code_in_gamepak() {
                cpu.clock_rom_execution_penalty_after_memory(bus, 2);
            }
        } // STRB
        3 => {
            // LDRSB
            cpu.registers[rd] = bus.read8(addr) as i8 as i32 as u32;
            cpu.clock_internal(bus, 1);
            if bus.code_in_gamepak() {
                cpu.clock_rom_execution_penalty_after_memory(bus, 2);
            }
        }
        4 => {
            // LDR
            let region = addr >> 24;
            cpu.registers[rd] = if matches!(region, 0x0E | 0x0F) {
                bus.read32(addr)
            } else {
                let data = bus.read32(addr & !3);
                if (addr & 3) != 0 && region != 4 {
                    data.rotate_right((addr & 3) * 8)
                } else {
                    data
                }
            };
            cpu.clock_internal(bus, 1);
            if bus.code_in_gamepak() {
                cpu.clock_rom_execution_penalty_after_memory(bus, 2);
            }
        }
        5 => {
            // LDRH
            let data = if matches!(addr >> 24, 0x0E | 0x0F) {
                bus.read16(addr)
            } else {
                bus.read16(addr & !1)
            };
            cpu.registers[rd] = if (addr & 1) != 0 {
                (data as u32).rotate_right(8)
            } else {
                data as u32
            };
            cpu.clock_internal(bus, 1);
            if bus.code_in_gamepak() {
                cpu.clock_rom_execution_penalty_after_memory(bus, 2);
            }
        }
        6 => {
            // LDRB
            cpu.registers[rd] = bus.read8(addr) as u32;
            cpu.clock_internal(bus, 1);
            if bus.code_in_gamepak() {
                cpu.clock_rom_execution_penalty_after_memory(bus, 2);
            }
        }
        7 => {
            // LDRSH
            cpu.registers[rd] = if (addr & 1) != 0 {
                bus.read8(addr) as i8 as i32 as u32
            } else {
                bus.read16(addr) as i16 as i32 as u32
            };
            cpu.clock_internal(bus, 1);
            if bus.code_in_gamepak() {
                cpu.clock_rom_execution_penalty_after_memory(bus, 2);
            }
        }
        _ => {}
    }
}

fn execute_thumb_load_store_imm(cpu: &mut Cpu, instr: u16, bus: &mut Bus) {
    let b = (instr >> 12) & 1;
    let l = (instr >> 11) & 1;
    let imm = ((instr >> 6) & 0x1F) as u32;
    let rb = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;
    let addr = cpu.registers[rb].wrapping_add(if b == 1 { imm } else { imm << 2 });
    if l == 1 {
        if b == 1 {
            cpu.registers[rd] = bus.read8(addr) as u32;
        } else {
            let region = addr >> 24;
            cpu.registers[rd] = if matches!(region, 0x0E | 0x0F) {
                bus.read32(addr)
            } else {
                let data = bus.read32(addr & !3);
                if (addr & 3) != 0 && region != 4 {
                    data.rotate_right((addr & 3) * 8)
                } else {
                    data
                }
            };
        }
        cpu.clock_internal(bus, 1);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    } else {
        if b == 1 {
            bus.write8(addr, cpu.registers[rd] as u8);
        } else {
            bus.write32(addr & !3, cpu.registers[rd]);
        }
        bus.clock(0);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    }
}

fn execute_thumb_load_store_half(cpu: &mut Cpu, instr: u16, bus: &mut Bus) {
    let l = (instr >> 11) & 1;
    let imm = ((instr >> 6) & 0x1F) as u32;
    let rb = ((instr >> 3) & 0x7) as usize;
    let rd = (instr & 0x7) as usize;
    let addr = cpu.registers[rb].wrapping_add(imm << 1);
    if l == 1 {
        let data = if matches!(addr >> 24, 0x0E | 0x0F) {
            bus.read16(addr)
        } else {
            bus.read16(addr & !1)
        };
        cpu.registers[rd] = if (addr & 1) != 0 {
            (data as u32).rotate_right(8)
        } else {
            data as u32
        };
        cpu.clock_internal(bus, 1);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    } else {
        bus.write16(addr, cpu.registers[rd] as u16);
        bus.clock(0);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    }
}

fn execute_thumb_load_store_sp(cpu: &mut Cpu, instr: u16, bus: &mut Bus) {
    let l = (instr >> 11) & 1;
    let rd = ((instr >> 8) & 0x7) as usize;
    let imm = (instr & 0xFF) as u32;
    let addr = cpu.registers[13].wrapping_add(imm << 2);
    if l == 1 {
        let region = addr >> 24;
        cpu.registers[rd] = if matches!(region, 0x0E | 0x0F) {
            bus.read32(addr)
        } else {
            let data = bus.read32(addr & !3);
            if (addr & 3) != 0 && region != 4 {
                data.rotate_right((addr & 3) * 8)
            } else {
                data
            }
        };
        cpu.clock_internal(bus, 1);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    } else {
        bus.write32(addr & !3, cpu.registers[rd]);
        bus.clock(0);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    }
}

fn execute_thumb_load_addr(cpu: &mut Cpu, instr: u16) {
    let sp = (instr >> 11) & 1;
    let rd = ((instr >> 8) & 0x7) as usize;
    let imm = (instr & 0xFF) as u32;
    if sp == 1 {
        cpu.registers[rd] = cpu.registers[13].wrapping_add(imm << 2);
    } else {
        cpu.registers[rd] = (cpu.registers[15].wrapping_add(2) & !3).wrapping_add(imm << 2);
    }
}

fn execute_thumb_add_sp(cpu: &mut Cpu, instr: u16) {
    let s = (instr & 0x0080) != 0;
    let imm = (instr & 0x7F) as u32;
    if s {
        cpu.registers[13] = cpu.registers[13].wrapping_sub(imm << 2);
    } else {
        cpu.registers[13] = cpu.registers[13].wrapping_add(imm << 2);
    }
}

fn execute_thumb_push_pop(cpu: &mut Cpu, instr: u16, bus: &mut Bus) {
    let l = (instr >> 11) & 1;
    let pc_lr = (instr >> 8) & 1;
    let reg_list = (instr & 0xFF) as u8;
    let mut count = 0;
    for i in 0..8 {
        if (reg_list & (1 << i)) != 0 {
            count += 1;
        }
    }
    if pc_lr == 1 {
        count += 1;
    }
    let mut curr = if l == 1 {
        cpu.registers[13]
    } else {
        cpu.registers[13].wrapping_sub(count * 4)
    };
    let start = curr;
    for i in 0..8 {
        if (reg_list & (1 << i)) != 0 {
            let access_addr = if matches!(curr >> 24, 0x0E | 0x0F) {
                curr
            } else {
                curr & !3
            };
            if l == 1 {
                cpu.registers[i] = bus.read32(access_addr);
            } else {
                bus.write32(access_addr, cpu.registers[i]);
            }
            curr = curr.wrapping_add(4);
        }
    }
    if pc_lr == 1 {
        if l == 1 {
            let access_addr = if matches!(curr >> 24, 0x0E | 0x0F) {
                curr
            } else {
                curr & !3
            };
            let target = bus.read32(access_addr);
            cpu.set_cpsr(cpu.cpsr | 0x20);
            cpu.registers[15] = target & !1;

            cpu.invalidate_pipeline();
        } else {
            let access_addr = if matches!(curr >> 24, 0x0E | 0x0F) {
                curr
            } else {
                curr & !3
            };
            bus.write32(access_addr, cpu.registers[14]);
        }
        curr = curr.wrapping_add(4);
    }
    if l == 1 {
        cpu.clock_internal(bus, 1);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
        cpu.registers[13] = curr;
    } else {
        cpu.registers[13] = start;
    }
}

fn execute_thumb_ldm_stm(cpu: &mut Cpu, instr: u16, bus: &mut Bus) {
    let l = (instr >> 11) & 1;
    let rb = ((instr >> 8) & 0x7) as usize;
    let reg_list = (instr & 0xFF) as u8;
    if reg_list == 0 {
        let base = cpu.registers[rb];
        if l == 1 {
            let access_addr = if matches!(base >> 24, 0x0E | 0x0F) {
                base
            } else {
                base & !3
            };
            let target = bus.read32(access_addr);
            cpu.registers[rb] = base.wrapping_add(0x40);
            cpu.clock_internal(bus, 1);
            cpu.set_cpsr(cpu.cpsr | 0x20);
            cpu.registers[15] = target & !1;
            cpu.invalidate_pipeline();
        } else {
            let pc_value = cpu.registers[15].wrapping_add(4);
            let access_addr = if matches!(base >> 24, 0x0E | 0x0F) {
                base
            } else {
                base & !3
            };
            bus.write32(access_addr, pc_value);
            cpu.registers[rb] = base.wrapping_add(0x40);
        }
        return;
    }

    let count = reg_list.count_ones() as u32;
    let writeback_base = cpu.registers[rb].wrapping_add(count * 4);
    let mut curr = cpu.registers[rb];
    let first_reg_in_list = (0..8).find(|i| (reg_list & (1 << i)) != 0);
    for i in 0..8 {
        if (reg_list & (1 << i)) != 0 {
            let access_addr = if matches!(curr >> 24, 0x0E | 0x0F) {
                curr
            } else {
                curr & !3
            };
            if l == 1 {
                cpu.registers[i] = bus.read32(access_addr);
            } else {
                let value = if i == rb && first_reg_in_list != Some(rb) {
                    writeback_base
                } else {
                    cpu.registers[i]
                };
                bus.write32(access_addr, value);
            }
            curr = curr.wrapping_add(4);
        }
    }
    if l == 1 {
        cpu.clock_internal(bus, 1);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    } else {
        bus.clock(0);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    }
    cpu.registers[rb] = curr;
}

fn execute_thumb_cond_branch(cpu: &mut Cpu, instr: u16, _bus: &mut Bus) {
    let cond = ((instr >> 8) & 0xF) as u32;
    if check_cond(cpu.cpsr, cond) {
        let offset = (instr & 0xFF) as i8 as i32;
        let new_pc = cpu.registers[15]
            .wrapping_add(2)
            .wrapping_add((offset << 1) as u32);

        cpu.registers[15] = new_pc;
        cpu.invalidate_pipeline();
    }
}

fn execute_thumb_swi(cpu: &mut Cpu, _instr: u16, _bus: &mut Bus) {
    trace_thumb_swi(cpu.registers[15].wrapping_sub(2), (_instr & 0x00FF) as u32);
    cpu.spsr_svc = cpu.cpsr;
    cpu.set_cpsr((cpu.cpsr & !0x3F) | 0x93);
    cpu.registers[14] = cpu.registers[15];

    cpu.registers[15] = 0x0000_0008;
    cpu.invalidate_pipeline();
}

fn trace_thumb_swi(caller_pc: u32, swi_number: u32) {
    use std::sync::OnceLock;

    static TRACE_SWI: OnceLock<bool> = OnceLock::new();
    if !*TRACE_SWI.get_or_init(|| std::env::var_os("VIBE_TRACE_SWI").is_some()) {
        return;
    }

    eprintln!("[swi] mode=THUMB caller={caller_pc:08X} num={swi_number:02X}");
}

fn execute_thumb_uncond_branch(cpu: &mut Cpu, instr: u16, _bus: &mut Bus) {
    let mut offset = (instr & 0x7FF) as i32;
    if (offset & 0x400) != 0 {
        offset |= !0x7FF;
    }
    let new_pc = cpu.registers[15]
        .wrapping_add(2)
        .wrapping_add((offset << 1) as u32);

    cpu.registers[15] = new_pc;
    cpu.invalidate_pipeline();
}

fn execute_thumb_bl(cpu: &mut Cpu, instr: u16, _bus: &mut Bus) {
    let setup = (instr & 0x0800) == 0;
    let offset = (instr & 0x7FF) as u32;
    if setup {
        let mut off = offset;
        if (off & 0x400) != 0 {
            off |= 0xFFFFF800;
        }
        cpu.registers[14] = cpu.registers[15].wrapping_add(2).wrapping_add(off << 12);
    } else {
        let next_pc = cpu.registers[15] | 1;
        let target = cpu.registers[14].wrapping_add(offset << 1);
        cpu.registers[14] = next_pc;

        cpu.registers[15] = target & !1;
        cpu.invalidate_pipeline();
    }
}
