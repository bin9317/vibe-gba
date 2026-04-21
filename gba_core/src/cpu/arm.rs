use super::{Cpu, alu::check_cond};
use crate::bus::Bus;

pub fn execute_arm(cpu: &mut Cpu, instr: u32, bus: &mut Bus) {
    let cond = instr >> 28;
    if !check_cond(cpu.cpsr, cond) {
        return;
    }
    let op1 = (instr >> 25) & 0b111;
    let op2 = (instr >> 4) & 0b1;
    match op1 {
        0b000 => {
            if (instr & 0x0FFFFFF0) == 0x012FFF10 {
                execute_bx(cpu, instr, bus);
                return;
            }
            if op2 == 1 && ((instr >> 7) & 1) == 1 {
                let op_bits_6_5 = (instr >> 5) & 0b11;
                if op_bits_6_5 == 0b00 {
                    if ((instr >> 24) & 1) == 0 {
                        execute_multiply(cpu, instr, bus);
                    } else {
                        // SWP / SWPB
                        let rn = ((instr >> 16) & 0xF) as usize;
                        let rd = ((instr >> 12) & 0xF) as usize;
                        let rm = (instr & 0xF) as usize;
                        let is_byte = (instr >> 22) & 1;
                        // SWP base Rn is always PC + 8
                        let addr = if rn == 15 {
                            cpu.registers[15].wrapping_add(4)
                        } else {
                            cpu.registers[rn]
                        };
                        let rm_val = if rm == 15 {
                            cpu.registers[15].wrapping_add(4)
                        } else {
                            cpu.registers[rm]
                        };

                        if is_byte == 1 {
                            let val = bus.read8(addr) as u32;
                            bus.write8(addr, rm_val as u8);
                            cpu.registers[rd] = val;
                        } else {
                            // SWP word also handles rotation on unaligned addresses
                            let data = bus.read32(addr & !3);
                            let region = addr >> 24;
                            let val = if (addr & 3) != 0 && region != 4 {
                                data.rotate_right((addr & 3) * 8)
                            } else {
                                data
                            };
                            bus.write32(addr & !3, rm_val);
                            cpu.registers[rd] = val;
                        }
                        cpu.clock_internal(bus, 1); // 1I cycle
                    }
                } else {
                    execute_halfword_transfer(cpu, instr, bus);
                }
            } else {
                execute_data_processing(cpu, instr, bus);
            }
        }
        0b001 => {
            execute_data_processing(cpu, instr, bus);
        }
        0b010 => {
            execute_load_store(cpu, instr, bus);
        }
        0b011 => {
            if (instr & 0x00000010) == 0x00000010 { /* Undef */
            } else {
                execute_load_store(cpu, instr, bus);
            }
        }
        0b100 => {
            execute_ldm_stm(cpu, instr, bus);
        }
        0b101 => {
            execute_branch(cpu, instr, bus);
        }
        0b111 => {
            if (instr & 0x0F000000) == 0x0F000000 {
                execute_swi(cpu, instr, bus);
            }
        }
        _ => {}
    }
}

