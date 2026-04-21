#![allow(dead_code)]

use gba_core::GAMEPAK_ROM_START;
use gba_core::bus::Bus;
use gba_core::cpu::Cpu;

pub const DMA_IWRAM_SRC: u32 = 0x0300_0000;
pub const DMA_IWRAM_DST: u32 = 0x0300_0040;
pub const DMA_ROM_SRC: u32 = 0x0800_0100;
pub const DMA_ROM_DST: u32 = 0x0800_0140;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaTimingTrace {
    pub total_cycles: u64,
    pub dma_dst_word0: u32,
    pub dma_dst_word1: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaCpuStepTrace {
    pub total_cycles: u64,
    pub last_fetch_cycles: u32,
    pub last_fetch_used_prefetch: bool,
    pub dma_dst_word0: u32,
    pub dma_dst_word1: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaSuiteLikeTrace {
    pub total_cycles: u64,
    pub timer_value: u16,
    pub dma_dst_word0: u32,
    pub dma_dst_word1: u32,
}

pub fn write_rom_word(rom: &mut [u8], addr: u32, value: u32) {
    let offset = (addr - gba_core::GAMEPAK_ROM_START) as usize;
    rom[offset] = (value & 0xFF) as u8;
    rom[offset + 1] = ((value >> 8) & 0xFF) as u8;
    rom[offset + 2] = ((value >> 16) & 0xFF) as u8;
    rom[offset + 3] = ((value >> 24) & 0xFF) as u8;
}

pub fn run_dma3_transfer(control: u16, src: u32, dst: u32, count: u32) -> DmaTimingTrace {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x400];
    write_rom_word(&mut rom, DMA_ROM_SRC, 0xA0B0_C0D0);
    write_rom_word(&mut rom, DMA_ROM_SRC + 4, 0xA1B1_C1D1);
    write_rom_word(&mut rom, DMA_ROM_DST, 0xDEAD_BEEF);
    write_rom_word(&mut rom, DMA_ROM_DST + 4, 0xDEAD_BEEF);
    bus.load_rom(&rom);

    bus.internal_write32(DMA_IWRAM_SRC, 0x1122_3344);
    bus.internal_write32(DMA_IWRAM_SRC + 4, 0x5566_7788);
    bus.internal_write32(DMA_IWRAM_DST, 0xDEAD_BEEF);
    bus.internal_write32(DMA_IWRAM_DST + 4, 0xDEAD_BEEF);

    bus.dma.channels[3].src = src;
    bus.dma.channels[3].dst = dst;
    bus.dma.channels[3].count = count as u16;
    bus.dma.channels[3].cnt = control;
    bus.dma.channels[3].internal_src = src & if (control & 0x0400) != 0 { !3 } else { !1 };
    bus.dma.channels[3].internal_dst = dst & if (control & 0x0400) != 0 { !3 } else { !1 };
    bus.dma.channels[3].internal_count = count;

    bus.run_dma(3);

    let (read0, read1) = if (dst >> 24) == 0x08 {
        (
            bus.internal_read32(DMA_ROM_DST),
            bus.internal_read32(DMA_ROM_DST + 4),
        )
    } else {
        (
            bus.internal_read32(DMA_IWRAM_DST),
            bus.internal_read32(DMA_IWRAM_DST + 4),
        )
    };

    DmaTimingTrace {
        total_cycles: bus.cycles,
        dma_dst_word0: read0,
        dma_dst_word1: read1,
    }
}

pub fn run_thumb_dma_enable_step(
    timed_instr: u16,
    waitcnt: u16,
    control: u16,
    src: u32,
    dst: u32,
    count: u16,
) -> DmaCpuStepTrace {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x400];
    write_rom_word(
        &mut rom,
        GAMEPAK_ROM_START,
        (timed_instr as u32) | ((0x46C0u16 as u32) << 16),
    );
    write_rom_word(&mut rom, DMA_ROM_SRC, 0xA0B0_C0D0);
    write_rom_word(&mut rom, DMA_ROM_SRC + 4, 0xA1B1_C1D1);
    write_rom_word(&mut rom, DMA_ROM_DST, 0xDEAD_BEEF);
    write_rom_word(&mut rom, DMA_ROM_DST + 4, 0xDEAD_BEEF);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;

    bus.internal_write32(DMA_IWRAM_SRC, 0x1122_3344);
    bus.internal_write32(DMA_IWRAM_SRC + 4, 0x5566_7788);
    bus.internal_write32(DMA_IWRAM_DST, 0xDEAD_BEEF);
    bus.internal_write32(DMA_IWRAM_DST + 4, 0xDEAD_BEEF);

    bus.dma.channels[3].src = src;
    bus.dma.channels[3].dst = dst;
    bus.dma.channels[3].count = count;
    bus.dma.channels[3].cnt = control & !0x8000;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_003F;
    cpu.registers[2] = 0x0400_00D4;
    cpu.registers[3] = ((control as u32) << 16) | count as u32;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);

    let (read0, read1) = if (dst >> 24) == 0x08 {
        (
            bus.internal_read32(DMA_ROM_DST),
            bus.internal_read32(DMA_ROM_DST + 4),
        )
    } else {
        (
            bus.internal_read32(DMA_IWRAM_DST),
            bus.internal_read32(DMA_IWRAM_DST + 4),
        )
    };

    DmaCpuStepTrace {
        total_cycles: bus.cycles,
        last_fetch_cycles: cpu.timing.last_opcode_fetch_cycles,
        last_fetch_used_prefetch: cpu.timing.last_opcode_used_prefetch,
        dma_dst_word0: read0,
        dma_dst_word1: read1,
    }
}

