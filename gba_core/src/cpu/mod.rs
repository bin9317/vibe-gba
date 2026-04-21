pub mod alu;
pub mod arm;
pub mod thumb;
pub mod timing;

use crate::bus::Bus;
use crate::timing::access::gamepak_wait_components;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use timing::CpuTimingState;

const STACK_POINTER_USER_SYSTEM: u32 = 0x0300_7F00;
const STACK_POINTER_IRQ: u32 = 0x0300_7FA0;
const STACK_POINTER_SUPERVISOR: u32 = 0x0300_7FE0;
const BIOS_INTR_WAIT_FLAG_ADDR: u32 = 0x0300_7FF8;
const BIOS_IRQ_DISPATCH_ADDR: u32 = 0x0300_7FFC;
const BIOS_IRQ_DISPATCH_TARGET: u32 = 0x0000_0300;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CpuMode {
    User = 0b10000,
    Fiq = 0b10001,
    Irq = 0b10010,
    Supervisor = 0b10011,
    Abort = 0b10111,
    Undefined = 0b11011,
    System = 0b11111,
}

impl CpuMode {
    pub fn from_bits(bits: u32) -> Self {
        match bits & 0x1F {
            0b10000 => CpuMode::User,
            0b10001 => CpuMode::Fiq,
            0b10010 => CpuMode::Irq,
            0b10011 => CpuMode::Supervisor,
            0b10111 => CpuMode::Abort,
            0b11011 => CpuMode::Undefined,
            0b11111 => CpuMode::System,
            _ => CpuMode::User,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Cpu {
    pub registers: [u32; 16],
    pub cpsr: u32,
    // Legacy two-slot instruction window used by the current executor.
    pub pipeline_valid: bool,
    pub pipeline_thumb: bool,
    pub pipeline_pc: u32,
    pub pipeline_next_fetch: u32,
    pub pipeline_instrs: [u32; 2],
    // New timing-model scaffold. This does not fully drive execution yet, but it defines
    // the state we need for a cycle-accurate ARM7TDMI pipeline model.
    pub timing: CpuTimingState,

    // Banked registers
    pub banked_fiq: [u32; 7],        // R8-R14
    pub banked_svc: [u32; 2],        // R13-R14
    pub banked_abt: [u32; 2],        // R13-R14
    pub banked_irq: [u32; 2],        // R13-R14
    pub banked_und: [u32; 2],        // R13-R14
    pub banked_usr: [u32; 2],        // R13-R14 (User/System only)
    pub banked_usr_r8_r12: [u32; 5], // R8-R12 (Shared by all modes except FIQ)

    pub spsr_fiq: u32,
    pub spsr_svc: u32,
    pub spsr_abt: u32,
    pub spsr_irq: u32,
    pub spsr_und: u32,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    pub fn new() -> Self {
        let mut cpu = Self {
            registers: [0; 16],
            cpsr: 0x000000D3, // Supervisor mode, ARM state, IRQ/FIQ disabled
            pipeline_valid: false,
            pipeline_thumb: false,
            pipeline_pc: 0,
            pipeline_next_fetch: 0,
            pipeline_instrs: [0; 2],
            timing: CpuTimingState::default(),
            banked_fiq: [0; 7],
            banked_svc: [STACK_POINTER_SUPERVISOR, 0],
            banked_abt: [0; 2],
            banked_irq: [STACK_POINTER_IRQ, 0],
            banked_und: [0; 2],
            banked_usr: [STACK_POINTER_USER_SYSTEM, 0],
            banked_usr_r8_r12: [0; 5],
            spsr_fiq: 0,
            spsr_svc: 0,
            spsr_abt: 0,
            spsr_irq: 0,
            spsr_und: 0,
        };
        cpu.registers[13] = STACK_POINTER_SUPERVISOR; // Initial mode is Supervisor
        cpu
    }

    pub fn skip_bios(&mut self, bus: &mut Bus) {
        self.registers[13] = STACK_POINTER_USER_SYSTEM; // SP_usr/sys
        self.banked_irq[0] = STACK_POINTER_IRQ; // SP_irq
        self.banked_svc[0] = STACK_POINTER_SUPERVISOR; // SP_svc
        self.banked_abt[0] = STACK_POINTER_USER_SYSTEM;
        self.banked_und[0] = STACK_POINTER_USER_SYSTEM;
        self.banked_fiq[5] = STACK_POINTER_USER_SYSTEM;

        self.registers[15] = crate::GAMEPAK_ROM_START; // PC
        self.cpsr = 0x0000001F; // System mode

        // Initial BIOS variables in IWRAM
        bus.internal_write32(BIOS_INTR_WAIT_FLAG_ADDR, 0); // IntrWait flag
        bus.internal_write32(BIOS_IRQ_DISPATCH_ADDR, BIOS_IRQ_DISPATCH_TARGET); // BIOS IRQ dispatcher
        bus.postflg = 1;
        bus.note_control_flow_break();
        self.invalidate_pipeline();
    }

    pub fn invalidate_pipeline(&mut self) {
        self.timing.invalidate_pipeline();
        self.clear_legacy_pipeline_view();
    }

    fn pipeline_needs_refill(&self, thumb: bool) -> bool {
        !self.pipeline_valid || self.pipeline_thumb != thumb
    }

    fn refill_pipeline(&mut self, bus: &mut Bus) {
        self.timing.begin_refill();
        let thumb = self.get_thumb_mode();
        let pc = if thumb {
            self.registers[15] & !1
        } else {
            self.registers[15] & !3
        };
        let step = if thumb { 2 } else { 4 };
        self.pipeline_thumb = thumb;
        self.pipeline_pc = pc;
        self.pipeline_next_fetch = pc.wrapping_add(step * 2);
        let execute_instr = self.fetch_opcode(bus, pc, false, thumb);
        bus.note_bios_prefetch(pc, execute_instr, thumb);
        let decode_instr = self.fetch_opcode(bus, pc.wrapping_add(step), true, thumb);
        bus.note_bios_prefetch(pc.wrapping_add(step), decode_instr, thumb);
        self.pipeline_instrs[0] = execute_instr;
        self.pipeline_instrs[1] = decode_instr;
        self.pipeline_valid = true;
        self.timing
            .finish_refill(thumb, pc, step, execute_instr, decode_instr, 0);
        self.timing.fetch.valid = false;
        self.timing.fetch.pc = self.pipeline_next_fetch;
        self.update_gamepak_prefetch_stream(bus, thumb);
    }

    pub fn get_mode(&self) -> CpuMode {
        CpuMode::from_bits(self.cpsr)
    }

    pub fn set_cpsr(&mut self, val: u32) {
        let old_mode = self.get_mode();
        let new_mode = CpuMode::from_bits(val);
        if old_mode != new_mode {
            self.change_mode(old_mode, new_mode);
        }
        self.cpsr = val;
    }

    pub fn get_spsr(&self) -> u32 {
        match self.get_mode() {
            CpuMode::Fiq => self.spsr_fiq,
            CpuMode::Supervisor => self.spsr_svc,
            CpuMode::Abort => self.spsr_abt,
            CpuMode::Irq => self.spsr_irq,
            CpuMode::Undefined => self.spsr_und,
            _ => self.cpsr, // User/System don't have SPSR
        }
    }

    pub fn set_spsr(&mut self, val: u32) {
        match self.get_mode() {
            CpuMode::Fiq => self.spsr_fiq = val,
            CpuMode::Supervisor => self.spsr_svc = val,
            CpuMode::Abort => self.spsr_abt = val,
            CpuMode::Irq => self.spsr_irq = val,
            CpuMode::Undefined => self.spsr_und = val,
            _ => {} // User/System don't have SPSR
        }
    }

    fn change_mode(&mut self, old_mode: CpuMode, new_mode: CpuMode) {
        if old_mode == new_mode {
            return;
        }

        // 1. Save R13, R14 and SPSR of the OLD mode
        match old_mode {
            CpuMode::User | CpuMode::System => {
                self.banked_usr[0] = self.registers[13];
                self.banked_usr[1] = self.registers[14];
            }
            CpuMode::Fiq => {
                for i in 0..5 {
                    self.banked_fiq[i] = self.registers[8 + i];
                }
                self.banked_fiq[5] = self.registers[13];
                self.banked_fiq[6] = self.registers[14];
            }
            CpuMode::Supervisor => {
                self.banked_svc[0] = self.registers[13];
                self.banked_svc[1] = self.registers[14];
            }
            CpuMode::Abort => {
                self.banked_abt[0] = self.registers[13];
                self.banked_abt[1] = self.registers[14];
            }
            CpuMode::Irq => {
                self.banked_irq[0] = self.registers[13];
                self.banked_irq[1] = self.registers[14];
            }
            CpuMode::Undefined => {
                self.banked_und[0] = self.registers[13];
                self.banked_und[1] = self.registers[14];
            }
        }

        // 2. Specialized handling for R8-R12 (only banked in FIQ)
        if old_mode == CpuMode::Fiq {
            // Restore User/System R8-R12 from backup when LEAVING FIQ
            for i in 0..5 {
                self.registers[8 + i] = self.banked_usr_r8_r12[i];
            }
        } else if new_mode == CpuMode::Fiq {
            // Save User/System R8-R12 to backup when ENTERING FIQ
            for i in 0..5 {
                self.banked_usr_r8_r12[i] = self.registers[8 + i];
            }
            // Then load FIQ's private R8-R12
            for i in 0..5 {
                self.registers[8 + i] = self.banked_fiq[i];
            }
        }

        // 3. Load R13, R14 and SPSR of the NEW mode
        match new_mode {
            CpuMode::User | CpuMode::System => {
                self.registers[13] = self.banked_usr[0];
                self.registers[14] = self.banked_usr[1];
            }
            CpuMode::Fiq => {
                self.registers[13] = self.banked_fiq[5];
                self.registers[14] = self.banked_fiq[6];
            }
            CpuMode::Supervisor => {
                self.registers[13] = self.banked_svc[0];
                self.registers[14] = self.banked_svc[1];
            }
            CpuMode::Abort => {
                self.registers[13] = self.banked_abt[0];
                self.registers[14] = self.banked_abt[1];
            }
            CpuMode::Irq => {
                self.registers[13] = self.banked_irq[0];
                self.registers[14] = self.banked_irq[1];
            }
            CpuMode::Undefined => {
                self.registers[13] = self.banked_und[0];
                self.registers[14] = self.banked_und[1];
            }
        }
    }

    pub fn get_thumb_mode(&self) -> bool {
        (self.cpsr & 0x20) != 0
    }

    pub fn get_reg_usr(&self, reg: usize) -> u32 {
        if reg < 8 || reg == 15 {
            self.registers[reg]
        } else if reg < 13 {
            // R8-R12 are only banked in FIQ mode
            if self.get_mode() == CpuMode::Fiq {
                self.banked_usr_r8_r12[reg - 8]
            } else {
                self.registers[reg]
            }
        } else if reg == 13 {
            // R13 (SP) is banked in all privileged modes
            match self.get_mode() {
                CpuMode::User | CpuMode::System => self.registers[13],
                _ => self.banked_usr[0],
            }
        } else {
            // reg == 14
            // R14 (LR) is banked in all privileged modes
            match self.get_mode() {
                CpuMode::User | CpuMode::System => self.registers[14],
                _ => self.banked_usr[1],
            }
        }
    }

    pub fn set_reg_usr(&mut self, reg: usize, val: u32) {
        if reg < 8 || reg == 15 {
            self.registers[reg] = val;
        } else if reg < 13 {
            if self.get_mode() == CpuMode::Fiq {
                self.banked_usr_r8_r12[reg - 8] = val;
            } else {
                self.registers[reg] = val;
            }
        } else if reg == 13 {
            match self.get_mode() {
                CpuMode::User | CpuMode::System => self.registers[13] = val,
                _ => self.banked_usr[0] = val,
            }
        } else {
            // reg == 14
            match self.get_mode() {
                CpuMode::User | CpuMode::System => self.registers[14] = val,
                _ => self.banked_usr[1] = val,
            }
        }
    }

    pub fn step(&mut self, bus: &mut Bus) {
        let thumb = self.get_thumb_mode();
        let start_cycles = bus.cycles;
        if self.pipeline_needs_refill(thumb) {
            self.refill_pipeline(bus);
        }

        let pc = self.pipeline_pc;
        let step = if thumb { 2 } else { 4 };
        let instr = self.pipeline_instrs[0];
        bus.pc_at_access = pc;
        let fetched_tail_pc = self.pipeline_next_fetch;

        self.registers[15] = pc.wrapping_add(step);
        self.timing.begin_legacy_step();

        if thumb {
            self.execute_thumb(instr as u16, bus);
        } else {
            self.execute_arm(instr, bus);
        }

        let control_flow_changed =
            self.get_thumb_mode() != thumb || self.registers[15] != pc.wrapping_add(step);
        if control_flow_changed {
            bus.invalidate_gamepak_prefetch();
            bus.note_control_flow_break();
            self.timing.flush_control_flow();
            self.clear_legacy_pipeline_view();
        } else {
            let fetched_tail = self.ensure_fetch_slot(bus, thumb);
            bus.note_bios_prefetch(fetched_tail_pc, fetched_tail, thumb);
            self.pipeline_pc = self.pipeline_pc.wrapping_add(step);
            self.pipeline_next_fetch = self.pipeline_next_fetch.wrapping_add(step);
            self.pipeline_instrs[0] = self.pipeline_instrs[1];
            self.pipeline_instrs[1] = fetched_tail;
            self.timing.commit_legacy_step(thumb, step);
            self.update_gamepak_prefetch_stream(bus, thumb);
            // Don't consume prefetch here — let ensure_fetch_slot find them next step
            self.timing.seed_legacy_window(
                thumb,
                self.pipeline_pc,
                step,
                self.pipeline_instrs[0],
                self.pipeline_instrs[1],
                0,
            );
            self.timing.fetch.valid = false;
            self.timing.fetch.pc = self.pipeline_next_fetch;
        }

        trace_instruction_step(
            self,
            bus,
            pc,
            instr,
            thumb,
            start_cycles,
            control_flow_changed,
        );
        trace_mario_math_wrapper_step(self, bus, pc, instr, thumb);
    }

    pub(crate) fn clock_internal(&mut self, bus: &mut Bus, cycles: u32) {
        if cycles == 0 {
            return;
        }
        trace_timing_internal("before-internal", self, bus, cycles);
        self.timing.begin_internal_cycles(cycles);
        bus.clock(cycles);
        self.timing.finish_internal_cycles();
        trace_timing_internal("after-internal", self, bus, cycles);
        self.try_fill_fetch_slot_from_prefetch(bus);
    }

    pub(crate) fn clock_rom_execution_penalty(&mut self, _bus: &mut Bus, _cycles: u32) {
        if _cycles == 0 || !_bus.code_in_gamepak() || _bus.is_gamepak_prefetch_enabled() {
            return;
        }
        let region = _bus.pc_at_access >> 24;
        let (n_wait, s_wait) = gamepak_wait_components(_bus.waitcnt, region);
        let penalty = n_wait.saturating_sub(s_wait).max(1);
        self.clock_internal(_bus, penalty);
    }

    pub(crate) fn clock_rom_execution_penalty_after_memory(&mut self, bus: &mut Bus, cycles: u32) {
        if bus.last_data_used_gamepak_bus() {
            return;
        }
        self.clock_rom_execution_penalty(bus, cycles);
    }

    fn fetch_opcode(&mut self, bus: &mut Bus, addr: u32, sequential: bool, thumb: bool) -> u32 {
        self.timing.begin_opcode_fetch(thumb, addr);
        let fetch = if thumb {
            bus.fetch16_timed(addr, sequential)
        } else {
            bus.fetch32_timed(addr, sequential)
        };
        self.timing
            .finish_opcode_fetch(fetch.value, fetch.cycles, fetch.used_prefetch);
        fetch.value
    }

    fn ensure_fetch_slot(&mut self, bus: &mut Bus, thumb: bool) -> u32 {
        if !self.timing.fetch.valid {
            let addr = self.pipeline_next_fetch;
            // The bus tracks whether the current Game Pak opcode stream stayed intact.
            // Non-Game Pak traffic does not break it, but Game Pak data accesses do.
            let sequential = bus.next_gamepak_fetch_is_sequential();
            self.fetch_opcode(bus, addr, sequential, thumb);
        }
        self.timing.fetch.instruction
    }

    fn try_fill_fetch_slot_from_prefetch(&mut self, bus: &mut Bus) {
        if self.timing.fetch.valid {
            return;
        }
        let thumb = self.get_thumb_mode();
        let addr = self.timing.fetch.pc;
        let fetch = if thumb {
            bus.try_fetch_prefetched16(addr, true)
        } else {
            bus.try_fetch_prefetched32(addr, true)
        };
        let Some(fetch) = fetch else {
            return;
        };
        self.timing.begin_opcode_fetch(thumb, addr);
        self.timing
            .finish_opcode_fetch(fetch.value, fetch.cycles, fetch.used_prefetch);
    }

    fn update_gamepak_prefetch_stream(&mut self, bus: &mut Bus, thumb: bool) {
        let step = if thumb { 2 } else { 4 };
        let next_fetch_addr = if self.timing.fetch.valid {
            self.timing.fetch.pc.wrapping_add(step)
        } else {
            self.timing.fetch.pc
        };
        bus.set_gamepak_prefetch_stream(next_fetch_addr);
    }

    fn clear_legacy_pipeline_view(&mut self) {
        self.pipeline_valid = false;
        self.pipeline_pc = 0;
        self.pipeline_next_fetch = 0;
        self.pipeline_instrs = [0; 2];
    }

    fn execute_arm(&mut self, instr: u32, bus: &mut Bus) {
        arm::execute_arm(self, instr, bus);
    }

    fn execute_thumb(&mut self, instr: u16, bus: &mut Bus) {
        thumb::execute_thumb(self, instr, bus);
    }
}

fn trace_timing_internal(phase: &str, cpu: &Cpu, bus: &Bus, cycles: u32) {
    static TRACE_TIMING: OnceLock<bool> = OnceLock::new();
    if !*TRACE_TIMING.get_or_init(|| std::env::var_os("VIBE_TRACE_TIMING").is_some()) {
        return;
    }

    eprintln!(
        "[timing-cpu] {phase} pc={:08X} cyc={} pref_count={} pref_cyc={} head={:08X} fill={:08X} block={} fetch_valid={} fetch_pc={:08X}",
        cpu.registers[15],
        cycles,
        bus.gamepak_prefetch_count,
        bus.gamepak_prefetch_cycles,
        bus.gamepak_prefetch_head_addr,
        bus.gamepak_prefetch_fill_addr,
        bus.gamepak_prefetch_block_cycles,
        cpu.timing.fetch.valid as u8,
        cpu.timing.fetch.pc,
    );
}

fn trace_instruction_step(
    cpu: &Cpu,
    bus: &Bus,
    pc: u32,
    instr: u32,
    thumb: bool,
    start_cycles: u64,
    control_flow_changed: bool,
) {
    static TRACE_STEP_TIMING: OnceLock<bool> = OnceLock::new();
    if !*TRACE_STEP_TIMING.get_or_init(|| std::env::var_os("VIBE_TRACE_STEP_TIMING").is_some()) {
        return;
    }

    eprintln!(
        "[timing-step] pc={:08X} instr={:08X} thumb={} cyc={} fetch_cyc={} pref_hit={} ctrl_flow={} next_pc={:08X} pref_count={} pref_cyc={} head={:08X} fill={:08X}",
        pc,
        instr,
        thumb as u8,
        bus.cycles.saturating_sub(start_cycles),
        cpu.timing.last_opcode_fetch_cycles,
        cpu.timing.last_opcode_used_prefetch as u8,
        control_flow_changed as u8,
        cpu.registers[15],
        bus.gamepak_prefetch_count,
        bus.gamepak_prefetch_cycles,
        bus.gamepak_prefetch_head_addr,
        bus.gamepak_prefetch_fill_addr,
    );
}

fn trace_mario_math_wrapper_step(cpu: &Cpu, bus: &Bus, pc: u32, instr: u32, thumb: bool) {
    use std::sync::OnceLock;

    static TRACE: OnceLock<bool> = OnceLock::new();
    if !*TRACE.get_or_init(|| std::env::var_os("VIBE_TRACE_MK_MATH").is_some()) {
        return;
    }
    if !thumb {
        return;
    }

    if !(0x0806_1348..=0x0806_135A).contains(&pc) {
        return;
    }

    eprintln!(
        "[mk-math] pc={pc:08X} instr={:04X} r0={:08X} r1={:08X} r2={:08X} r3={:08X} lr={:08X} sp={:08X} scanline={} line_cycle={}",
        instr as u16,
        cpu.registers[0],
        cpu.registers[1],
        cpu.registers[2],
        cpu.registers[3],
        cpu.registers[14],
        cpu.registers[13],
        bus.ppu.current_scanline,
        bus.ppu.cycles,
    );
}