fn execute_data_processing(cpu: &mut Cpu, instr: u32, bus: &mut Bus) {
    let i = (instr >> 25) & 1;
    let opcode = (instr >> 21) & 0xF;
    let s = (instr >> 20) & 1;
    let rn = ((instr >> 16) & 0xF) as usize;
    let rd = ((instr >> 12) & 0xF) as usize;
    if s == 0 && opcode >= 8 && opcode <= 11 {
        execute_psr_transfer(cpu, instr, bus);
        return;
    }

    // R15 offset: PC+12 for register-specified shifts, PC+8 otherwise.
    // Note: cpu.registers[15] is already PC+4.
    let is_reg_shift = i == 0 && (instr & 0x10) != 0;
    let pc_offset = if is_reg_shift { 8 } else { 4 };

    let old_carry = crate::cpu::alu::get_flag(cpu.cpsr, 29);
    let mut shifter_carry = old_carry;
    let op1 = if rn == 15 {
        cpu.registers[15].wrapping_add(pc_offset)
    } else {
        cpu.registers[rn]
    };
    let op2 = if i == 1 {
        let imm = instr & 0xFF;
        let rotate = ((instr >> 8) & 0xF) * 2;
        let res = imm.rotate_right(rotate);
        if rotate != 0 {
            shifter_carry = (res >> 31) != 0;
        }
        res
    } else {
        let rm = (instr & 0xF) as usize;
        let val = if rm == 15 {
            cpu.registers[15].wrapping_add(pc_offset)
        } else {
            cpu.registers[rm]
        };
        let shift_type = match (instr >> 5) & 0x3 {
            0 => crate::cpu::alu::ShiftType::LSL,
            1 => crate::cpu::alu::ShiftType::LSR,
            2 => crate::cpu::alu::ShiftType::ASR,
            _ => crate::cpu::alu::ShiftType::ROR,
        };
        let (shift_amount, is_imm_shift) = if (instr & 0x10) == 0 {
            (((instr >> 7) & 0x1F), true)
        } else {
            cpu.clock_internal(bus, 1);
            (cpu.registers[((instr >> 8) & 0xF) as usize] & 0xFF, false)
        };
        let (res, new_carry) =
            crate::cpu::alu::barrel_shift(shift_type, shift_amount, val, old_carry, is_imm_shift);
        shifter_carry = new_carry;
        res
    };
    let result: u32;
    let mut write_back = true;
    let mut carry = shifter_carry;
    let mut v_flag = crate::cpu::alu::get_flag(cpu.cpsr, 28);
    match opcode {
        0x0 => {
            result = op1 & op2;
        }
        0x1 => {
            result = op1 ^ op2;
        }
        0x2 => {
            // SUB
            let (res, b) = op1.overflowing_sub(op2);
            result = res;
            carry = !b;
            v_flag = ((op1 ^ op2) & (op1 ^ res) & 0x8000_0000) != 0;
        }
        0x3 => {
            // RSB
            let (res, b) = op2.overflowing_sub(op1);
            result = res;
            carry = !b;
            v_flag = ((op2 ^ op1) & (op2 ^ res) & 0x8000_0000) != 0;
        }
        0x4 => {
            // ADD
            let (res, c) = op1.overflowing_add(op2);
            result = res;
            carry = c;
            v_flag = (!(op1 ^ op2) & (op1 ^ res) & 0x8000_0000) != 0;
        }
        0x5 => {
            // ADC
            let c_in = if old_carry { 1u64 } else { 0u64 };
            let res_64 = (op1 as u64).wrapping_add(op2 as u64).wrapping_add(c_in);
            result = res_64 as u32;
            carry = res_64 > 0xFFFFFFFF;
            v_flag = (!(op1 ^ op2) & (op1 ^ result) & 0x8000_0000) != 0;
        }
        0x6 => {
            // SBC
            let c_in = if old_carry { 1u64 } else { 0u64 };
            let res_64 = (op1 as u64).wrapping_sub(op2 as u64).wrapping_sub(1 - c_in);
            result = res_64 as u32;
            carry = res_64 <= 0xFFFFFFFF;
            v_flag = ((op1 ^ op2) & (op1 ^ result) & 0x8000_0000) != 0;
        }
        0x7 => {
            // RSC
            let c_in = if old_carry { 1u64 } else { 0u64 };
            let res_64 = (op2 as u64).wrapping_sub(op1 as u64).wrapping_sub(1 - c_in);
            result = res_64 as u32;
            carry = res_64 <= 0xFFFFFFFF;
            v_flag = ((op2 ^ op1) & (op2 ^ result) & 0x8000_0000) != 0;
        }
        0x8 => {
            result = op1 & op2;
            write_back = false;
        }
        0x9 => {
            result = op1 ^ op2;
            write_back = false;
        }
        0xA => {
            // CMP
            let (res, b) = op1.overflowing_sub(op2);
            result = res;
            carry = !b;
            v_flag = ((op1 ^ op2) & (op1 ^ res) & 0x8000_0000) != 0;
            write_back = false;
        }
        0xB => {
            // CMN
            let (res, c) = op1.overflowing_add(op2);
            result = res;
            carry = c;
            v_flag = (!(op1 ^ op2) & (op1 ^ res) & 0x8000_0000) != 0;
            write_back = false;
        }
        0xC => {
            result = op1 | op2;
        }
        0xD => {
            result = op2;
        }
        0xE => {
            result = op1 & (!op2);
        }
        0xF => {
            result = !op2;
        }
        _ => unreachable!(),
    }
    let psr_transfer_like = s == 1 && rd == 15 && (0x8..=0xB).contains(&opcode);
    if psr_transfer_like {
        cpu.set_cpsr(cpu.get_spsr());
        return;
    }
    if s == 1 {
        if rd == 15 && write_back {
            let spsr = cpu.get_spsr();
            cpu.set_cpsr(spsr);
        } else {
            crate::cpu::alu::set_flag(&mut cpu.cpsr, 31, (result & 0x8000_0000) != 0);
            crate::cpu::alu::set_flag(&mut cpu.cpsr, 30, result == 0);
            crate::cpu::alu::set_flag(&mut cpu.cpsr, 29, carry);
            crate::cpu::alu::set_flag(&mut cpu.cpsr, 28, v_flag);
        }
    }

    if write_back {
        if rd == 15 {
            // Alignment based on NEW mode
            let new_pc = if cpu.get_thumb_mode() {
                result & !1
            } else {
                result & !3
            };
            cpu.registers[15] = new_pc;
            cpu.invalidate_pipeline();
        } else {
            cpu.registers[rd] = result;
        }
    }
}

