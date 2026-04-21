use crate::dma::Dma;
use crate::eeprom::Eeprom;
use crate::ppu::Ppu;
use crate::ppu::io as ppu_io;
use crate::timers::Timers;
use crate::timing::access::{
    AccessDescriptor, AccessKind, AccessTiming, AccessTimingConfig, AccessWidth,
    lookup_access_timing,
};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

pub const REG_DISPCNT: u32 = 0x04000000;
pub const REG_DISPSTAT: u32 = 0x04000004;
pub const REG_VCOUNT: u32 = 0x04000006;
pub const REG_KEYINPUT: u32 = 0x04000130;
pub const REG_IE: u32 = 0x04000200;
pub const REG_IF: u32 = 0x04000202;
pub const REG_WAITCNT: u32 = 0x04000204;
pub const REG_IME: u32 = 0x04000208;
pub const REG_POSTFLG: u32 = 0x04000300;
pub const REG_HALTCNT: u32 = 0x04000301;
pub const REG_INTERNAL_MEMORY_CONTROL: u32 = 0x04000800;

const SOUND_IO_START: u32 = 0x0060;
const SOUND_IO_END: u32 = 0x00A7;
const SOUND_IO_LEN: usize = (SOUND_IO_END - SOUND_IO_START + 1) as usize;
const REG_SOUNDBIAS_LO: usize = (0x0088 - SOUND_IO_START) as usize;
const REG_SOUNDBIAS_HI: usize = (0x0089 - SOUND_IO_START) as usize;
const SOUND_FIFO_CAPACITY: usize = 32;
const SOUND_FIFO_DMA_CHUNK_BYTES: usize = 16;
const WAITCNT_PREFETCH_ENABLE: u16 = 1 << 14;
const EEPROM_UPPER_BOUNDARY: usize = 0x0100_0000;
const SRAM_FLASH_LEN: usize = 128 * 1024;
const FLASH_BANK_LEN: usize = 64 * 1024;
const FLASH_SECTOR_LEN: usize = 4 * 1024;
const GAMEPAK_PREFETCH_CAPACITY: u8 = 8;

