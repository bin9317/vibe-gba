use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PipelinePhase {
    #[default]
    Fetch,
    Decode,
    Execute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BusAccessKind {
    #[default]
    None,
    OpcodeFetch16,
    OpcodeFetch32,
    DataRead16,
    DataRead32,
    DataWrite16,
    DataWrite32,
    InternalCycle,
    PipelineRefill,
    ExceptionFlush,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PipelineSlot {
    pub valid: bool,
    pub thumb: bool,
    pub pc: u32,
    pub instruction: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CpuTimingState {
    // Explicit three-stage pipeline scaffold for the future ARM7TDMI timing model.
    // The current executor still uses the legacy two-slot window, but this state lets us
    // gradually move toward a first-principles implementation without another broad rewrite.
    pub fetch: PipelineSlot,
    pub decode: PipelineSlot,
    pub execute: PipelineSlot,
    // Bus/timing state that will eventually replace many scattered bus.clock(...) calls.
    pub phase: PipelinePhase,
    pub last_bus_access: BusAccessKind,
    pub last_opcode_fetch_cycles: u32,
    pub last_opcode_used_prefetch: bool,
    pub internal_cycles_remaining: u32,
    pub pending_pipeline_flush: bool,
    pub pending_refill: bool,
}

impl Default for CpuTimingState {
    fn default() -> Self {
        Self {
            fetch: PipelineSlot::default(),
            decode: PipelineSlot::default(),
            execute: PipelineSlot::default(),
            phase: PipelinePhase::Fetch,
            last_bus_access: BusAccessKind::None,
            last_opcode_fetch_cycles: 0,
            last_opcode_used_prefetch: false,
            internal_cycles_remaining: 0,
            pending_pipeline_flush: false,
            pending_refill: true,
        }
    }
}

impl CpuTimingState {
    pub fn needs_refill(&self, thumb: bool) -> bool {
        self.pending_refill
            || !self.execute.valid
            || !self.decode.valid
            || self.execute.thumb != thumb
            || self.decode.thumb != thumb
    }

    pub fn begin_refill(&mut self) {
        self.fetch = PipelineSlot::default();
        self.decode = PipelineSlot::default();
        self.execute = PipelineSlot::default();
        self.phase = PipelinePhase::Fetch;
        self.last_bus_access = BusAccessKind::PipelineRefill;
        self.pending_pipeline_flush = false;
        self.pending_refill = true;
    }

    pub fn finish_refill(
        &mut self,
        thumb: bool,
        pc: u32,
        step: u32,
        execute_instr: u32,
        decode_instr: u32,
        fetch_instr: u32,
    ) {
        self.seed_legacy_window(thumb, pc, step, execute_instr, decode_instr, fetch_instr);
    }

    pub fn invalidate_pipeline(&mut self) {
        self.fetch = PipelineSlot::default();
        self.decode = PipelineSlot::default();
        self.execute = PipelineSlot::default();
        self.phase = PipelinePhase::Fetch;
        self.last_bus_access = BusAccessKind::ExceptionFlush;
        self.internal_cycles_remaining = 0;
        self.pending_pipeline_flush = true;
        self.pending_refill = true;
    }

    pub fn begin_legacy_step(&mut self) {
        self.phase = PipelinePhase::Execute;
        self.pending_pipeline_flush = false;
    }

    pub fn flush_control_flow(&mut self) {
        self.invalidate_pipeline();
        self.last_bus_access = BusAccessKind::ExceptionFlush;
    }

    pub fn begin_internal_cycles(&mut self, cycles: u32) {
        self.phase = PipelinePhase::Execute;
        self.last_bus_access = BusAccessKind::InternalCycle;
        self.internal_cycles_remaining = cycles;
    }

    pub fn finish_internal_cycles(&mut self) {
        self.internal_cycles_remaining = 0;
    }

    pub fn begin_pipeline_refill_cycles(&mut self, cycles: u32) {
        self.phase = PipelinePhase::Fetch;
        self.last_bus_access = BusAccessKind::PipelineRefill;
        self.internal_cycles_remaining = cycles;
        self.pending_refill = true;
    }

    pub fn finish_pipeline_refill_cycles(&mut self) {
        self.internal_cycles_remaining = 0;
    }

    pub fn begin_rom_execution_penalty(&mut self, cycles: u32) {
        self.phase = PipelinePhase::Execute;
        self.last_bus_access = BusAccessKind::OpcodeFetch32;
        self.internal_cycles_remaining = cycles;
    }

    pub fn finish_rom_execution_penalty(&mut self) {
        self.internal_cycles_remaining = 0;
    }

    pub fn begin_opcode_fetch(&mut self, thumb: bool, pc: u32) {
        self.phase = PipelinePhase::Fetch;
        self.last_bus_access = if thumb {
            BusAccessKind::OpcodeFetch16
        } else {
            BusAccessKind::OpcodeFetch32
        };
        self.last_opcode_fetch_cycles = 0;
        self.last_opcode_used_prefetch = false;
        self.fetch = PipelineSlot {
            valid: false,
            thumb,
            pc,
            instruction: 0,
        };
    }

    pub fn finish_opcode_fetch(&mut self, instruction: u32, cycles: u32, used_prefetch: bool) {
        self.internal_cycles_remaining = 0;
        self.last_opcode_fetch_cycles = cycles;
        self.last_opcode_used_prefetch = used_prefetch;
        self.fetch.valid = true;
        self.fetch.instruction = instruction;
    }

    pub fn seed_legacy_window(
        &mut self,
        thumb: bool,
        pc: u32,
        step: u32,
        execute_instr: u32,
        decode_instr: u32,
        fetch_instr: u32,
    ) {
        self.fetch = PipelineSlot {
            valid: true,
            thumb,
            pc: pc.wrapping_add(step * 2),
            instruction: fetch_instr,
        };
        self.decode = PipelineSlot {
            valid: true,
            thumb,
            pc: pc.wrapping_add(step),
            instruction: decode_instr,
        };
        self.execute = PipelineSlot {
            valid: true,
            thumb,
            pc,
            instruction: execute_instr,
        };
        self.phase = PipelinePhase::Execute;
        self.last_bus_access = if thumb {
            BusAccessKind::OpcodeFetch16
        } else {
            BusAccessKind::OpcodeFetch32
        };
        self.pending_pipeline_flush = false;
        self.pending_refill = false;
    }

    pub fn commit_legacy_step(&mut self, thumb: bool, step: u32) {
        let next_fetch_pc = self.fetch.pc.wrapping_add(step);
        self.execute = self.decode;
        self.decode = self.fetch;
        self.fetch = PipelineSlot {
            valid: false,
            thumb,
            pc: next_fetch_pc,
            instruction: 0,
        };
        self.phase = PipelinePhase::Execute;
        self.last_bus_access = if thumb {
            BusAccessKind::OpcodeFetch16
        } else {
            BusAccessKind::OpcodeFetch32
        };
    }
}