fn execute_psr_transfer(cpu: &mut Cpu, instr: u32, _bus: &mut Bus) {
    let psr = (instr >> 22) & 1;
    let op = (instr >> 21) & 1;
    if op == 0 {
        let rd = ((instr >> 12) & 0xF) as usize;
        cpu.registers[rd] = if psr == 0 { cpu.cpsr } else { cpu.get_spsr() };
    } else {
        let i = (instr >> 25) & 1;
        let val = if i == 1 {
            let imm = instr & 0xFF;
            let rotate = ((instr >> 8) & 0xF) * 2;
            imm.rotate_right(rotate)
        } else {
            cpu.registers[(instr & 0xF) as usize]
        };

        let mut mask = 0;
        if (instr & (1 << 19)) != 0 {
            mask |= 0xFF000000;
        } // f
        if (instr & (1 << 18)) != 0 {
            mask |= 0x00FF0000;
        } // s
        if (instr & (1 << 17)) != 0 {
            mask |= 0x0000FF00;
        } // x
        if (instr & (1 << 16)) != 0 {
            mask |= 0x000000FF;
        } // c

        // In user mode, only the flags (f) can be modified
        if (cpu.cpsr & 0x1F) == 0x10 {
            mask &= 0xFF000000;
        }

        if psr == 0 {
            cpu.set_cpsr((cpu.cpsr & !mask) | (val & mask));
        } else {
            let old = cpu.get_spsr();
            cpu.set_spsr((old & !mask) | (val & mask));
        }
    }
}

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

fn execute_multiply(cpu: &mut Cpu, instr: u32, bus: &mut Bus) {
    let mul_long = (instr >> 23) & 1;
    let s = (instr >> 20) & 1;
    let rm = cpu.registers[(instr & 0xF) as usize];
    let rs = cpu.registers[((instr >> 8) & 0xF) as usize];
    if mul_long == 0 {
        let a = (instr >> 21) & 1;
        let rd = ((instr >> 16) & 0xF) as usize;
        let rn = cpu.registers[((instr >> 12) & 0xF) as usize];
        let m = multiply_internal_cycles(rs);
        // Fetch timing is modeled separately; keep only the variable internal MUL cycles here.
        cpu.clock_internal(bus, m);
        if a == 1 {
            cpu.clock_internal(bus, 1);
        }
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty(bus, 2);
        }
        let res = rm
            .wrapping_mul(rs)
            .wrapping_add(if a == 1 { rn } else { 0 });
        cpu.registers[rd] = res;
        if s == 1 {
            crate::cpu::alu::set_flag(&mut cpu.cpsr, 31, (res & 0x8000_0000) != 0);
            crate::cpu::alu::set_flag(&mut cpu.cpsr, 30, res == 0);
        }
    } else {
        let u = (instr >> 22) & 1;
        let a = (instr >> 21) & 1;
        let rd_hi = ((instr >> 16) & 0xF) as usize;
        let rd_lo = ((instr >> 12) & 0xF) as usize;
        let m = multiply_internal_cycles(rs);
        // Long multiplies cost one extra internal cycle beyond plain MUL timing.
        cpu.clock_internal(bus, m + 1);
        if a == 1 {
            cpu.clock_internal(bus, 1);
        }
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty(bus, 2);
        }
        let mut res: u64 = if u == 1 {
            (rm as i32 as i64).wrapping_mul(rs as i32 as i64) as u64
        } else {
            (rm as u64).wrapping_mul(rs as u64)
        };
        if a == 1 {
            res = res.wrapping_add(
                ((cpu.registers[rd_hi] as u64) << 32) | (cpu.registers[rd_lo] as u64),
            );
        }
        cpu.registers[rd_lo] = (res & 0xFFFFFFFF) as u32;
        cpu.registers[rd_hi] = (res >> 32) as u32;
        if s == 1 {
            crate::cpu::alu::set_flag(&mut cpu.cpsr, 31, (res & 0x8000_0000_0000_0000) != 0);
            crate::cpu::alu::set_flag(&mut cpu.cpsr, 30, res == 0);
        }
    }
}