pub const REGION_BIOS: u32 = 0x00;
pub const REGION_EWRAM: u32 = 0x02;
pub const REGION_IWRAM: u32 = 0x03;
pub const REGION_IO: u32 = 0x04;
pub const REGION_PALETTE: u32 = 0x05;
pub const REGION_VRAM: u32 = 0x06;
pub const REGION_OAM: u32 = 0x07;
pub const REGION_ROM_WS0: u32 = 0x08;
pub const REGION_ROM_WS1: u32 = 0x0A;
pub const REGION_ROM_WS2: u32 = 0x0C;
pub const REGION_SRAM: u32 = 0x0E;
pub const REGION_SRAM_MIRROR: u32 = 0x0F;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FetchAccess {
    pub value: u32,
    pub cycles: u32,
    pub used_prefetch: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupType {
    Unknown,
    Eeprom,
    Sram,
    Flash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum FlashState {
    Ready,
    Command55,
    Program,
    EraseAA,
    Erase55,
    BankSwitch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CpuBusTimingState {
    last_data_address: u32,
    last_data_valid: bool,
    last_data_used_gamepak_bus: bool,
    last_gamepak_data_address: u32,
    last_gamepak_data_valid: bool,
    next_gamepak_fetch_is_sequential: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SoundFifo {
    bytes: [u8; SOUND_FIFO_CAPACITY],
    read_index: u8,
    len: u8,
    last_sample: i8,
}

impl Default for SoundFifo {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundFifo {
    pub fn new() -> Self {
        Self {
            bytes: [0; SOUND_FIFO_CAPACITY],
            read_index: 0,
            len: 0,
            last_sample: 0,
        }
    }

    fn clear(&mut self) {
        self.read_index = 0;
        self.len = 0;
        self.last_sample = 0;
    }

    fn len(&self) -> usize {
        self.len as usize
    }

    fn push_byte(&mut self, value: u8) {
        if self.len as usize >= SOUND_FIFO_CAPACITY {
            return;
        }
        let write_index = (self.read_index as usize + self.len as usize) % SOUND_FIFO_CAPACITY;
        self.bytes[write_index] = value;
        self.len += 1;
    }

    fn pop_sample(&mut self) -> i8 {
        if self.len == 0 {
            return self.last_sample;
        }
        let value = self.bytes[self.read_index as usize] as i8;
        self.read_index = ((self.read_index as usize + 1) % SOUND_FIFO_CAPACITY) as u8;
        self.len -= 1;
        self.last_sample = value;
        value
    }
}

pub struct Bus {
    pub bios: Box<[u8; 16 * 1024]>,
    pub on_board_wram: Box<[u8; 256 * 1024]>,
    pub on_chip_wram: Box<[u8; 32 * 1024]>,
    pub palette_ram: Box<[u8; 1 * 1024]>,
    pub vram: Box<[u8; 96 * 1024]>,
    pub oam: Box<[u8; 1 * 1024]>,
    pub rom: Vec<u8>,
    pub sram: Box<[u8; SRAM_FLASH_LEN]>,
    pub eeprom: Eeprom,
    pub ppu: Ppu,
    pub dma: Dma,
    pub timers: Timers,
    pub sound_io: Box<[u8; SOUND_IO_LEN]>,
    pub(crate) sound_fifos: [SoundFifo; 2],
    pub ie: u16,
    pub if_: u16,
    pub ime: u32,
    pub postflg: u8,
    pub halted: bool,
    pub waitcnt: u16,
    pub internal_memory_control: u32,
    pub cycles: u64,
    pub pc_at_access: u32,
    pub bios_open_bus_value: u32,
    pub prefetch_open_bus_value: u32,
    pub gamepak_prefetch_count: u8,
    pub gamepak_prefetch_head_addr: u32,
    pub gamepak_prefetch_fill_addr: u32,
    pub gamepak_prefetch_cycles: u32,
    pub gamepak_prefetch_block_cycles: u32,
    pub key_state: u16,
    pub sram_dirty: bool,
    pub pending_dma: u16,
    pub dma_running: bool,
    pub backup_type: BackupType,
    pub flash_is_128k: bool,
    pub flash_bank: usize,
    flash_state: FlashState,
    cpu_bus_timing: CpuBusTimingState,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus {
    pub fn new() -> Self {
        Self {
            bios: vec![0; 16 * 1024].into_boxed_slice().try_into().unwrap(),
            on_board_wram: vec![0; 256 * 1024].into_boxed_slice().try_into().unwrap(),
            on_chip_wram: vec![0; 32 * 1024].into_boxed_slice().try_into().unwrap(),
            palette_ram: vec![0; 1 * 1024].into_boxed_slice().try_into().unwrap(),
            vram: vec![0; 96 * 1024].into_boxed_slice().try_into().unwrap(),
            oam: vec![0; 1 * 1024].into_boxed_slice().try_into().unwrap(),
            rom: Vec::new(),
            sram: vec![0xFF; SRAM_FLASH_LEN]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
            eeprom: Eeprom::new(),
            ppu: Ppu::new(),
            dma: Dma::new(),
            timers: Timers::new(),
            sound_io: {
                let mut regs: Box<[u8; SOUND_IO_LEN]> =
                    vec![0; SOUND_IO_LEN].into_boxed_slice().try_into().unwrap();
                regs[REG_SOUNDBIAS_LO] = 0x00;
                regs[REG_SOUNDBIAS_HI] = 0x02;
                regs
            },
            sound_fifos: [SoundFifo::new(), SoundFifo::new()],
            ie: 0,
            if_: 0,
            ime: 0,
            postflg: 0,
            halted: false,
            waitcnt: 0,
            internal_memory_control: 0x0D000020,
            cycles: 0,
            pc_at_access: 0,
            bios_open_bus_value: 0,
            prefetch_open_bus_value: 0,
            gamepak_prefetch_count: 0,
            gamepak_prefetch_head_addr: 0,
            gamepak_prefetch_fill_addr: 0,
            gamepak_prefetch_cycles: 0,
            gamepak_prefetch_block_cycles: 0,
            key_state: 0x03FF,
            sram_dirty: false,
            pending_dma: 0,
            dma_running: false,
            backup_type: BackupType::Unknown,
            flash_is_128k: false,
            flash_bank: 0,
            flash_state: FlashState::Ready,
            cpu_bus_timing: CpuBusTimingState {
                last_data_address: 0,
                last_data_valid: false,
                last_data_used_gamepak_bus: false,
                last_gamepak_data_address: 0,
                last_gamepak_data_valid: false,
                next_gamepak_fetch_is_sequential: true,
            },
        }
    }

    fn step_timers(&mut self, cycles: u32) -> [bool; 4] {
        let mut overflow = [false; 4];
        for i in 0..4 {
            let cnt = self.timers.timers[i].cnt;
            if (cnt & 0x80) == 0 {
                continue;
            }

            // 2-cycle startup delay after enable: skip ticking during delay period
            if self.timers.timers[i].startup_delay > 0 {
                self.timers.timers[i].startup_delay -= 1;
                continue;
            }

            let is_cascade = (cnt & 0x0004) != 0 && i > 0;
            let mut ticks = 0;

            if is_cascade {
                if overflow[i - 1] {
                    ticks = 1;
                }
            } else {
                let prescaler = match cnt & 0x3 {
                    0 => 1,
                    1 => 64,
                    2 => 256,
                    3 => 1024,
                    _ => 1,
                };
                self.timers.timers[i].internal_value += (cycles as f64) / (prescaler as f64);
                ticks = self.timers.timers[i].internal_value.floor() as u32;
                self.timers.timers[i].internal_value -= ticks as f64;
            }

            if ticks > 0 {
                let (new_val, ovf) = self.timers.timers[i]
                    .current_value
                    .overflowing_add(ticks as u16);
                if ovf {
                    overflow[i] = true;
                    self.timers.timers[i].current_value = self.timers.timers[i]
                        .reload
                        .wrapping_add((ticks - 1) as u16);
                    if (cnt & 0x40) != 0 {
                        self.if_ |= 1 << (3 + i);
                        self.halted = false;
                    }
                } else {
                    self.timers.timers[i].current_value = new_val;
                }
            }
        }
        overflow
    }

    pub fn clock(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.cycles += 1;
            self.tick_gamepak_prefetch();
            let (vblank, hblank, vmatch) = self.ppu.step(
                1,
                self.vram.as_ref(),
                self.palette_ram.as_ref(),
                self.oam.as_ref(),
            );
            if vblank {
                if (self.ppu.registers.dispstat & 0x0008) != 0 {
                    self.if_ |= 1 << 0;
                    self.halted = false;
                }
                // Trigger DMA on VBlank
                for ch in 0..4 {
                    if (self.dma.channels[ch].cnt & 0x8000) != 0
                        && (self.dma.channels[ch].cnt >> 12) & 0x3 == 1
                    {
                        self.pending_dma |= 1 << ch;
                    }
                }
            }
            if hblank {
                let visible_line = self.ppu.current_scanline < 160;
                if visible_line && (self.ppu.registers.dispstat & 0x0010) != 0 {
                    self.if_ |= 1 << 1;
                    self.halted = false;
                    trace_irq_request("hblank", self);
                }
                // HBlank IRQ/DMA are only generated for visible scanlines.
                if visible_line {
                    for ch in 0..4 {
                        if (self.dma.channels[ch].cnt & 0x8000) != 0
                            && (self.dma.channels[ch].cnt >> 12) & 0x3 == 2
                        {
                            self.pending_dma |= 1 << ch;
                        }
                    }
                }
            }
            if vmatch {
                if (self.ppu.registers.dispstat & 0x0020) != 0 {
                    self.if_ |= 1 << 2;
                    self.halted = false;
                    trace_irq_request("vcount", self);
                }
            }
            let timer_overflow = self.step_timers(1);
            self.handle_sound_timer_overflows(timer_overflow);

            self.service_pending_dma();
        }
    }

    pub fn service_pending_dma(&mut self) {
        if self.pending_dma == 0 || self.dma_running {
            return;
        }
        for ch in 0..4 {
            if (self.pending_dma & (1 << ch)) != 0 {
                self.pending_dma &= !(1 << ch);
                trace_timing_dma(
                    "before-run-dma",
                    self,
                    ch,
                    self.dma.channels[ch].internal_count,
                );
                trace_bg2_hblank_dma(self, ch);
                self.run_dma(ch);
                trace_timing_dma(
                    "after-run-dma",
                    self,
                    ch,
                    self.dma.channels[ch].internal_count,
                );
            }
        }
    }

    fn sound_control_halfword(&self, offset: u32) -> u16 {
        let base = (offset - SOUND_IO_START) as usize;
        u16::from_le_bytes([self.sound_io[base], self.sound_io[base + 1]])
    }

    fn soundcnt_h(&self) -> u16 {
        self.sound_control_halfword(0x0082)
    }

    fn soundcnt_x(&self) -> u16 {
        self.sound_control_halfword(0x0084)
    }

    fn direct_sound_enabled(&self, fifo_index: usize) -> bool {
        if (self.soundcnt_x() & 0x0080) == 0 {
            return false;
        }
        let cnt = self.soundcnt_h();
        match fifo_index {
            0 => (cnt & 0x0300) != 0,
            1 => (cnt & 0x3000) != 0,
            _ => false,
        }
    }

    fn direct_sound_timer_index(&self, fifo_index: usize) -> usize {
        let cnt = self.soundcnt_h();
        match fifo_index {
            0 => {
                if (cnt & 0x0400) != 0 {
                    1
                } else {
                    0
                }
            }
            1 => {
                if (cnt & 0x4000) != 0 {
                    1
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    fn sound_dma_fifo_target(&self, ch: usize) -> Option<u32> {
        if ch == 3 || ((self.dma.channels[ch].cnt >> 12) & 0x3) != 3 {
            return None;
        }
        match self.dma.channels[ch].internal_dst & !3 {
            0x0400_00A0 => Some(0x0400_00A0),
            0x0400_00A4 => Some(0x0400_00A4),
            _ => None,
        }
    }

    fn request_sound_fifo_dma(&mut self, fifo_index: usize) {
        let target = match fifo_index {
            0 => 0x0400_00A0,
            1 => 0x0400_00A4,
            _ => return,
        };
        for ch in 1..=2 {
            if (self.dma.channels[ch].cnt & 0x8000) == 0 {
                continue;
            }
            if self.sound_dma_fifo_target(ch) == Some(target) {
                self.pending_dma |= 1 << ch;
            }
        }
    }

    fn handle_sound_timer_overflows(&mut self, overflow: [bool; 4]) {
        for fifo_index in 0..2 {
            if !self.direct_sound_enabled(fifo_index) {
                continue;
            }
            let timer_index = self.direct_sound_timer_index(fifo_index);
            if !overflow[timer_index] {
                continue;
            }
            let prev_len = self.sound_fifos[fifo_index].len();
            self.sound_fifos[fifo_index].pop_sample();
            let new_len = self.sound_fifos[fifo_index].len();
            if prev_len == 0
                || (prev_len > SOUND_FIFO_DMA_CHUNK_BYTES && new_len <= SOUND_FIFO_DMA_CHUNK_BYTES)
            {
                self.request_sound_fifo_dma(fifo_index);
            }
        }
    }

    fn push_sound_fifo_byte(&mut self, offset: u32, value: u8) {
        match offset {
            0x00A0..=0x00A3 => self.sound_fifos[0].push_byte(value),
            0x00A4..=0x00A7 => self.sound_fifos[1].push_byte(value),
            _ => {}
        }
    }

    fn apply_sound_control_side_effects(&mut self) {
        let soundcnt_h = self.soundcnt_h();
        if (soundcnt_h & 0x0800) != 0 {
            self.sound_fifos[0].clear();
            self.sound_io[(0x0083 - SOUND_IO_START) as usize] &= !0x08;
        }
        if (soundcnt_h & 0x8000) != 0 {
            self.sound_fifos[1].clear();
            self.sound_io[(0x0083 - SOUND_IO_START) as usize] &= !0x80;
        }
    }

    pub fn read8(&mut self, addr: u32) -> u8 {
        self.access(addr, AccessKind::DataRead, AccessWidth::Byte);
        self.internal_read8_prot(addr)
    }
    pub fn read16(&mut self, addr: u32) -> u16 {
        self.access(addr, AccessKind::DataRead, AccessWidth::Halfword);
        self.internal_read16_prot(addr)
    }
    pub fn read32(&mut self, addr: u32) -> u32 {
        self.access(addr, AccessKind::DataRead, AccessWidth::Word);
        self.internal_read32_prot(addr)
    }
    pub fn fetch16(&mut self, addr: u32, sequential: bool) -> u16 {
        self.fetch16_timed(addr, sequential).value as u16
    }
    pub fn fetch32(&mut self, addr: u32, sequential: bool) -> u32 {
        self.fetch32_timed(addr, sequential).value
    }
    pub fn fetch16_timed(&mut self, addr: u32, sequential: bool) -> FetchAccess {
        let prefetch_cycles = self.consume_gamepak_prefetch(addr, false, sequential);
        let timing = self.lookup_cpu_access_timing(
            addr,
            AccessKind::OpcodeFetch,
            AccessWidth::Halfword,
            sequential,
        );
        let c = prefetch_cycles.unwrap_or(timing.cycles);
        let used_prefetch = prefetch_cycles.is_some();
        trace_timing_bus_event(
            "before-fetch16",
            self,
            addr,
            AccessKind::OpcodeFetch,
            AccessWidth::Halfword,
            sequential,
            c,
        );
        // Block prefetch during ROM code fetch (Game Pak bus is busy for remaining cycles)
        if timing.uses_gamepak_bus && c > 0 {
            self.gamepak_prefetch_block_cycles =
                self.gamepak_prefetch_block_cycles.saturating_add(c);
        }
        self.prepare_gamepak_prefetch_after_fetch(addr, false, c);
        self.clock(c);
        self.note_opcode_fetch(timing.uses_gamepak_bus);
        trace_timing_bus_event(
            "after-fetch16",
            self,
            addr,
            AccessKind::OpcodeFetch,
            AccessWidth::Halfword,
            sequential,
            c,
        );
        FetchAccess {
            value: self.internal_read16(addr) as u32,
            cycles: c,
            used_prefetch,
        }
    }
    pub fn fetch32_timed(&mut self, addr: u32, sequential: bool) -> FetchAccess {
        let prefetch_cycles = self.consume_gamepak_prefetch(addr, true, sequential);
        let timing = self.lookup_cpu_access_timing(
            addr,
            AccessKind::OpcodeFetch,
            AccessWidth::Word,
            sequential,
        );
        let c = prefetch_cycles.unwrap_or(timing.cycles);
        let used_prefetch = prefetch_cycles.is_some();
        trace_timing_bus_event(
            "before-fetch32",
            self,
            addr,
            AccessKind::OpcodeFetch,
            AccessWidth::Word,
            sequential,
            c,
        );
        // Block prefetch during ROM code fetch (Game Pak bus is busy for remaining cycles)
        if timing.uses_gamepak_bus && c > 0 {
            self.gamepak_prefetch_block_cycles =
                self.gamepak_prefetch_block_cycles.saturating_add(c);
        }
        self.prepare_gamepak_prefetch_after_fetch(addr, true, c);
        self.clock(c);
        self.note_opcode_fetch(timing.uses_gamepak_bus);
        trace_timing_bus_event(
            "after-fetch32",
            self,
            addr,
            AccessKind::OpcodeFetch,
            AccessWidth::Word,
            sequential,
            c,
        );
        FetchAccess {
            value: self.internal_read32(addr),
            cycles: c,
            used_prefetch,
        }
    }
    pub fn try_fetch_prefetched16(&mut self, addr: u32, sequential: bool) -> Option<FetchAccess> {
        if !self.consume_ready_gamepak_prefetch(addr, false, sequential) {
            return None;
        }
        self.note_opcode_fetch(true);
        Some(FetchAccess {
            value: self.internal_read16(addr) as u32,
            cycles: 0,
            used_prefetch: true,
        })
    }
    pub fn try_fetch_prefetched32(&mut self, addr: u32, sequential: bool) -> Option<FetchAccess> {
        if !self.consume_ready_gamepak_prefetch(addr, true, sequential) {
            return None;
        }
        self.note_opcode_fetch(true);
        Some(FetchAccess {
            value: self.internal_read32(addr),
            cycles: 0,
            used_prefetch: true,
        })
    }
    pub fn write8(&mut self, addr: u32, value: u8) {
        self.access(addr, AccessKind::DataWrite, AccessWidth::Byte);
        self.internal_write8(addr, value)
    }
    pub fn write16(&mut self, addr: u32, value: u16) {
        self.access(addr, AccessKind::DataWrite, AccessWidth::Halfword);
        self.internal_write16(addr, value)
    }
    pub fn write32(&mut self, addr: u32, value: u32) {
        self.access(addr, AccessKind::DataWrite, AccessWidth::Word);
        self.internal_write32(addr, value)
    }

    fn access(&mut self, addr: u32, kind: AccessKind, width: AccessWidth) {
        let sequential = self.is_sequential_data_access(addr, width);
        let timing = self.lookup_cpu_access_timing(addr, kind, width, sequential);
        trace_timing_bus_event(
            "before-data",
            self,
            addr,
            kind,
            width,
            sequential,
            timing.cycles,
        );
        if timing.uses_gamepak_bus {
            // Data access to Game Pak bus: abort any in-progress prefetch fill.
            self.gamepak_prefetch_cycles = 0;
            self.gamepak_prefetch_block_cycles = self
                .gamepak_prefetch_block_cycles
                .saturating_add(timing.cycles);
        }
        self.clock(timing.cycles);
        self.note_data_access(addr, timing.uses_gamepak_bus);
        trace_timing_bus_event(
            "after-data",
            self,
            addr,
            kind,
            width,
            sequential,
            timing.cycles,
        );
    }

    pub fn code_in_gamepak(&self) -> bool {
        matches!(self.pc_at_access >> 24, REGION_ROM_WS0..=0x0D)
    }

    pub fn next_gamepak_fetch_is_sequential(&self) -> bool {
        self.cpu_bus_timing.next_gamepak_fetch_is_sequential
    }

    pub fn last_data_used_gamepak_bus(&self) -> bool {
        self.cpu_bus_timing.last_data_used_gamepak_bus
    }

    pub fn note_control_flow_break(&mut self) {
        self.cpu_bus_timing.next_gamepak_fetch_is_sequential = false;
    }

    pub(crate) fn cpu_bus_timing_snapshot(&self) -> (u32, bool, bool, bool) {
        (
            self.cpu_bus_timing.last_data_address,
            self.cpu_bus_timing.last_data_valid,
            self.cpu_bus_timing.last_data_used_gamepak_bus,
            self.cpu_bus_timing.next_gamepak_fetch_is_sequential,
        )
    }

    pub(crate) fn restore_cpu_bus_timing(
        &mut self,
        last_data_address: u32,
        last_data_valid: bool,
        last_data_used_gamepak_bus: bool,
        next_gamepak_fetch_is_sequential: bool,
    ) {
        self.cpu_bus_timing = CpuBusTimingState {
            last_data_address,
            last_data_valid,
            last_data_used_gamepak_bus,
            last_gamepak_data_address: last_data_address,
            last_gamepak_data_valid: last_data_valid && last_data_used_gamepak_bus,
            next_gamepak_fetch_is_sequential,
        };
    }

    pub(crate) fn reconstruct_gamepak_prefetch_after_load(
        &mut self,
        cpu_pipeline_valid: bool,
        cpu_pipeline_next_fetch: u32,
        cpu_fetch_valid: bool,
        cpu_fetch_pc: u32,
    ) {
        self.prefetch_open_bus_value = self.bios_open_bus_value;
        self.gamepak_prefetch_count = 0;
        self.gamepak_prefetch_cycles = 0;
        self.gamepak_prefetch_block_cycles = 0;
        self.gamepak_prefetch_head_addr = 0;
        self.gamepak_prefetch_fill_addr = 0;

        let next_fetch_addr = if cpu_fetch_valid {
            cpu_fetch_pc
        } else if cpu_pipeline_valid {
            cpu_pipeline_next_fetch
        } else {
            0
        };
        if next_fetch_addr != 0 {
            self.set_gamepak_prefetch_stream(next_fetch_addr);
        }
    }

    fn is_sequential_data_access(&self, addr: u32, width: AccessWidth) -> bool {
        let uses_gamepak_bus = matches!(
            addr >> 24,
            REGION_ROM_WS0..=0x0D | REGION_SRAM | REGION_SRAM_MIRROR
        );
        let last_addr = if uses_gamepak_bus {
            if !self.cpu_bus_timing.last_gamepak_data_valid {
                return false;
            }
            self.cpu_bus_timing.last_gamepak_data_address
        } else {
            if !self.cpu_bus_timing.last_data_valid {
                return false;
            }
            self.cpu_bus_timing.last_data_address
        };
        let expected = last_addr.wrapping_add(if width.is_32bit() { 4 } else { 2 });
        addr == expected || addr == last_addr.wrapping_add(1)
    }

    fn note_data_access(&mut self, addr: u32, uses_gamepak_bus: bool) {
        self.cpu_bus_timing.last_data_address = addr;
        self.cpu_bus_timing.last_data_valid = true;
        self.cpu_bus_timing.last_data_used_gamepak_bus = uses_gamepak_bus;
        if uses_gamepak_bus {
            self.cpu_bus_timing.last_gamepak_data_address = addr;
            self.cpu_bus_timing.last_gamepak_data_valid = true;
        }
        if uses_gamepak_bus {
            self.cpu_bus_timing.next_gamepak_fetch_is_sequential = false;
        }
    }

    fn note_opcode_fetch(&mut self, uses_gamepak_bus: bool) {
        if uses_gamepak_bus {
            self.cpu_bus_timing.next_gamepak_fetch_is_sequential = true;
        }
    }

    pub fn set_gamepak_prefetch_stream(&mut self, next_fetch_addr: u32) {
        if !Self::is_rom_region(next_fetch_addr) {
            self.invalidate_gamepak_prefetch();
            return;
        }
        if self.gamepak_prefetch_count == 0 {
            // Reset cycles when the fill address changes — stale progress from
            // a different address must not carry over as partial credit.
            if self.gamepak_prefetch_fill_addr != next_fetch_addr {
                self.gamepak_prefetch_cycles = 0;
            }
            self.gamepak_prefetch_head_addr = next_fetch_addr;
            self.gamepak_prefetch_fill_addr = next_fetch_addr;
            return;
        }
        if self.gamepak_prefetch_head_addr != next_fetch_addr {
            self.gamepak_prefetch_count = 0;
            self.gamepak_prefetch_cycles = 0;
            self.gamepak_prefetch_head_addr = next_fetch_addr;
            self.gamepak_prefetch_fill_addr = next_fetch_addr;
        }
    }

    pub fn invalidate_gamepak_prefetch(&mut self) {
        self.gamepak_prefetch_count = 0;
        self.gamepak_prefetch_cycles = 0;
        self.gamepak_prefetch_block_cycles = 0;
        self.gamepak_prefetch_head_addr = 0;
        self.gamepak_prefetch_fill_addr = 0;
    }

    fn gamepak_prefetch_enabled(&self) -> bool {
        (self.waitcnt & WAITCNT_PREFETCH_ENABLE) != 0
    }

    fn gamepak_prefetch_capacity(&self) -> u8 {
        if self.gamepak_prefetch_enabled() {
            GAMEPAK_PREFETCH_CAPACITY
        } else {
            1
        }
    }

    pub fn is_gamepak_prefetch_enabled(&self) -> bool {
        self.gamepak_prefetch_enabled()
    }

    fn prepare_gamepak_prefetch_after_fetch(&mut self, addr: u32, is_32bit: bool, _cycles: u32) {
        if !self.code_in_gamepak() || !Self::is_rom_region(addr) {
            return;
        }

        // While the CPU is paying for the current Game Pak opcode fetch, the prefetcher should
        // already be working on the following halfword(s), not on the opcode being fetched now.
        let next_addr = addr.wrapping_add(if is_32bit { 4 } else { 2 });
        self.set_gamepak_prefetch_stream(next_addr);
    }

    fn is_rom_region(addr: u32) -> bool {
        matches!(addr >> 24, REGION_ROM_WS0..=0x0D)
    }

    fn tick_gamepak_prefetch(&mut self) {
        if self.dma_running {
            return;
        }
        if self.gamepak_prefetch_block_cycles != 0 {
            self.gamepak_prefetch_block_cycles -= 1;
            return;
        }

        if !self.code_in_gamepak()
            || !Self::is_rom_region(self.gamepak_prefetch_fill_addr)
            || self.gamepak_prefetch_count >= self.gamepak_prefetch_capacity()
        {
            return;
        }

        self.gamepak_prefetch_cycles += 1;
        let wait = self
            .lookup_cpu_access_timing(
                self.gamepak_prefetch_fill_addr,
                AccessKind::OpcodeFetch,
                AccessWidth::Halfword,
                true,
            )
            .cycles;
        if self.gamepak_prefetch_cycles >= wait {
            self.gamepak_prefetch_cycles -= wait;
            if self.gamepak_prefetch_count == 0 {
                self.gamepak_prefetch_head_addr = self.gamepak_prefetch_fill_addr;
            }
            self.gamepak_prefetch_count += 1;
            self.gamepak_prefetch_fill_addr = self.gamepak_prefetch_fill_addr.wrapping_add(2);
        }
    }

    fn consume_gamepak_prefetch(
        &mut self,
        addr: u32,
        is_32bit: bool,
        _sequential: bool,
    ) -> Option<u32> {
        // The prefetch buffer provides data based on address match, not bus sequentiality.
        // After a data access to I/O, the fetch is "non-sequential" on the bus, but the
        // prefetch buffer still has the correct data at the expected code address.
        if !Self::is_rom_region(addr) {
            return None;
        }

        // If the buffer has completed entries at this address, consume them.
        if self.gamepak_prefetch_count > 0 && self.gamepak_prefetch_head_addr == addr {
            if !is_32bit {
                self.gamepak_prefetch_count -= 1;
                self.gamepak_prefetch_head_addr = self.gamepak_prefetch_head_addr.wrapping_add(2);
                if self.gamepak_prefetch_count == 0 {
                    self.gamepak_prefetch_fill_addr = self.gamepak_prefetch_head_addr;
                    self.gamepak_prefetch_cycles = 0;
                }
                self.note_opcode_fetch(true);
                return Some(1);
            }

            // ARM fetches are two sequential halfword accesses on the 16-bit Game Pak bus.
            let available = self.gamepak_prefetch_count.min(2);
            let partial_cycles = self.gamepak_prefetch_cycles;
            self.gamepak_prefetch_count -= available;
            self.gamepak_prefetch_head_addr = self
                .gamepak_prefetch_head_addr
                .wrapping_add(available as u32 * 2);
            if self.gamepak_prefetch_count == 0 {
                self.gamepak_prefetch_fill_addr = self.gamepak_prefetch_head_addr;
                self.gamepak_prefetch_cycles = 0;
            }
            let remaining_halfwords = 2 - available as u32;
            let remaining_cycles = match remaining_halfwords {
                0 => 0,
                // 1 buffered halfword consumed; second halfword at sequential cost,
                // but if the prefetcher was partially filling it, give partial credit
                1 => {
                    let seq_wait = self
                        .lookup_cpu_access_timing(
                            addr.wrapping_add(2),
                            AccessKind::OpcodeFetch,
                            AccessWidth::Halfword,
                            true,
                        )
                        .cycles;
                    let partial = partial_cycles.min(seq_wait);
                    self.gamepak_prefetch_cycles = 0;
                    seq_wait - partial
                }
                _ => {
                    self.lookup_cpu_access_timing(
                        addr,
                        AccessKind::OpcodeFetch,
                        AccessWidth::Word,
                        true,
                    )
                    .cycles
                }
            };
            self.note_opcode_fetch(true);
            return Some(remaining_cycles);
        }

        // Partial prefetch credit: the prefetcher is in the middle of filling a halfword
        // at exactly the address the CPU needs. The CPU waits only for the remaining cycles.
        // After a Game Pak data access, the bus address changed, so partial credit from the
        // brief idle period (internal cycle) is not meaningful — skip it.
        if self.gamepak_prefetch_fill_addr == addr
            && self.gamepak_prefetch_cycles > 0
            && !self.last_data_used_gamepak_bus()
        {
            let seq_wait = self
                .lookup_cpu_access_timing(
                    addr,
                    AccessKind::OpcodeFetch,
                    AccessWidth::Halfword,
                    true,
                )
                .cycles;
            let remaining_first = seq_wait.saturating_sub(self.gamepak_prefetch_cycles);
            self.gamepak_prefetch_cycles = 0;
            self.gamepak_prefetch_count = 0;
            if !is_32bit {
                self.note_opcode_fetch(true);
                return Some(remaining_first);
            }
            // ARM 32-bit: remaining for first halfword + full sequential for second
            let seq_second = self
                .lookup_cpu_access_timing(
                    addr.wrapping_add(2),
                    AccessKind::OpcodeFetch,
                    AccessWidth::Halfword,
                    true,
                )
                .cycles;
            self.note_opcode_fetch(true);
            return Some(remaining_first + seq_second);
        }

        None
    }

    fn consume_ready_gamepak_prefetch(
        &mut self,
        addr: u32,
        is_32bit: bool,
        sequential: bool,
    ) -> bool {
        if !sequential || !Self::is_rom_region(addr) || self.gamepak_prefetch_head_addr != addr {
            return false;
        }

        let needed = if is_32bit { 2 } else { 1 };
        if self.gamepak_prefetch_count < needed {
            return false;
        }

        self.gamepak_prefetch_count -= needed;
        self.gamepak_prefetch_head_addr = self
            .gamepak_prefetch_head_addr
            .wrapping_add(needed as u32 * 2);
        if self.gamepak_prefetch_count == 0 {
            self.gamepak_prefetch_fill_addr = self.gamepak_prefetch_head_addr;
            self.gamepak_prefetch_cycles = 0;
        }
        true
    }

    pub fn get_access_time(&self, addr: u32, is_32bit: bool, sequential: bool) -> u32 {
        self.lookup_cpu_access_timing(
            addr,
            AccessKind::DataRead,
            if is_32bit {
                AccessWidth::Word
            } else {
                AccessWidth::Halfword
            },
            sequential,
        )
        .cycles
    }

    fn access_timing_config(&self) -> AccessTimingConfig {
        AccessTimingConfig {
            waitcnt: self.waitcnt,
            internal_memory_control: self.internal_memory_control,
            dispcnt: self.ppu.registers.dispcnt,
            current_scanline: self.ppu.current_scanline,
        }
    }

    fn lookup_cpu_access_timing(
        &self,
        addr: u32,
        kind: AccessKind,
        width: AccessWidth,
        sequential: bool,
    ) -> AccessTiming {
        lookup_access_timing(
            self.access_timing_config(),
            AccessDescriptor {
                addr,
                kind,
                width,
                sequential,
            },
        )
    }

    pub fn load_rom(&mut self, data: &[u8]) {
        self.rom = data.to_vec();
        self.backup_type = detect_backup_type(data);
        self.flash_is_128k = contains_marker(data, b"FLASH1M_V");
        self.flash_bank = 0;
        self.flash_state = FlashState::Ready;
    }
    pub fn load_bios(&mut self, data: &[u8]) {
        let len = data.len().min(self.bios.len());
        self.bios[..len].copy_from_slice(&data[..len]);
    }

    fn eeprom_offset(&self, addr: u32) -> Option<usize> {
        let region = addr >> 24;
        if region != REGION_ROM_WS2 && region != (REGION_ROM_WS2 + 1) {
            return None;
        }

        let offset = (addr & 0x01FF_FFFF) as usize;
        if offset >= EEPROM_UPPER_BOUNDARY
            && (self.rom.len() <= EEPROM_UPPER_BOUNDARY || offset >= self.rom.len())
        {
            Some(offset)
        } else {
            None
        }
    }

    fn palette_offset(addr: u32) -> usize {
        (addr & 0x0000_03FF) as usize
    }

    fn vram_offset(addr: u32) -> usize {
        let mut offset = (addr & 0x0001_FFFF) as usize;
        if offset >= 0x0001_8000 {
            offset -= 0x0000_8000;
        }
        offset
    }

    fn oam_offset(addr: u32) -> usize {
        (addr & 0x0000_03FF) as usize
    }

    fn rom_offset(&self, addr: u32) -> Option<usize> {
        let offset = (addr & 0x01FF_FFFF) as usize;
        if offset < self.rom.len() {
            Some(offset)
        } else {
            None
        }
    }

    fn is_gamepak_region(addr: u32) -> bool {
        matches!(
            addr >> 24,
            REGION_ROM_WS0..=0x0D | REGION_SRAM | REGION_SRAM_MIRROR
        )
    }

    fn is_unmapped_region(addr: u32) -> bool {
        matches!(addr >> 24, 0x01 | 0x10..=0xFF)
    }

    fn dma_can_latch_source(ch: usize, addr: u32) -> bool {
        let region = addr >> 24;
        if region == REGION_BIOS || Self::is_unmapped_region(addr) {
            return false;
        }
        if ch == 0 && Self::is_gamepak_region(addr) {
            return false;
        }
        true
    }

    pub fn note_bios_prefetch(&mut self, addr: u32, value: u32, thumb: bool) {
        self.prefetch_open_bus_value = if thumb {
            let half = value as u16 as u32;
            half | (half << 16)
        } else {
            value
        };
        if addr >= 0x4000 {
            return;
        }
        self.bios_open_bus_value = if thumb {
            let half = value as u16 as u32;
            half | (half << 16)
        } else {
            value
        };
    }

    fn bios_open_bus_byte(&self, addr: u32) -> u8 {
        ((self.bios_open_bus_value >> ((addr & 3) * 8)) & 0xFF) as u8
    }

    fn prefetch_open_bus_byte(&self, addr: u32) -> u8 {
        ((self.prefetch_open_bus_value >> ((addr & 3) * 8)) & 0xFF) as u8
    }

    fn open_bus_rom_byte(&self, addr: u32) -> u8 {
        let halfword = ((addr >> 1) & 0xFFFF) as u16;
        if (addr & 1) == 0 {
            (halfword & 0x00FF) as u8
        } else {
            (halfword >> 8) as u8
        }
    }

    fn io_open_bus_byte(&self, offset: u32) -> u8 {
        self.prefetch_open_bus_byte(0x0400_0000 | offset)
    }

    fn read_sound_io8(&self, offset: u32) -> u8 {
        let reg = 0x0400_0000 | offset;
        let base_reg = reg & !1;
        let half = match base_reg {
            0x0400_0060 => self.sound_halfword(offset, 0x007F),
            0x0400_0062 => self.sound_halfword(offset, 0xFFC0),
            0x0400_0064 => self.sound_halfword(offset, 0x4000),
            0x0400_0068 => self.sound_halfword(offset, 0xFFC0),
            0x0400_006C => self.sound_halfword(offset, 0x4000),
            0x0400_0070 => self.sound_halfword(offset, 0x00E0),
            0x0400_0072 => self.sound_halfword(offset, 0xE000),
            0x0400_0074 => self.sound_halfword(offset, 0x4000),
            0x0400_0078 => self.sound_halfword(offset, 0xFF00),
            0x0400_007C => self.sound_halfword(offset, 0x40FF),
            0x0400_0080 => self.sound_halfword(offset, 0xFF77),
            0x0400_0082 => self.sound_halfword(offset, 0x770F),
            0x0400_0084 => self.sound_halfword(offset, 0x0080),
            0x0400_0090..=0x0400_009E => {
                return self.sound_io[(offset - SOUND_IO_START) as usize];
            }
            0x0400_0066 | 0x0400_006A | 0x0400_006E | 0x0400_0076 | 0x0400_007A | 0x0400_007E
            | 0x0400_0086 | 0x0400_008A => return 0,
            0x0400_0088 => {
                return self.sound_io[(offset - SOUND_IO_START) as usize];
            }
            0x0400_008C | 0x0400_008D | 0x0400_008E | 0x0400_008F | 0x0400_00A0..=0x0400_00AF => {
                return self.io_open_bus_byte(offset);
            }
            _ => return self.sound_io[(offset - SOUND_IO_START) as usize],
        };

        if (offset & 1) == 0 {
            (half & 0xFF) as u8
        } else {
            (half >> 8) as u8
        }
    }

    fn sound_halfword(&self, offset: u32, mask: u16) -> u16 {
        let base = ((offset - SOUND_IO_START) & !1) as usize;
        let lo = self.sound_io[base] as u16;
        let hi = self.sound_io[base + 1] as u16;
        (lo | (hi << 8)) & mask
    }

    fn flash_offset(&self, addr: u32) -> usize {
        let bank = if self.flash_is_128k {
            self.flash_bank
        } else {
            0
        };
        bank * FLASH_BANK_LEN + ((addr & 0x0000_FFFF) as usize)
    }

    fn erase_flash_sector(&mut self, addr: u32) {
        let start = self.flash_offset(addr) & !(FLASH_SECTOR_LEN - 1);
        let end = start + FLASH_SECTOR_LEN;
        self.sram[start..end].fill(0xFF);
        self.sram_dirty = true;
    }

    fn erase_flash_chip(&mut self) {
        let len = if self.flash_is_128k {
            SRAM_FLASH_LEN
        } else {
            FLASH_BANK_LEN
        };
        self.sram[..len].fill(0xFF);
        self.sram_dirty = true;
    }

    fn write_flash_byte(&mut self, addr: u32, value: u8) {
        let flash_addr = addr & 0x0000_FFFF;
        match self.flash_state {
            FlashState::Ready => {
                if flash_addr == 0x5555 && value == 0xAA {
                    self.flash_state = FlashState::Command55;
                }
            }
            FlashState::Command55 => {
                if flash_addr == 0x2AAA && value == 0x55 {
                    self.flash_state = FlashState::Program;
                } else {
                    self.flash_state = FlashState::Ready;
                }
            }
            FlashState::Program => match (flash_addr, value) {
                (0x5555, 0xA0) => {
                    self.flash_state = FlashState::Program;
                }
                (0x5555, 0x80) => {
                    self.flash_state = FlashState::EraseAA;
                }
                (0x5555, 0xB0) => {
                    self.flash_state = FlashState::BankSwitch;
                }
                _ => {
                    let offset = self.flash_offset(addr);
                    self.sram[offset] = value;
                    self.sram_dirty = true;
                    self.flash_state = FlashState::Ready;
                }
            },
            FlashState::EraseAA => {
                if flash_addr == 0x5555 && value == 0xAA {
                    self.flash_state = FlashState::Erase55;
                } else {
                    self.flash_state = FlashState::Ready;
                }
            }
            FlashState::Erase55 => {
                if flash_addr == 0x2AAA && value == 0x55 {
                    self.flash_state = FlashState::BankSwitch;
                } else {
                    self.flash_state = FlashState::Ready;
                }
            }
            FlashState::BankSwitch => {
                match value {
                    0x10 if flash_addr == 0x5555 => self.erase_flash_chip(),
                    0x30 => self.erase_flash_sector(addr),
                    bank if self.flash_is_128k => {
                        self.flash_bank = (bank & 1) as usize;
                    }
                    _ => {}
                }
                self.flash_state = FlashState::Ready;
            }
        }
    }

    pub fn run_dma(&mut self, ch: usize) {
        if self.dma_running {
            return;
        }
        self.dma_running = true;

        let src_ctrl = (self.dma.channels[ch].cnt >> 7) & 0x3;
        let dst_ctrl = (self.dma.channels[ch].cnt >> 5) & 0x3;
        let is_32bit = (self.dma.channels[ch].cnt & 0x0400) != 0;
        let repeat = (self.dma.channels[ch].cnt & 0x0200) != 0;
        let sound_dma_dst = self.sound_dma_fifo_target(ch);
        let sound_dma = sound_dma_dst.is_some();
        let mut count = if sound_dma {
            4
        } else {
            self.dma.channels[ch].internal_count
        };
        let mut src = self.dma.channels[ch].internal_src;
        let mut dst = self.dma.channels[ch].internal_dst;
        let total_count = count;
        let dst_is_sram = matches!(dst >> 24, REGION_SRAM | REGION_SRAM_MIRROR);
        let can_write_sram = !dst_is_sram || ch == 3;

        if self.eeprom_offset(src).is_some() || self.eeprom_offset(dst).is_some() {
            self.eeprom.notify_dma_transfer_size(total_count);
        }

        while count > 0 {
            // Tick clock for each access
            let sequential = count < self.dma.channels[ch].internal_count;
            let width = if is_32bit {
                AccessWidth::Word
            } else {
                AccessWidth::Halfword
            };
            let c = self
                .lookup_cpu_access_timing(src, AccessKind::DataRead, width, sequential)
                .cycles
                + self
                    .lookup_cpu_access_timing(dst, AccessKind::DataWrite, width, sequential)
                    .cycles;
            self.clock(c);

            if is_32bit {
                let v = if ch == 0 && matches!(src >> 24, REGION_SRAM | REGION_SRAM_MIRROR) {
                    0
                } else {
                    self.internal_read32(src)
                };
                trace_eeprom_dma(ch, src, dst, 4, v as u64, count, total_count);
                if can_write_sram {
                    self.internal_write32(sound_dma_dst.unwrap_or(dst), v);
                }
                src = match src_ctrl {
                    0 => src.wrapping_add(4),
                    1 => src.wrapping_sub(4),
                    2 => src,
                    _ => src.wrapping_add(4),
                };
                dst = match if sound_dma { 2 } else { dst_ctrl } {
                    0 | 3 => dst.wrapping_add(4),
                    1 => dst.wrapping_sub(4),
                    2 => dst,
                    _ => dst.wrapping_add(4),
                };
            } else {
                let v = if ch == 0 && matches!(src >> 24, REGION_SRAM | REGION_SRAM_MIRROR) {
                    0
                } else {
                    self.internal_read16(src)
                };
                trace_eeprom_dma(ch, src, dst, 2, v as u64, count, total_count);
                if can_write_sram {
                    self.internal_write16(sound_dma_dst.unwrap_or(dst), v);
                }
                src = match src_ctrl {
                    0 => src.wrapping_add(2),
                    1 => src.wrapping_sub(2),
                    2 => src,
                    _ => src.wrapping_add(2),
                };
                dst = match if sound_dma { 2 } else { dst_ctrl } {
                    0 | 3 => dst.wrapping_add(2),
                    1 => dst.wrapping_sub(2),
                    2 => dst,
                    _ => dst.wrapping_add(2),
                };
            }
            count -= 1;
        }

        if !repeat || ((self.dma.channels[ch].cnt >> 12) & 0x3) == 0 {
            self.dma.channels[ch].cnt &= !0x8000;
        } else {
            // For repeat DMA, internal addresses are updated, but count is reloaded
            self.dma.channels[ch].internal_count = if sound_dma {
                4
            } else if self.dma.channels[ch].count == 0 {
                if ch == 3 { 0x10000 } else { 0x4000 }
            } else {
                self.dma.channels[ch].count as u32
            };
            self.dma.channels[ch].internal_src = src;
            // Destination depends on repeat behavior: some reload, some continue.
            // In GBA, bit 5-6 set to 3 reloads destination.
            if sound_dma {
                self.dma.channels[ch].internal_dst = sound_dma_dst.unwrap();
            } else if dst_ctrl == 3 {
                self.dma.channels[ch].internal_dst = self.dma.channels[ch].dst;
            } else {
                self.dma.channels[ch].internal_dst = dst;
            }
        }

        if (self.dma.channels[ch].cnt & 0x4000) != 0 {
            self.if_ |= 1 << (8 + ch);
            self.halted = false;
        }
        self.dma_running = false;
    }

    fn start_dma_channel(&mut self, ch: usize) {
        let is_32bit = (self.dma.channels[ch].cnt & 0x0400) != 0;
        let align_mask = if is_32bit { !3 } else { !1 };
        let requested_src = self.dma.channels[ch].src & align_mask;
        let requested_dst = self.dma.channels[ch].dst & align_mask;
        if Self::dma_can_latch_source(ch, requested_src)
            || (ch == 0 && matches!(requested_src >> 24, REGION_SRAM | REGION_SRAM_MIRROR))
        {
            self.dma.channels[ch].internal_src = requested_src;
        }
        // Latch the requested destination unconditionally and let the destination-side
        // write path decide whether the transfer has an effect. Filtering here leaves
        // stale internal_dst values around and breaks commercial ROM DMA setup.
        self.dma.channels[ch].internal_dst = requested_dst;
        self.dma.channels[ch].internal_count = if self.dma.channels[ch].count == 0 {
            if ch == 3 { 0x10000 } else { 0x4000 }
        } else {
            self.dma.channels[ch].count as u32
        };
        trace_dma_start(
            ch,
            self.dma.channels[ch].src,
            self.dma.channels[ch].dst,
            self.dma.channels[ch].internal_count,
            self.dma.channels[ch].cnt,
        );
        if (self.dma.channels[ch].cnt & 0x3000) == 0 {
            self.pending_dma |= 1 << ch;
        }
    }

    fn read_io8(&self, offset: u32) -> u8 {
        if let Some(value) = ppu_io::read_register_byte(&self.ppu.registers, offset) {
            return value;
        }

        let value = match offset {
            0x0000..=0x005F => self.io_open_bus_byte(offset),
            SOUND_IO_START..=SOUND_IO_END => self.read_sound_io8(offset),
            0x00A8..=0x00AF => self.io_open_bus_byte(offset),
            0x00B0..=0x00DF => {
                let reg = (offset - 0x00B0) % 12;
                if reg < 8 {
                    self.io_open_bus_byte(offset)
                } else {
                    self.dma.read_register_byte(offset)
                }
            }
            0x0100..=0x010F => self.timers.read_register_byte(offset),
            0x00E0..=0x00FF => self.io_open_bus_byte(offset),
            0x100C..=0x100F => self.io_open_bus_byte(offset),
            _ => self.read_system_io8(offset),
        };
        value
    }

    fn write_io8(&mut self, offset: u32, value: u8) {
        if ppu_io::write_register_byte(&mut self.ppu.registers, offset, value) {
            self.ppu.sync_internal_affine_reference(offset);
            return;
        }

        match offset {
            SOUND_IO_START..=SOUND_IO_END => {
                self.sound_io[(offset - SOUND_IO_START) as usize] = value;
                self.push_sound_fifo_byte(offset, value);
                self.apply_sound_control_side_effects();
            }
            0x00B0..=0x00DF => {
                if let Some(ch) = self.dma.write_register_byte(offset, value) {
                    self.start_dma_channel(ch);
                    self.service_pending_dma();
                }
            }
            0x0100..=0x010F => self.timers.write_register_byte(offset, value),
            _ => self.write_system_io8(offset, value),
        }
    }

    fn write_io32(&mut self, offset: u32, value: u32) {
        if ppu_io::write_register_word(&mut self.ppu.registers, offset, value) {
            self.ppu.sync_internal_affine_reference(offset);
            return;
        }

        match offset {
            0x0208 => self.ime = value & 1,
            0x00B0..=0x00DF => {
                let ch = ((offset - 0x00B0) / 12) as usize;
                let reg = (offset - 0x00B0) % 12;
                if reg == 0 {
                    self.dma.channels[ch].src = value;
                } else if reg == 4 {
                    self.dma.channels[ch].dst = value;
                } else {
                    self.internal_write16(0x0400_0000 | offset, (value & 0xFFFF) as u16);
                    self.internal_write16(0x0400_0000 | offset | 2, (value >> 16) as u16);
                }
            }
            _ => {
                self.internal_write16(0x0400_0000 | (offset & !3), (value & 0xFFFF) as u16);
                self.internal_write16(0x0400_0000 | offset | 2, (value >> 16) as u16);
            }
        }
    }

    fn read_system_io8(&self, offset: u32) -> u8 {
        if trace_keys_enabled() && offset == 0x0130 && self.key_state != 0x03FF {
            static KEY_TRACE_COUNT: AtomicU32 = AtomicU32::new(0);
            let count = KEY_TRACE_COUNT.fetch_add(1, Ordering::Relaxed);
            if count < 32 || count % 256 == 0 {
                eprintln!(
                    "[keys] pc={:08X} keyinput={:04X}",
                    self.pc_at_access, self.key_state
                );
            }
        }
        match offset {
            0x0130 => (self.key_state & 0x00FF) as u8,
            0x0131 => ((self.key_state >> 8) as u8) & 0x03,
            0x0200 => (self.ie & 0xFF) as u8,
            0x0201 => (self.ie >> 8) as u8,
            0x0202 => (self.if_ & 0xFF) as u8,
            0x0203 => (self.if_ >> 8) as u8,
            0x0208 => (self.ime & 0xFF) as u8,
            0x0300 => self.postflg,
            0x0800 => (self.internal_memory_control & 0xFF) as u8,
            0x0801 => ((self.internal_memory_control >> 8) & 0xFF) as u8,
            0x0802 => ((self.internal_memory_control >> 16) & 0xFF) as u8,
            0x0803 => ((self.internal_memory_control >> 24) & 0xFF) as u8,
            _ => 0,
        }
    }

    fn write_system_io8(&mut self, offset: u32, value: u8) {
        match offset {
            0x0200 => self.ie = (self.ie & 0xFF00) | (value as u16),
            0x0201 => self.ie = (self.ie & 0x00FF) | ((value as u16) << 8),
            0x0202 => self.if_ &= !(value as u16),
            0x0203 => self.if_ &= !((value as u16) << 8),
            0x0204 => self.waitcnt = (self.waitcnt & 0xFF00) | (value as u16),
            0x0205 => self.waitcnt = (self.waitcnt & 0x00FF) | ((value as u16) << 8),
            0x0208 => self.ime = (self.ime & 0xFFFF_FF00) | (value as u32),
            0x0209 | 0x020A | 0x020B => {}
            0x0300 => self.postflg = value & 1,
            0x0301 => {
                if value & 0x80 == 0 {
                    self.halted = true;
                }
            }
            0x0800..=0x0803 => {
                let shift = (offset - 0x0800) * 8;
                let mask = !(0x00FFu32 << shift);
                self.internal_memory_control =
                    (self.internal_memory_control & mask) | ((value as u32) << shift);
            }
            _ => {}
        }
    }

    pub fn internal_read8_prot(&self, addr: u32) -> u8 {
        if addr < 0x4000 && self.pc_at_access >= 0x02000000 {
            self.bios_open_bus_byte(addr)
        } else {
            self.internal_read8(addr)
        }
    }
    pub fn internal_read16_prot(&mut self, addr: u32) -> u16 {
        if addr < 0x4000 && self.pc_at_access >= 0x02000000 {
            (self.internal_read8_prot(addr & !1) as u16)
                | ((self.internal_read8_prot(addr | 1) as u16) << 8)
        } else {
            self.internal_read16(addr)
        }
    }
    pub fn internal_read32_prot(&mut self, addr: u32) -> u32 {
        if addr < 0x4000 && self.pc_at_access >= 0x02000000 {
            self.internal_read16_prot(addr & !3) as u32
                | ((self.internal_read16_prot(addr | 2) as u32) << 16)
        } else {
            self.internal_read32(addr)
        }
    }

    pub fn internal_read16(&mut self, addr: u32) -> u16 {
        if self.eeprom_offset(addr).is_some() {
            self.eeprom.read_halfword()
        } else if matches!(addr >> 24, REGION_SRAM | REGION_SRAM_MIRROR)
            && matches!(self.backup_type, BackupType::Sram | BackupType::Flash)
        {
            let offset = if self.backup_type == BackupType::Flash {
                self.flash_offset(addr)
            } else {
                (addr & 0x0000_FFFF) as usize
            };
            let value = self.sram[offset] as u16;
            value | (value << 8)
        } else {
            (self.internal_read8(addr & !1) as u16) | ((self.internal_read8(addr | 1) as u16) << 8)
        }
    }
    pub fn internal_read32(&mut self, addr: u32) -> u32 {
        if matches!(addr >> 24, REGION_SRAM | REGION_SRAM_MIRROR)
            && matches!(self.backup_type, BackupType::Sram | BackupType::Flash)
        {
            let offset = if self.backup_type == BackupType::Flash {
                self.flash_offset(addr)
            } else {
                (addr & 0x0000_FFFF) as usize
            };
            let value = self.sram[offset] as u32;
            value | (value << 8) | (value << 16) | (value << 24)
        } else {
            self.internal_read16(addr & !3) as u32 | ((self.internal_read16(addr | 2) as u32) << 16)
        }
    }

    pub fn internal_write16(&mut self, addr: u32, value: u16) {
        trace_watch_write(self, self.pc_at_access, addr, value as u64, 2);
        if self.eeprom_offset(addr).is_some() {
            self.eeprom.write_halfword(value);
            return;
        }

        match addr >> 24 {
            REGION_PALETTE => {
                let addr = addr & !1;
                let offset = Self::palette_offset(addr);
                self.palette_ram[offset] = (value & 0xFF) as u8;
                self.palette_ram[offset + 1] = (value >> 8) as u8;
                return;
            }
            REGION_VRAM => {
                let addr = addr & !1;
                let offset = Self::vram_offset(addr);
                if offset + 1 < self.vram.len() {
                    self.vram[offset] = (value & 0xFF) as u8;
                    self.vram[offset + 1] = (value >> 8) as u8;
                }
                return;
            }
            REGION_OAM => {
                let addr = addr & !1;
                let offset = Self::oam_offset(addr);
                if offset + 1 < self.oam.len() {
                    self.oam[offset] = (value & 0xFF) as u8;
                    self.oam[offset + 1] = (value >> 8) as u8;
                }
                return;
            }
            REGION_SRAM | REGION_SRAM_MIRROR
                if matches!(self.backup_type, BackupType::Sram | BackupType::Flash) =>
            {
                if self.backup_type == BackupType::Flash {
                    self.write_flash_byte(addr, ((value >> ((addr & 1) * 8)) & 0xFF) as u8);
                } else {
                    let shift = (addr & 1) * 8;
                    self.sram[(addr & 0xFFFF) as usize] = ((value >> shift) & 0xFF) as u8;
                    self.sram_dirty = true;
                }
                return;
            }
            _ => {}
        }

        let addr = addr & !1;
        match addr {
            0x04000208 => self.ime = value as u32 & 1,
            _ => {
                self.internal_write8(addr, (value & 0xFF) as u8);
                self.internal_write8(addr | 1, (value >> 8) as u8);
            }
        }
    }

    pub fn internal_write32(&mut self, addr: u32, value: u32) {
        trace_watch_write(self, self.pc_at_access, addr, value as u64, 4);
        match addr >> 24 {
            REGION_IO => {
                let offset = addr & 0x0000FFFF;
                self.write_io32(offset, value);
            }
            REGION_SRAM | REGION_SRAM_MIRROR
                if matches!(self.backup_type, BackupType::Sram | BackupType::Flash) =>
            {
                if self.backup_type == BackupType::Flash {
                    self.write_flash_byte(addr, ((value >> ((addr & 3) * 8)) & 0xFF) as u8);
                } else {
                    let shift = (addr & 3) * 8;
                    self.sram[(addr & 0xFFFF) as usize] = ((value >> shift) & 0xFF) as u8;
                    self.sram_dirty = true;
                }
            }
            _ => {
                self.internal_write16(addr & !3, (value & 0xFFFF) as u16);
                self.internal_write16(addr | 2, (value >> 16) as u16);
            }
        }
    }

    pub fn internal_read8(&self, addr: u32) -> u8 {
        match addr >> 24 {
            REGION_BIOS => {
                let offset = (addr & 0x00FFFFFF) as usize;
                if offset < self.bios.len() {
                    self.bios[offset]
                } else {
                    self.prefetch_open_bus_byte(addr)
                }
            }
            REGION_EWRAM => self.on_board_wram[(addr & 0x0003FFFF) as usize],
            REGION_IWRAM => self.on_chip_wram[(addr & 0x00007FFF) as usize],
            REGION_IO => self.read_io8(addr & 0x0000FFFF),
            REGION_PALETTE => self.palette_ram[Self::palette_offset(addr)],
            REGION_VRAM => self.vram[Self::vram_offset(addr)],
            REGION_OAM => self.oam[Self::oam_offset(addr)],
            REGION_ROM_WS0..=0x0D => self
                .rom_offset(addr)
                .map(|offset| self.rom[offset])
                .unwrap_or_else(|| {
                    let value = self.open_bus_rom_byte(addr);
                    trace_rom_oob_read(addr, self.pc_at_access, value);
                    value
                }),
            REGION_SRAM | REGION_SRAM_MIRROR => match self.backup_type {
                BackupType::Sram => self.sram[(addr & 0x0000FFFF) as usize],
                BackupType::Flash => self.sram[self.flash_offset(addr)],
                BackupType::Eeprom | BackupType::Unknown => 0xFF,
            },
            _ => self.prefetch_open_bus_byte(addr),
        }
    }

    pub fn internal_write8(&mut self, addr: u32, value: u8) {
        trace_watch_write(self, self.pc_at_access, addr, value as u64, 1);
        match addr >> 24 {
            REGION_EWRAM => self.on_board_wram[(addr & 0x0003FFFF) as usize] = value,
            REGION_IWRAM => {
                self.on_chip_wram[(addr & 0x00007FFF) as usize] = value;
            }
            REGION_IO => self.write_io8(addr & 0x0000FFFF, value),
            REGION_PALETTE => {
                let aligned = addr & !1;
                let offset = Self::palette_offset(aligned);
                if offset + 1 < self.palette_ram.len() {
                    self.palette_ram[offset] = value;
                    self.palette_ram[offset + 1] = value;
                }
            }
            REGION_OAM => {} // 8-bit writes to OAM are ignored
            REGION_VRAM => {
                let aligned = addr & !1;
                let offset = Self::vram_offset(aligned);
                if (0x0001_0000..0x0001_8000).contains(&offset) {
                    return;
                }
                if offset + 1 < self.vram.len() {
                    self.vram[offset] = value;
                    self.vram[offset + 1] = value;
                }
            }
            REGION_SRAM | REGION_SRAM_MIRROR => match self.backup_type {
                BackupType::Sram => {
                    self.sram[(addr & 0xFFFF) as usize] = value;
                    self.sram_dirty = true;
                }
                BackupType::Flash => self.write_flash_byte(addr, value),
                BackupType::Eeprom | BackupType::Unknown => {}
            },
            _ => {}
        }
    }
}

fn detect_backup_type(rom: &[u8]) -> BackupType {
    if contains_marker(rom, b"EEPROM_V") {
        BackupType::Eeprom
    } else if contains_marker(rom, b"SRAM_V") || contains_marker(rom, b"SRAM_F_V") {
        BackupType::Sram
    } else if contains_marker(rom, b"FLASH_V")
        || contains_marker(rom, b"FLASH512_V")
        || contains_marker(rom, b"FLASH1M_V")
    {
        BackupType::Flash
    } else {
        BackupType::Unknown
    }
}

fn contains_marker(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn trace_rom_oob_read(addr: u32, pc: u32, value: u8) {
    static TRACE_ROM_OOB: OnceLock<bool> = OnceLock::new();
    if !*TRACE_ROM_OOB.get_or_init(|| std::env::var_os("VIBE_TRACE_ROM_OOB").is_some()) {
        return;
    }

    eprintln!("[rom-oob] pc={pc:08X} addr={addr:08X} value={value:02X}");
}

fn trace_eeprom_dma(ch: usize, src: u32, dst: u32, width: u8, value: u64, count: u32, total: u32) {
    static TRACE_COUNT: OnceLock<bool> = OnceLock::new();
    if !*TRACE_COUNT.get_or_init(|| std::env::var_os("VIBE_TRACE_EEPROM_DMA").is_some()) {
        return;
    }

    let remaining = count.saturating_sub(1);
    let index = total.saturating_sub(remaining + 1);
    if total <= 81 && index < 12 {
        eprintln!(
            "[eeprom-dma] ch={} step={}/{} src={:08X} dst={:08X} width={} value={:0width$X}",
            ch,
            index + 1,
            total,
            src,
            dst,
            width * 8,
            value,
            width = (width as usize) * 2
        );
    }
}

fn trace_watch_addr() -> Option<u32> {
    static WATCH_ADDR: OnceLock<Option<u32>> = OnceLock::new();
    *WATCH_ADDR.get_or_init(|| {
        std::env::var("VIBE_WATCH_ADDR")
            .ok()
            .and_then(|value| u32::from_str_radix(value.trim_start_matches("0x"), 16).ok())
    })
}

fn trace_watch_write(bus: &Bus, pc: u32, addr: u32, value: u64, size: u8) {
    let Some(watch_addr) = trace_watch_addr() else {
        return;
    };

    let write_start = addr;
    let write_end = addr + size as u32 - 1;
    if (write_start..=write_end).contains(&watch_addr) {
        eprintln!(
            "[watch] pc={pc:08X} scanline={} line_cycle={} write{} addr={addr:08X} value={value:0width$X}",
            bus.ppu.current_scanline,
            bus.ppu.cycles,
            size * 8,
            width = (size as usize) * 2
        );
    }
}

fn trace_irq_request(kind: &str, bus: &Bus) {
    static TRACE_IRQ: OnceLock<bool> = OnceLock::new();
    if !*TRACE_IRQ.get_or_init(|| std::env::var_os("VIBE_TRACE_IRQ").is_some()) {
        return;
    }

    eprintln!(
        "[irq-req] kind={kind} scanline={} line_cycle={} halt={} ie={:04X} if={:04X} ime={}",
        bus.ppu.current_scanline, bus.ppu.cycles, bus.halted, bus.ie, bus.if_, bus.ime,
    );
}

fn trace_keys_enabled() -> bool {
    static TRACE_KEYS: OnceLock<bool> = OnceLock::new();
    *TRACE_KEYS.get_or_init(|| std::env::var_os("VIBE_TRACE_KEYS").is_some())
}

fn trace_dma_start(ch: usize, src: u32, dst: u32, count: u32, cnt: u16) {
    static TRACE_DMA: OnceLock<bool> = OnceLock::new();
    if !*TRACE_DMA.get_or_init(|| std::env::var_os("VIBE_TRACE_DMA").is_some()) {
        return;
    }

    eprintln!("[dma] ch={ch} src={src:08X} dst={dst:08X} count={count:04X} cnt={cnt:04X}");
}

fn trace_timing_bus_event(
    phase: &str,
    bus: &Bus,
    addr: u32,
    kind: AccessKind,
    width: AccessWidth,
    sequential: bool,
    cycles: u32,
) {
    static TRACE_TIMING: OnceLock<bool> = OnceLock::new();
    if !*TRACE_TIMING.get_or_init(|| std::env::var_os("VIBE_TRACE_TIMING").is_some()) {
        return;
    }

    eprintln!(
        "[timing-bus] {phase} pc={:08X} addr={addr:08X} kind={kind:?} width={width:?} seq={} cyc={} pref_count={} pref_cyc={} head={:08X} fill={:08X} block={}",
        bus.pc_at_access,
        sequential as u8,
        cycles,
        bus.gamepak_prefetch_count,
        bus.gamepak_prefetch_cycles,
        bus.gamepak_prefetch_head_addr,
        bus.gamepak_prefetch_fill_addr,
        bus.gamepak_prefetch_block_cycles,
    );
}

fn trace_timing_dma(phase: &str, bus: &Bus, ch: usize, count: u32) {
    static TRACE_TIMING: OnceLock<bool> = OnceLock::new();
    if !*TRACE_TIMING.get_or_init(|| std::env::var_os("VIBE_TRACE_TIMING").is_some()) {
        return;
    }
    eprintln!(
        "[timing-dma] {} pc={:08X} ch={} count={} cycles={} scanline={} line_cycle={} pending={:04X} running={} src={:08X} dst={:08X} cnt={:04X}",
        phase,
        bus.pc_at_access,
        ch,
        count,
        bus.cycles,
        bus.ppu.current_scanline,
        bus.ppu.cycles,
        bus.pending_dma,
        bus.dma_running as u8,
        bus.dma.channels[ch].internal_src,
        bus.dma.channels[ch].internal_dst,
        bus.dma.channels[ch].cnt,
    );
}

fn trace_bg2_hblank_dma(bus: &Bus, ch: usize) {
    static TRACE_BG2_HDMA: OnceLock<bool> = OnceLock::new();
    if !*TRACE_BG2_HDMA.get_or_init(|| std::env::var_os("VIBE_TRACE_BG2_HDMA").is_some()) {
        return;
    }

    let channel = &bus.dma.channels[ch];
    if ch != 0
        || channel.internal_dst != 0x0400_0020
        || channel.internal_count != 4
        || channel.cnt != 0xA660
    {
        return;
    }

    fn read_ram_word(bus: &Bus, addr: u32) -> Option<u32> {
        match addr >> 24 {
            REGION_EWRAM => {
                let offset = (addr & 0x0003_FFFF) as usize;
                if offset + 3 < bus.on_board_wram.len() {
                    Some(u32::from_le_bytes([
                        bus.on_board_wram[offset],
                        bus.on_board_wram[offset + 1],
                        bus.on_board_wram[offset + 2],
                        bus.on_board_wram[offset + 3],
                    ]))
                } else {
                    None
                }
            }
            REGION_IWRAM => {
                let offset = (addr & 0x0000_7FFF) as usize;
                if offset + 3 < bus.on_chip_wram.len() {
                    Some(u32::from_le_bytes([
                        bus.on_chip_wram[offset],
                        bus.on_chip_wram[offset + 1],
                        bus.on_chip_wram[offset + 2],
                        bus.on_chip_wram[offset + 3],
                    ]))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    let src = channel.internal_src;
    let w0 = read_ram_word(bus, src).unwrap_or(0);
    let w1 = read_ram_word(bus, src.wrapping_add(4)).unwrap_or(0);
    let w2 = read_ram_word(bus, src.wrapping_add(8)).unwrap_or(0);
    let w3 = read_ram_word(bus, src.wrapping_add(12)).unwrap_or(0);
    eprintln!(
        "[bg2-hdma] y={} line_cycle={} src={:08X} w0={:08X} w1={:08X} w2={:08X} w3={:08X}",
        bus.ppu.current_scanline, bus.ppu.cycles, src, w0, w1, w2, w3
    );
}