pub fn run_thumb_dma_enable_two_steps(
    first_instr: u16,
    second_instr: u16,
    waitcnt: u16,
    control: u16,
    src: u32,
    dst: u32,
    count: u16,
) -> DmaCpuStepTrace {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x400];
    write_rom_word(
        &mut rom,
        GAMEPAK_ROM_START,
        (first_instr as u32) | ((second_instr as u32) << 16),
    );
    write_rom_word(&mut rom, DMA_ROM_SRC, 0xA0B0_C0D0);
    write_rom_word(&mut rom, DMA_ROM_SRC + 4, 0xA1B1_C1D1);
    write_rom_word(&mut rom, DMA_ROM_DST, 0xDEAD_BEEF);
    write_rom_word(&mut rom, DMA_ROM_DST + 4, 0xDEAD_BEEF);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;

    bus.internal_write32(DMA_IWRAM_SRC, 0x1122_3344);
    bus.internal_write32(DMA_IWRAM_SRC + 4, 0x5566_7788);
    bus.internal_write32(DMA_IWRAM_DST, 0xDEAD_BEEF);
    bus.internal_write32(DMA_IWRAM_DST + 4, 0xDEAD_BEEF);

    bus.dma.channels[3].src = src;
    bus.dma.channels[3].dst = dst;
    bus.dma.channels[3].count = count;
    bus.dma.channels[3].cnt = control & !0x8000;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_003F;
    cpu.registers[2] = 0x0400_00D4;
    cpu.registers[3] = ((control as u32) << 16) | count as u32;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    let (read0, read1) = if (dst >> 24) == 0x08 {
        (
            bus.internal_read32(DMA_ROM_DST),
            bus.internal_read32(DMA_ROM_DST + 4),
        )
    } else {
        (
            bus.internal_read32(DMA_IWRAM_DST),
            bus.internal_read32(DMA_IWRAM_DST + 4),
        )
    };

    DmaCpuStepTrace {
        total_cycles: bus.cycles,
        last_fetch_cycles: cpu.timing.last_opcode_fetch_cycles,
        last_fetch_used_prefetch: cpu.timing.last_opcode_used_prefetch,
        dma_dst_word0: read0,
        dma_dst_word1: read1,
    }
}