fn execute_halfword_transfer(cpu: &mut Cpu, instr: u32, bus: &mut Bus) {
    let p = (instr >> 24) & 1;
    let u = (instr >> 23) & 1;
    let i = (instr >> 22) & 1;
    let w = (instr >> 21) & 1;
    let l = (instr >> 20) & 1;
    let rn = ((instr >> 16) & 0xF) as usize;
    let rd = ((instr >> 12) & 0xF) as usize;
    let op = (instr >> 5) & 0x3;
    let rm = (instr & 0xF) as usize;
    let offset = if i == 1 {
        ((instr >> 8) & 0xF) << 4 | (instr & 0xF)
    } else {
        if rm == 15 {
            cpu.registers[15].wrapping_add(4)
        } else {
            cpu.registers[rm]
        }
    };

    // LDR/STR base Rn is always PC + 8
    let base = if rn == 15 {
        cpu.registers[15].wrapping_add(4)
    } else {
        cpu.registers[rn]
    };
    let addr = if p == 1 {
        if u == 1 {
            base.wrapping_add(offset)
        } else {
            base.wrapping_sub(offset)
        }
    } else {
        base
    };

    if l == 1 {
        let val = match op {
            1 => {
                // LDRH (Unsigned Halfword)
                let data = if matches!(addr >> 24, 0x0E | 0x0F) {
                    bus.read16(addr)
                } else {
                    bus.read16(addr & !1)
                };
                if (addr & 1) != 0 {
                    (data as u32).rotate_right(8)
                } else {
                    data as u32
                }
            }
            2 => bus.read8(addr) as i8 as i32 as u32, // LDRSB (Signed Byte)
            3 => {
                // LDRSH (Signed Halfword)
                if (addr & 1) != 0 {
                    bus.read8(addr) as i8 as i32 as u32 // Becomes LDRSB at odd addresses
                } else {
                    bus.read16(addr) as i16 as i32 as u32
                }
            }
            _ => 0,
        };
        cpu.registers[rd] = val;
        // Halfword/signed loads incur a single post-read internal cycle.
        cpu.clock_internal(bus, 1);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    } else {
        let val = if rd == 15 {
            cpu.registers[15].wrapping_add(8)
        } else {
            cpu.registers[rd]
        };
        if op == 1 {
            bus.write16(addr, val as u16);
        }
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    }

    if w == 1 || p == 0 {
        let new_base = if u == 1 {
            base.wrapping_add(offset)
        } else {
            base.wrapping_sub(offset)
        };
        if l == 0 || rd != rn {
            cpu.registers[rn] = new_base;
        }
    }
}

