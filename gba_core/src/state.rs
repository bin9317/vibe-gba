use crate::Gba;
use crate::bus::{BackupType, Bus, SoundFifo};
use crate::cpu::Cpu;
use crate::dma::Dma;
use crate::eeprom::{Eeprom, EepromState};
use crate::ppu::registers::PpuRegisters;
use crate::timers::Timers;
use serde::{Deserialize, Serialize};

const SAVE_STATE_VERSION: u32 = 5;

#[derive(Serialize, Deserialize)]
struct SaveStateFile {
    version: u32,
    state: GbaState,
}

#[derive(Serialize, Deserialize)]
struct GbaState {
    cpu: Cpu,
    bus: BusState,
}

#[derive(Serialize, Deserialize)]
struct BusState {
    on_board_wram: Vec<u8>,
    on_chip_wram: Vec<u8>,
    palette_ram: Vec<u8>,
    vram: Vec<u8>,
    oam: Vec<u8>,
    sram: Vec<u8>,
    eeprom: EepromData,
    ppu: PpuState,
    dma: Dma,
    timers: Timers,
    sound_io: Vec<u8>,
    ie: u16,
    if_: u16,
    ime: u32,
    postflg: u8,
    halted: bool,
    waitcnt: u16,
    internal_memory_control: u32,
    cycles: u64,
    last_data_address: u32,
    last_data_valid: bool,
    last_data_used_gamepak_bus: bool,
    next_gamepak_fetch_is_sequential: bool,
    pc_at_access: u32,
    bios_open_bus_value: u32,
    key_state: u16,
    sram_dirty: bool,
    pending_dma: u16,
    dma_running: bool,
    backup_type: BackupType,
    flash_is_128k: bool,
    flash_bank: usize,
}

#[derive(Serialize, Deserialize)]
struct PpuState {
    registers: PpuRegisters,
    frame_buffer: Vec<u8>,
    current_scanline: u16,
    cycles: u32,
    scanline_rendered: bool,
}

#[derive(Serialize, Deserialize)]
struct EepromData {
    data: Vec<u8>,
    state: EepromState,
    command: u128,
    bit_count: usize,
    read_buffer: u64,
    address_bits: usize,
    dirty: bool,
}

impl Gba {
    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        let file = SaveStateFile {
            version: SAVE_STATE_VERSION,
            state: GbaState::from_gba(self),
        };
        bincode::serialize(&file).map_err(|err| format!("failed to serialize save state: {err}"))
    }

    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        let file: SaveStateFile = bincode::deserialize(bytes)
            .map_err(|err| format!("failed to deserialize save state: {err}"))?;
        if file.version != SAVE_STATE_VERSION {
            return Err(format!(
                "unsupported save state version {} (expected {})",
                file.version, SAVE_STATE_VERSION
            ));
        }
        file.state.apply_to(self)
    }
}

impl GbaState {
    fn from_gba(gba: &Gba) -> Self {
        Self {
            cpu: gba.cpu.clone(),
            bus: BusState::from_bus(&gba.bus),
        }
    }

    fn apply_to(self, gba: &mut Gba) -> Result<(), String> {
        gba.cpu = self.cpu;
        self.bus.apply_to(&mut gba.bus)?;
        gba.bus.reconstruct_gamepak_prefetch_after_load(
            gba.cpu.pipeline_valid,
            gba.cpu.pipeline_next_fetch,
            gba.cpu.timing.fetch.valid,
            gba.cpu.timing.fetch.pc,
        );
        Ok(())
    }
}

impl BusState {
    fn from_bus(bus: &Bus) -> Self {
        let (
            last_data_address,
            last_data_valid,
            last_data_used_gamepak_bus,
            next_gamepak_fetch_is_sequential,
        ) = bus.cpu_bus_timing_snapshot();
        Self {
            on_board_wram: bus.on_board_wram.to_vec(),
            on_chip_wram: bus.on_chip_wram.to_vec(),
            palette_ram: bus.palette_ram.to_vec(),
            vram: bus.vram.to_vec(),
            oam: bus.oam.to_vec(),
            sram: bus.sram.to_vec(),
            eeprom: EepromData::from_eeprom(&bus.eeprom),
            ppu: PpuState::from_bus(bus),
            dma: bus.dma.clone(),
            timers: bus.timers.clone(),
            sound_io: bus.sound_io.to_vec(),
            ie: bus.ie,
            if_: bus.if_,
            ime: bus.ime,
            postflg: bus.postflg,
            halted: bus.halted,
            waitcnt: bus.waitcnt,
            internal_memory_control: bus.internal_memory_control,
            cycles: bus.cycles,
            last_data_address,
            last_data_valid,
            last_data_used_gamepak_bus,
            next_gamepak_fetch_is_sequential,
            pc_at_access: bus.pc_at_access,
            bios_open_bus_value: bus.bios_open_bus_value,
            key_state: bus.key_state,
            sram_dirty: bus.sram_dirty,
            pending_dma: bus.pending_dma,
            dma_running: bus.dma_running,
            backup_type: bus.backup_type,
            flash_is_128k: bus.flash_is_128k,
            flash_bank: bus.flash_bank,
        }
    }