pub fn run_thumb_dma_suite_like_sequence(
    waitcnt: u16,
    control: u16,
    src: u32,
    dst: u32,
    count: u16,
) -> DmaSuiteLikeTrace {
    const STR_DMA3CNT_H: u16 = 0x6093;
    const LDRH_R2_R0: u16 = 0x8802;
    const STRH_R1_R0_2: u16 = 0x8041;
    const ADDS_R0_R2_0: u16 = 0x1C10;
    const NOP: u16 = 0x46C0;

    let mut bus = Bus::new();
    let mut rom = vec![0; 0x400];
    write_rom_word(
        &mut rom,
        GAMEPAK_ROM_START,
        (STR_DMA3CNT_H as u32) | ((LDRH_R2_R0 as u32) << 16),
    );
    write_rom_word(
        &mut rom,
        GAMEPAK_ROM_START + 4,
        (STRH_R1_R0_2 as u32) | ((ADDS_R0_R2_0 as u32) << 16),
    );
    write_rom_word(
        &mut rom,
        GAMEPAK_ROM_START + 8,
        NOP as u32 | ((NOP as u32) << 16),
    );
    write_rom_word(&mut rom, DMA_ROM_SRC, 0xA0B0_C0D0);
    write_rom_word(&mut rom, DMA_ROM_SRC + 4, 0xA1B1_C1D1);
    write_rom_word(&mut rom, DMA_ROM_DST, 0xDEAD_BEEF);
    write_rom_word(&mut rom, DMA_ROM_DST + 4, 0xDEAD_BEEF);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;

    bus.internal_write32(DMA_IWRAM_SRC, 0x1122_3344);
    bus.internal_write32(DMA_IWRAM_SRC + 4, 0x5566_7788);
    bus.internal_write32(DMA_IWRAM_DST, 0xDEAD_BEEF);
    bus.internal_write32(DMA_IWRAM_DST + 4, 0xDEAD_BEEF);

    bus.dma.channels[3].src = src;
    bus.dma.channels[3].dst = dst;
    bus.dma.channels[3].count = count;
    bus.dma.channels[3].cnt = control & !0x8000;

    // Approximate mgba-suite START: timer is enabled immediately before CODE,
    // including the hardware's 2-cycle startup delay.
    bus.internal_write32(0x0400_0100, 0x0080_0000);

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_003F;
    cpu.registers[0] = 0x0400_0100;
    cpu.registers[1] = 0;
    cpu.registers[2] = 0x0400_00D4;
    cpu.registers[3] = ((control as u32) << 16) | count as u32;
    cpu.registers[15] = GAMEPAK_ROM_START;

    for _ in 0..4 {
        cpu.step(&mut bus);
    }

    let (read0, read1) = if (dst >> 24) == 0x08 {
        (
            bus.internal_read32(DMA_ROM_DST),
            bus.internal_read32(DMA_ROM_DST + 4),
        )
    } else {
        (
            bus.internal_read32(DMA_IWRAM_DST),
            bus.internal_read32(DMA_IWRAM_DST + 4),
        )
    };

    DmaSuiteLikeTrace {
        total_cycles: bus.cycles,
        timer_value: cpu.registers[0] as u16,
        dma_dst_word0: read0,
        dma_dst_word1: read1,
    }
}

pub fn run_thumb_timer_calibration_sequence(waitcnt: u16) -> DmaSuiteLikeTrace {
    const LDRH_R2_R0: u16 = 0x8802;
    const STRH_R1_R0_2: u16 = 0x8041;
    const ADDS_R0_R2_0: u16 = 0x1C10;
    const NOP: u16 = 0x46C0;

    let mut bus = Bus::new();
    let mut rom = vec![0; 0x400];
    write_rom_word(
        &mut rom,
        GAMEPAK_ROM_START,
        (LDRH_R2_R0 as u32) | ((STRH_R1_R0_2 as u32) << 16),
    );
    write_rom_word(
        &mut rom,
        GAMEPAK_ROM_START + 4,
        (ADDS_R0_R2_0 as u32) | ((NOP as u32) << 16),
    );
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;
    bus.internal_write32(0x0400_0100, 0x0080_0000);

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_003F;
    cpu.registers[0] = 0x0400_0100;
    cpu.registers[1] = 0;
    cpu.registers[15] = GAMEPAK_ROM_START;

    for _ in 0..3 {
        cpu.step(&mut bus);
    }

    DmaSuiteLikeTrace {
        total_cycles: bus.cycles,
        timer_value: cpu.registers[0] as u16,
        dma_dst_word0: 0,
        dma_dst_word1: 0,
    }
}