fn execute_load_store(cpu: &mut Cpu, instr: u32, bus: &mut Bus) {
    let i = (instr >> 25) & 1;
    let p = (instr >> 24) & 1;
    let u = (instr >> 23) & 1;
    let b = (instr >> 22) & 1;
    let w = (instr >> 21) & 1;
    let l = (instr >> 20) & 1;
    let rn = ((instr >> 16) & 0xF) as usize;
    let rd = ((instr >> 12) & 0xF) as usize;

    let offset = if i == 0 {
        instr & 0xFFF
    } else {
        let rm = (instr & 0xF) as usize;
        // In LDR/STR, index Rm is always PC + 8
        let val = if rm == 15 {
            cpu.registers[15].wrapping_add(4)
        } else {
            cpu.registers[rm]
        };
        let shift_type = match (instr >> 5) & 0x3 {
            0 => crate::cpu::alu::ShiftType::LSL,
            1 => crate::cpu::alu::ShiftType::LSR,
            2 => crate::cpu::alu::ShiftType::ASR,
            _ => crate::cpu::alu::ShiftType::ROR,
        };
        let amount = (instr >> 7) & 0x1F;
        let (res, _) = crate::cpu::alu::barrel_shift(
            shift_type,
            amount,
            val,
            crate::cpu::alu::get_flag(cpu.cpsr, 29),
            true,
        );
        res
    };

    // LDR/STR base Rn is always PC + 8
    let base = if rn == 15 {
        cpu.registers[15].wrapping_add(4)
    } else {
        cpu.registers[rn]
    };
    let addr = if p == 1 {
        if u == 1 {
            base.wrapping_add(offset)
        } else {
            base.wrapping_sub(offset)
        }
    } else {
        base
    };

    if l == 1 {
        let mut val = if b == 1 {
            bus.read8(addr) as u32
        } else {
            let region = addr >> 24;
            if matches!(region, 0x0E | 0x0F) {
                bus.read32(addr)
            } else {
                let data = bus.read32(addr & !3);
                if (addr & 3) != 0 && region != 4 {
                    data.rotate_right((addr & 3) * 8)
                } else {
                    data
                }
            }
        };
        if rd == 15 {
            val &= !3;
        }
        cpu.registers[rd] = val;
        cpu.clock_internal(bus, 1); // 1I cycle
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    } else {
        let val = if rd == 15 {
            cpu.registers[15].wrapping_add(8)
        } else {
            cpu.registers[rd]
        };
        if b == 1 {
            bus.write8(addr, val as u8);
        } else {
            bus.write32(addr, val);
        }
        // Stores do not get the post-load internal cycle.
        bus.clock(0);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    }

    if w == 1 || p == 0 {
        let new_base = if u == 1 {
            base.wrapping_add(offset)
        } else {
            base.wrapping_sub(offset)
        };
        // LDR specific: loaded value into Rd takes precedence over Rn writeback
        if l == 0 || rd != rn {
            cpu.registers[rn] = new_base;
        }
    }
}

fn execute_ldm_stm(cpu: &mut Cpu, instr: u32, bus: &mut Bus) {
    let p = (instr >> 24) & 1;
    let u = (instr >> 23) & 1;
    let s = (instr >> 22) & 1;
    let w = (instr >> 21) & 1;
    let l = (instr >> 20) & 1;
    let rn = ((instr >> 16) & 0xF) as usize;
    let reg_list = (instr & 0xFFFF) as u16;
    let base = if rn == 15 {
        cpu.registers[15].wrapping_add(4)
    } else {
        cpu.registers[rn]
    };
    let mut count = 0;
    for i in 0..16 {
        if (reg_list & (1 << i)) != 0 {
            count += 1;
        }
    }

    // Empty list loads/stores R15 and advances by 0x40
    let (actual_list, _actual_count, transfer_words) = if reg_list == 0 {
        (0x8000, 1, 16)
    } else {
        (reg_list, count, count)
    };
    let offset = transfer_words * 4;
    let mut addr = if u == 1 {
        base
    } else {
        base.wrapping_sub(offset)
    };
    let new_base = if u == 1 {
        base.wrapping_add(offset)
    } else {
        base.wrapping_sub(offset)
    };
    if p == u {
        addr = addr.wrapping_add(4);
    }

    let mut current_addr = addr;
    let mut rn_loaded = false;
    let has_pc = (actual_list & 0x8000) != 0;
    let user_bank_transfer = s == 1 && (!has_pc || l == 0);
    let first_reg_in_list = (0..16).find(|i| (actual_list & (1 << i)) != 0);

    for i in 0..16 {
        if (actual_list & (1 << i)) != 0 {
            let access_addr = if matches!(current_addr >> 24, 0x0E | 0x0F) {
                current_addr
            } else {
                current_addr & !3
            };
            if l == 1 {
                let val = bus.read32(access_addr);
                if i == 15 {
                    cpu.registers[15] = val & !3;
                    if s == 1 {
                        cpu.set_cpsr(cpu.get_spsr());
                    }
                    cpu.invalidate_pipeline();
                } else {
                    if user_bank_transfer {
                        cpu.set_reg_usr(i, val);
                    } else {
                        cpu.registers[i] = val;
                    }
                }
                if i == rn {
                    rn_loaded = true;
                }
            } else {
                let val = if i == 15 {
                    cpu.registers[15].wrapping_add(8)
                } else {
                    if user_bank_transfer {
                        cpu.get_reg_usr(i)
                    } else if i == rn && w == 1 && reg_list != 0 && first_reg_in_list != Some(rn) {
                        new_base
                    } else {
                        cpu.registers[i]
                    }
                };
                bus.write32(access_addr, val);
            }
            current_addr = current_addr.wrapping_add(4);
        }
    }
    if l == 1 {
        // LDM ends with one post-transfer internal cycle.
        cpu.clock_internal(bus, 1);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    } else {
        // STM does not get that extra post-load cycle.
        bus.clock(0);
        if bus.code_in_gamepak() {
            cpu.clock_rom_execution_penalty_after_memory(bus, 2);
        }
    }

    // Writeback logic
    // 1. Rn must not be in the list (or it must be an empty list)
    // 2. For LDM with PC and S-bit set, writeback is suppressed on ARM7TDMI
    let suppress_wb = l == 1 && s == 1 && has_pc;
    if w == 1 && !suppress_wb && (!rn_loaded || reg_list == 0) {
        cpu.registers[rn] = new_base;
    }
}