    fn apply_to(self, bus: &mut Bus) -> Result<(), String> {
        copy_exact(
            &mut bus.on_board_wram[..],
            &self.on_board_wram,
            "on_board_wram",
        )?;
        copy_exact(
            &mut bus.on_chip_wram[..],
            &self.on_chip_wram,
            "on_chip_wram",
        )?;
        copy_exact(&mut bus.palette_ram[..], &self.palette_ram, "palette_ram")?;
        copy_exact(&mut bus.vram[..], &self.vram, "vram")?;
        copy_exact(&mut bus.oam[..], &self.oam, "oam")?;
        copy_exact(&mut bus.sram[..], &self.sram, "sram")?;
        copy_exact(&mut bus.sound_io[..], &self.sound_io, "sound_io")?;
        bus.sound_fifos = [SoundFifo::new(), SoundFifo::new()];

        self.eeprom.apply_to(&mut bus.eeprom)?;
        self.ppu.apply_to(bus)?;
        bus.dma = self.dma;
        bus.timers = self.timers;
        bus.ie = self.ie;
        bus.if_ = self.if_;
        bus.ime = self.ime;
        bus.postflg = self.postflg;
        bus.halted = self.halted;
        bus.waitcnt = self.waitcnt;
        bus.internal_memory_control = self.internal_memory_control;
        bus.cycles = self.cycles;
        bus.restore_cpu_bus_timing(
            self.last_data_address,
            self.last_data_valid,
            self.last_data_used_gamepak_bus,
            self.next_gamepak_fetch_is_sequential,
        );
        bus.pc_at_access = self.pc_at_access;
        bus.bios_open_bus_value = self.bios_open_bus_value;
        bus.key_state = self.key_state;
        bus.sram_dirty = self.sram_dirty;
        bus.pending_dma = self.pending_dma;
        bus.dma_running = self.dma_running;
        bus.backup_type = self.backup_type;
        bus.flash_is_128k = self.flash_is_128k;
        bus.flash_bank = self.flash_bank;

        Ok(())
    }
}

impl PpuState {
    fn from_bus(bus: &Bus) -> Self {
        Self {
            registers: bus.ppu.registers.clone(),
            frame_buffer: bus.ppu.frame_buffer.to_vec(),
            current_scanline: bus.ppu.current_scanline,
            cycles: bus.ppu.cycles,
            scanline_rendered: bus.ppu.scanline_rendered,
        }
    }

    fn apply_to(self, bus: &mut Bus) -> Result<(), String> {
        bus.ppu.registers = self.registers;
        copy_exact(
            &mut bus.ppu.frame_buffer[..],
            &self.frame_buffer,
            "ppu_frame_buffer",
        )?;
        bus.ppu.current_scanline = self.current_scanline;
        bus.ppu.cycles = self.cycles;
        bus.ppu.scanline_rendered = self.scanline_rendered;
        bus.ppu.reset_transient_state();
        Ok(())
    }
}

impl EepromData {
    fn from_eeprom(eeprom: &Eeprom) -> Self {
        Self {
            data: eeprom.data.to_vec(),
            state: eeprom.state,
            command: eeprom.command,
            bit_count: eeprom.bit_count,
            read_buffer: eeprom.read_buffer,
            address_bits: eeprom.address_bits,
            dirty: eeprom.dirty,
        }
    }

    fn apply_to(self, eeprom: &mut Eeprom) -> Result<(), String> {
        copy_exact(&mut eeprom.data[..], &self.data, "eeprom_data")?;
        eeprom.state = self.state;
        eeprom.command = self.command;
        eeprom.bit_count = self.bit_count;
        eeprom.read_buffer = self.read_buffer;
        eeprom.address_bits = self.address_bits;
        eeprom.dirty = self.dirty;
        Ok(())
    }
}

fn copy_exact(dst: &mut [u8], src: &[u8], label: &str) -> Result<(), String> {
    if dst.len() != src.len() {
        return Err(format!(
            "{label} size mismatch: expected {} bytes, got {}",
            dst.len(),
            src.len()
        ));
    }
    dst.copy_from_slice(src);
    Ok(())
}