fn execute_branch(cpu: &mut Cpu, instr: u32, _bus: &mut Bus) {
    let link = (instr >> 24) & 1;
    let mut offset = instr & 0x00FFFFFF;
    if (offset & 0x00800000) != 0 {
        offset |= 0xFF000000;
    }

    // Check if this is the failure loop in armwrestler
    if cpu.registers[15] == 0x08001090 && (instr >> 28) == 0x1 {
        // println!("FAILED TEST DETECTED: R0={:08X}, R1={:08X}", cpu.registers[0], cpu.registers[1]);
    }

    if link == 1 {
        cpu.registers[14] = cpu.registers[15];
    }
    let new_pc = cpu.registers[15].wrapping_add(4).wrapping_add(offset << 2);

    cpu.registers[15] = new_pc;
    cpu.invalidate_pipeline();
}

fn execute_bx(cpu: &mut Cpu, instr: u32, _bus: &mut Bus) {
    let rm = cpu.registers[(instr & 0xF) as usize];
    let new_pc;
    if (rm & 1) != 0 {
        cpu.set_cpsr(cpu.cpsr | 0x20);
        new_pc = rm & !1;
    } else {
        cpu.set_cpsr(cpu.cpsr & !0x20);
        new_pc = rm & !3;
    }
    cpu.registers[15] = new_pc;
    cpu.invalidate_pipeline();
}

fn execute_swi(cpu: &mut Cpu, _instr: u32, _bus: &mut Bus) {
    trace_swi(
        cpu.registers[15].wrapping_sub(4),
        _instr & 0x00FF_FFFF,
        false,
    );
    cpu.spsr_svc = cpu.cpsr;
    // Switch to Supervisor mode, ARM state, and disable IRQ
    cpu.set_cpsr((cpu.cpsr & !0x3F) | 0x13 | 0x80);
    // Bit 5 (Thumb) is now 0 because it was cleared by !0x3F and not set by | 0x13 | 0x80

    cpu.registers[14] = cpu.registers[15];

    cpu.registers[15] = 0x00000008;
    cpu.invalidate_pipeline();
}

fn trace_swi(caller_pc: u32, swi_number: u32, thumb: bool) {
    use std::sync::OnceLock;

    static TRACE_SWI: OnceLock<bool> = OnceLock::new();
    if !*TRACE_SWI.get_or_init(|| std::env::var_os("VIBE_TRACE_SWI").is_some()) {
        return;
    }

    let mode = if thumb { "THUMB" } else { "ARM" };
    eprintln!("[swi] mode={mode} caller={caller_pc:08X} num={swi_number:02X}");
}
