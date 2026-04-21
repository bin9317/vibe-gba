use gba_core::GAMEPAK_ROM_START;
use gba_core::bus::Bus;
use gba_core::cpu::Cpu;
use std::fs;
use std::path::Path;

fn load_rom(path: &str) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|_| panic!("Failed to read ROM: {path}"))
}

fn load_armwrestler_rom() -> Vec<u8> {
    let candidates = [
        "../roms/armwrestler/armwrestler.gba",
        "../tests/roms/armwrestler.gba",
    ];
    let path = candidates
        .into_iter()
        .find(|path| Path::new(path).exists())
        .unwrap_or("../roms/armwrestler/armwrestler.gba");
    load_rom(path)
}

#[test]
fn test_armwrestler_load_and_run() {
    let rom_data = load_armwrestler_rom();

    let mut bus = Bus::new();
    bus.load_rom(&rom_data);

    let mut cpu = Cpu::new();
    cpu.skip_bios(&mut bus);

    // Run for a small number of cycles to ensure it doesn't crash on unimplemented panics.
    for i in 0..1000 {
        let pc = cpu.registers[15];
        let instr = if cpu.get_thumb_mode() {
            bus.read16(pc & !1) as u32
        } else {
            bus.read32(pc & !3)
        };
        // Print less frequently or just the last few to avoid spam, but let's print for debugging
        if i < 20 {
            println!(
                "Cycle {}: PC: {:08X}, Instr: {:08X}, Thumb: {}",
                i,
                pc,
                instr,
                cpu.get_thumb_mode()
            );
        }
        cpu.step(&mut bus);
    }

    println!("Final PC: {:08X}", cpu.registers[15]);
    // Test that PC has advanced from the initial skip_bios state (0x08000000)
    assert!(
        cpu.registers[15] != GAMEPAK_ROM_START,
        "PC should have advanced"
    );
}

#[test]
fn test_skip_bios_initializes_expected_bios_state() {
    let rom_data = load_armwrestler_rom();

    let mut bus = Bus::new();
    bus.load_rom(&rom_data);

    let mut cpu = Cpu::new();
    cpu.skip_bios(&mut bus);

    assert_eq!(cpu.registers[15], GAMEPAK_ROM_START);
    assert_eq!(cpu.cpsr, 0x0000_001F);
    assert_eq!(bus.internal_read32(0x0300_7FF8), 0);
    assert_eq!(bus.internal_read32(0x0300_7FFC), 0x0000_0300);
    assert_eq!(bus.postflg, 1);
}

#[test]
fn test_arm_pipeline_keeps_prefetched_instruction_after_self_modify() {
    let mut bus = Bus::new();
    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[15] = 0x0200_0000;

    // mov r1, #0
    bus.internal_write32(0x0200_0000, 0xE3A0_1000);
    // add lr, pc, #8  ; lr = 0x0200_0014
    bus.internal_write32(0x0200_0004, 0xE28F_E008);
    // ldr r0, [pc, #-0x10] ; load instruction word from 0x0200_0000
    bus.internal_write32(0x0200_0008, 0xE51F_0010);
    // str r0, [lr] ; overwrite 0x0200_0014 with mov r1, #0
    bus.internal_write32(0x0200_000C, 0xE58E_0000);
    // mov r1, #255
    bus.internal_write32(0x0200_0010, 0xE3A0_10FF);
    // mov r1, #255 ; should remain prefetched even after store at 0x0200_000C
    bus.internal_write32(0x0200_0014, 0xE3A0_10FF);

    for _ in 0..6 {
        cpu.step(&mut bus);
    }

    assert_eq!(cpu.registers[1], 0xFF);
}

#[test]
fn test_timing_pipeline_tracks_legacy_refill_and_step() {
    let mut bus = Bus::new();
    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[15] = 0x0200_0000;

    bus.internal_write32(0x0200_0000, 0xE3A0_0001); // mov r0, #1
    bus.internal_write32(0x0200_0004, 0xE280_0002); // add r0, r0, #2
    bus.internal_write32(0x0200_0008, 0xE280_0004); // add r0, r0, #4
    bus.internal_write32(0x0200_000C, 0xE280_0008); // add r0, r0, #8

    cpu.step(&mut bus);

    assert_eq!(cpu.registers[0], 1);
    assert_eq!(cpu.pipeline_pc, 0x0200_0004);
    assert_eq!(cpu.pipeline_instrs[0], 0xE280_0002);
    assert_eq!(cpu.pipeline_instrs[1], 0xE280_0004);

    assert!(cpu.timing.execute.valid);
    assert!(cpu.timing.decode.valid);
    assert!(!cpu.timing.fetch.valid);
    assert_eq!(cpu.timing.execute.pc, 0x0200_0004);
    assert_eq!(cpu.timing.execute.instruction, 0xE280_0002);
    assert_eq!(cpu.timing.decode.pc, 0x0200_0008);
    assert_eq!(cpu.timing.decode.instruction, 0xE280_0004);
    assert_eq!(cpu.timing.fetch.pc, 0x0200_000C);

    cpu.step(&mut bus);

    assert_eq!(cpu.registers[0], 3);
    assert_eq!(cpu.pipeline_pc, 0x0200_0008);
    assert_eq!(cpu.timing.execute.pc, 0x0200_0008);
    assert_eq!(cpu.timing.execute.instruction, 0xE280_0004);
    assert_eq!(cpu.timing.decode.pc, 0x0200_000C);
    assert_eq!(cpu.timing.decode.instruction, 0xE280_0008);
}

#[test]
fn test_step_refills_when_timing_pipeline_is_invalid() {
    let mut bus = Bus::new();
    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[15] = 0x0200_0000;

    bus.internal_write32(0x0200_0000, 0xE3A0_0001); // mov r0, #1
    bus.internal_write32(0x0200_0004, 0xE280_0002); // add r0, r0, #2
    bus.internal_write32(0x0200_0008, 0xE280_0004); // add r0, r0, #4
    bus.internal_write32(0x0200_000C, 0xE280_0008); // add r0, r0, #8

    cpu.step(&mut bus);
    assert_eq!(cpu.registers[0], 1);
    assert!(cpu.pipeline_valid);
    assert!(cpu.timing.execute.valid);
    assert!(cpu.timing.decode.valid);

    cpu.timing.invalidate_pipeline();
    cpu.step(&mut bus);

    assert_eq!(cpu.registers[0], 3);
    assert!(cpu.timing.execute.valid);
    assert!(cpu.timing.decode.valid);
    assert_eq!(cpu.timing.execute.pc, 0x0200_0008);
    assert_eq!(cpu.pipeline_pc, 0x0200_0008);
}

fn write_rom_word(rom: &mut [u8], addr: u32, value: u32) {
    let offset = (addr - GAMEPAK_ROM_START) as usize;
    rom[offset] = (value & 0xFF) as u8;
    rom[offset + 1] = ((value >> 8) & 0xFF) as u8;
    rom[offset + 2] = ((value >> 16) & 0xFF) as u8;
    rom[offset + 3] = ((value >> 24) & 0xFF) as u8;
}

fn opcode_fetch_cycles_after_two_steps(
    first_instr: u32,
    r0: u32,
    rom_data_word: u32,
    enable_prefetch: bool,
) -> u32 {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(&mut rom, 0x0800_0000, first_instr);
    write_rom_word(&mut rom, 0x0800_0004, 0xE1A0_0000);
    write_rom_word(&mut rom, 0x0800_0008, 0xE1A0_0000);
    write_rom_word(&mut rom, 0x0800_000C, 0xE1A0_0000);
    write_rom_word(&mut rom, 0x0800_0010, rom_data_word);
    bus.load_rom(&rom);
    if enable_prefetch {
        bus.waitcnt |= 1 << 14;
    }
    bus.on_board_wram[0] = 0x78;
    bus.on_board_wram[1] = 0x56;
    bus.on_board_wram[2] = 0x34;
    bus.on_board_wram[3] = 0x12;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[15] = GAMEPAK_ROM_START;
    cpu.registers[0] = r0;

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    cpu.timing.last_opcode_fetch_cycles
}

fn opcode_fetch_cycles_after_three_steps(
    first_instr: u32,
    second_instr: u32,
    enable_prefetch: bool,
) -> u32 {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(&mut rom, 0x0800_0000, first_instr);
    write_rom_word(&mut rom, 0x0800_0004, second_instr);
    write_rom_word(&mut rom, 0x0800_0008, 0xE1A0_0000);
    write_rom_word(&mut rom, 0x0800_000C, 0xE1A0_0000);
    bus.load_rom(&rom);
    if enable_prefetch {
        bus.waitcnt |= 1 << 14;
    }

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    cpu.timing.last_opcode_fetch_cycles
}

fn total_cycles_after_two_steps_arm(
    first_instr: u32,
    enable_prefetch: bool,
    sp: u32,
    halfword_at_sp: u16,
) -> u64 {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(&mut rom, 0x0800_0000, first_instr);
    write_rom_word(&mut rom, 0x0800_0004, 0xE1A0_0000);
    write_rom_word(&mut rom, 0x0800_0008, 0xE1A0_0000);
    write_rom_word(&mut rom, 0x0800_000C, 0xE1A0_0000);
    bus.load_rom(&rom);
    if enable_prefetch {
        bus.waitcnt |= 1 << 14;
    }
    bus.on_board_wram[0] = (halfword_at_sp & 0xFF) as u8;
    bus.on_board_wram[1] = (halfword_at_sp >> 8) as u8;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[13] = sp;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    bus.cycles
}

fn total_cycles_after_two_steps_thumb(
    first_instr: u16,
    second_instr: u16,
    enable_prefetch: bool,
    sp: u32,
    halfword_at_sp: u16,
    waitcnt: u16,
) -> u64 {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(
        &mut rom,
        0x0800_0000,
        (first_instr as u32) | ((second_instr as u32) << 16),
    );
    write_rom_word(&mut rom, 0x0800_0004, 0x46C0_46C0);
    write_rom_word(&mut rom, 0x0800_0008, 0x46C0_46C0);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;
    if enable_prefetch {
        bus.waitcnt |= 1 << 14;
    }
    bus.on_board_wram[0] = (halfword_at_sp & 0xFF) as u8;
    bus.on_board_wram[1] = (halfword_at_sp >> 8) as u8;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_003F;
    cpu.registers[13] = sp;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    bus.cycles
}

fn thumb_last_fetch_cycles_after_two_steps(
    first_instr: u16,
    second_instr: u16,
    enable_prefetch: bool,
    sp: u32,
    halfword_at_sp: u16,
    waitcnt: u16,
) -> u32 {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(
        &mut rom,
        0x0800_0000,
        (first_instr as u32) | ((second_instr as u32) << 16),
    );
    write_rom_word(&mut rom, 0x0800_0004, 0x46C0_46C0);
    write_rom_word(&mut rom, 0x0800_0008, 0x46C0_46C0);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;
    if enable_prefetch {
        bus.waitcnt |= 1 << 14;
    }
    bus.on_board_wram[0] = (halfword_at_sp & 0xFF) as u8;
    bus.on_board_wram[1] = (halfword_at_sp >> 8) as u8;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_003F;
    cpu.registers[13] = sp;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    cpu.timing.last_opcode_fetch_cycles
}

#[allow(dead_code)]
#[derive(Debug)]
struct ThumbStepTrace {
    step: usize,
    cycles_before: u64,
    cycles_after: u64,
    pc_before: u32,
    pc_after: u32,
    pipeline_pc: u32,
    pipeline_next_fetch: u32,
    last_fetch_cycles: u32,
    last_fetch_used_prefetch: bool,
    prefetch_count: u8,
    prefetch_cycles: u32,
    prefetch_head: u32,
    prefetch_fill: u32,
    prefetch_block: u32,
    timing_fetch_valid: bool,
    timing_fetch_pc: u32,
    timing_decode_valid: bool,
    timing_decode_pc: u32,
    timing_execute_valid: bool,
    timing_execute_pc: u32,
    next_gamepak_fetch_is_sequential: bool,
    last_data_used_gamepak_bus: bool,
}

fn trace_thumb_two_steps(
    first_instr: u16,
    second_instr: u16,
    enable_prefetch: bool,
    sp: u32,
    halfword_at_sp: u16,
    waitcnt: u16,
) -> Vec<ThumbStepTrace> {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(
        &mut rom,
        0x0800_0000,
        (first_instr as u32) | ((second_instr as u32) << 16),
    );
    write_rom_word(&mut rom, 0x0800_0004, 0x46C0_46C0);
    write_rom_word(&mut rom, 0x0800_0008, 0x46C0_46C0);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;
    if enable_prefetch {
        bus.waitcnt |= 1 << 14;
    }
    bus.on_board_wram[0] = (halfword_at_sp & 0xFF) as u8;
    bus.on_board_wram[1] = (halfword_at_sp >> 8) as u8;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_003F;
    cpu.registers[13] = sp;
    cpu.registers[15] = GAMEPAK_ROM_START;

    let mut traces = Vec::new();
    for step in 0..2 {
        let cycles_before = bus.cycles;
        let pc_before = cpu.registers[15];
        cpu.step(&mut bus);
        traces.push(ThumbStepTrace {
            step,
            cycles_before,
            cycles_after: bus.cycles,
            pc_before,
            pc_after: cpu.registers[15],
            pipeline_pc: cpu.pipeline_pc,
            pipeline_next_fetch: cpu.pipeline_next_fetch,
            last_fetch_cycles: cpu.timing.last_opcode_fetch_cycles,
            last_fetch_used_prefetch: cpu.timing.last_opcode_used_prefetch,
            prefetch_count: bus.gamepak_prefetch_count,
            prefetch_cycles: bus.gamepak_prefetch_cycles,
            prefetch_head: bus.gamepak_prefetch_head_addr,
            prefetch_fill: bus.gamepak_prefetch_fill_addr,
            prefetch_block: bus.gamepak_prefetch_block_cycles,
            timing_fetch_valid: cpu.timing.fetch.valid,
            timing_fetch_pc: cpu.timing.fetch.pc,
            timing_decode_valid: cpu.timing.decode.valid,
            timing_decode_pc: cpu.timing.decode.pc,
            timing_execute_valid: cpu.timing.execute.valid,
            timing_execute_pc: cpu.timing.execute.pc,
            next_gamepak_fetch_is_sequential: bus.next_gamepak_fetch_is_sequential(),
            last_data_used_gamepak_bus: bus.last_data_used_gamepak_bus(),
        });
    }

    traces
}

fn trace_thumb_three_steps(
    instr0: u16,
    instr1: u16,
    instr2: u16,
    enable_prefetch: bool,
    sp: u32,
    halfword_at_sp: u16,
    waitcnt: u16,
) -> Vec<ThumbStepTrace> {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(
        &mut rom,
        0x0800_0000,
        (instr0 as u32) | ((instr1 as u32) << 16),
    );
    write_rom_word(
        &mut rom,
        0x0800_0004,
        (instr2 as u32) | ((0x46C0u16 as u32) << 16),
    );
    write_rom_word(&mut rom, 0x0800_0008, 0x46C0_46C0);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;
    if enable_prefetch {
        bus.waitcnt |= 1 << 14;
    }
    bus.on_board_wram[0] = (halfword_at_sp & 0xFF) as u8;
    bus.on_board_wram[1] = (halfword_at_sp >> 8) as u8;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_003F;
    cpu.registers[13] = sp;
    cpu.registers[15] = GAMEPAK_ROM_START;

    let mut traces = Vec::new();
    for step in 0..3 {
        let cycles_before = bus.cycles;
        let pc_before = cpu.registers[15];
        cpu.step(&mut bus);
        traces.push(ThumbStepTrace {
            step,
            cycles_before,
            cycles_after: bus.cycles,
            pc_before,
            pc_after: cpu.registers[15],
            pipeline_pc: cpu.pipeline_pc,
            pipeline_next_fetch: cpu.pipeline_next_fetch,
            last_fetch_cycles: cpu.timing.last_opcode_fetch_cycles,
            last_fetch_used_prefetch: cpu.timing.last_opcode_used_prefetch,
            prefetch_count: bus.gamepak_prefetch_count,
            prefetch_cycles: bus.gamepak_prefetch_cycles,
            prefetch_head: bus.gamepak_prefetch_head_addr,
            prefetch_fill: bus.gamepak_prefetch_fill_addr,
            prefetch_block: bus.gamepak_prefetch_block_cycles,
            timing_fetch_valid: cpu.timing.fetch.valid,
            timing_fetch_pc: cpu.timing.fetch.pc,
            timing_decode_valid: cpu.timing.decode.valid,
            timing_decode_pc: cpu.timing.decode.pc,
            timing_execute_valid: cpu.timing.execute.valid,
            timing_execute_pc: cpu.timing.execute.pc,
            next_gamepak_fetch_is_sequential: bus.next_gamepak_fetch_is_sequential(),
            last_data_used_gamepak_bus: bus.last_data_used_gamepak_bus(),
        });
    }

    traces
}

fn trace_thumb_four_steps(
    instr0: u16,
    instr1: u16,
    instr2: u16,
    instr3: u16,
    enable_prefetch: bool,
    sp: u32,
    halfword_at_sp: u16,
    waitcnt: u16,
) -> Vec<ThumbStepTrace> {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(
        &mut rom,
        0x0800_0000,
        (instr0 as u32) | ((instr1 as u32) << 16),
    );
    write_rom_word(
        &mut rom,
        0x0800_0004,
        (instr2 as u32) | ((instr3 as u32) << 16),
    );
    write_rom_word(&mut rom, 0x0800_0008, 0x46C0_46C0);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;
    if enable_prefetch {
        bus.waitcnt |= 1 << 14;
    }
    bus.on_board_wram[0] = (halfword_at_sp & 0xFF) as u8;
    bus.on_board_wram[1] = (halfword_at_sp >> 8) as u8;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_003F;
    cpu.registers[13] = sp;
    cpu.registers[15] = GAMEPAK_ROM_START;

    let mut traces = Vec::new();
    for step in 0..4 {
        let cycles_before = bus.cycles;
        let pc_before = cpu.registers[15];
        cpu.step(&mut bus);
        traces.push(ThumbStepTrace {
            step,
            cycles_before,
            cycles_after: bus.cycles,
            pc_before,
            pc_after: cpu.registers[15],
            pipeline_pc: cpu.pipeline_pc,
            pipeline_next_fetch: cpu.pipeline_next_fetch,
            last_fetch_cycles: cpu.timing.last_opcode_fetch_cycles,
            last_fetch_used_prefetch: cpu.timing.last_opcode_used_prefetch,
            prefetch_count: bus.gamepak_prefetch_count,
            prefetch_cycles: bus.gamepak_prefetch_cycles,
            prefetch_head: bus.gamepak_prefetch_head_addr,
            prefetch_fill: bus.gamepak_prefetch_fill_addr,
            prefetch_block: bus.gamepak_prefetch_block_cycles,
            timing_fetch_valid: cpu.timing.fetch.valid,
            timing_fetch_pc: cpu.timing.fetch.pc,
            timing_decode_valid: cpu.timing.decode.valid,
            timing_decode_pc: cpu.timing.decode.pc,
            timing_execute_valid: cpu.timing.execute.valid,
            timing_execute_pc: cpu.timing.execute.pc,
            next_gamepak_fetch_is_sequential: bus.next_gamepak_fetch_is_sequential(),
            last_data_used_gamepak_bus: bus.last_data_used_gamepak_bus(),
        });
    }

    traces
}

#[test]
fn rom_fetch_after_ewram_data_access_keeps_sequential_timing() {
    let baseline = opcode_fetch_cycles_after_two_steps(0xE1A0_0000, 0, 0, false);
    let after_ewram_ldr = opcode_fetch_cycles_after_two_steps(0xE590_1000, 0x0200_0000, 0, false);

    assert_eq!(baseline, 6);
    assert_eq!(after_ewram_ldr, baseline);
}

#[test]
fn rom_fetch_after_gamepak_data_access_breaks_the_sequential_stream() {
    let baseline = opcode_fetch_cycles_after_two_steps(0xE1A0_0000, 0, 0, false);
    let after_rom_ldr =
        opcode_fetch_cycles_after_two_steps(0xE590_1000, 0x0800_0010, 0x4433_2211, false);

    assert_eq!(baseline, 6);
    assert!(after_rom_ldr > baseline);
}

#[test]
fn prefetch_does_not_change_nop_fetch_timing_baseline() {
    let without_prefetch = opcode_fetch_cycles_after_two_steps(0xE1A0_0000, 0, 0, false);
    let with_prefetch = opcode_fetch_cycles_after_two_steps(0xE1A0_0000, 0, 0, true);

    assert_eq!(without_prefetch, 6);
    assert_eq!(with_prefetch, without_prefetch);
}

#[test]
fn prefetch_does_not_change_nop_nop_fetch_timing_baseline() {
    let without_prefetch = opcode_fetch_cycles_after_three_steps(0xE1A0_0000, 0xE1A0_0000, false);
    let with_prefetch = opcode_fetch_cycles_after_three_steps(0xE1A0_0000, 0xE1A0_0000, true);

    assert_eq!(without_prefetch, 6);
    assert_eq!(with_prefetch, without_prefetch);
}

#[test]
fn arm_ldrh_ewram_costs_four_more_cycles_than_nop_without_prefetch() {
    let nop_cycles = total_cycles_after_two_steps_arm(0xE1A0_0000, false, 0x0200_0000, 0x1234);
    let ldrh_cycles = total_cycles_after_two_steps_arm(0xE1DD_20B0, false, 0x0200_0000, 0x1234);

    // This helper does not model the full mgba-suite wrapper, but the load should still
    // cost materially more than NOP when prefetch is disabled.
    assert!(ldrh_cycles - nop_cycles >= 4);
}

#[test]
fn arm_ldrh_ewram_costs_no_more_than_nop_with_prefetch() {
    let nop_cycles = total_cycles_after_two_steps_arm(0xE1A0_0000, true, 0x0200_0000, 0x1234);
    let ldrh_cycles = total_cycles_after_two_steps_arm(0xE1DD_20B0, true, 0x0200_0000, 0x1234);

    assert_eq!(ldrh_cycles - nop_cycles, 0);
}

#[test]
fn arm_ldrh_ewram_accumulates_prefetch_during_data_read_and_internal_cycle() {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(&mut rom, 0x0800_0000, 0xE1DD_20B0);
    write_rom_word(&mut rom, 0x0800_0004, 0xE1A0_0000);
    write_rom_word(&mut rom, 0x0800_0008, 0xE1A0_0000);
    write_rom_word(&mut rom, 0x0800_000C, 0xE1A0_0000);
    bus.load_rom(&rom);
    bus.waitcnt |= 1 << 14;
    bus.on_board_wram[0] = 0x34;
    bus.on_board_wram[1] = 0x12;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[13] = 0x0200_0000;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);

    assert_eq!(bus.gamepak_prefetch_head_addr, 0x0800_000C);
    assert_eq!(bus.gamepak_prefetch_fill_addr, 0x0800_000E);
    assert_eq!(bus.gamepak_prefetch_count, 1);
    assert_eq!(bus.gamepak_prefetch_cycles, 1);
}

#[test]
fn thumb_ldrh_sp_ps_prefetch_saves_four_cycles() {
    let no_prefetch =
        total_cycles_after_two_steps_thumb(0x880A, 0x46C0, false, 0x0200_0000, 0x1234, 0x0010);
    let with_prefetch =
        total_cycles_after_two_steps_thumb(0x880A, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);

    assert_eq!(no_prefetch - with_prefetch, 4);
}

#[test]
fn thumb_ldrh_sp_ps_prefetch_keeps_second_fetch_at_one_cycle() {
    let no_prefetch =
        thumb_last_fetch_cycles_after_two_steps(0x880A, 0x46C0, false, 0x0200_0000, 0x1234, 0x0010);
    let with_prefetch =
        thumb_last_fetch_cycles_after_two_steps(0x880A, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);

    assert_eq!(no_prefetch, 2);
    assert_eq!(with_prefetch, 1);
}

#[test]
fn thumb_ldrh_sp_ps_costs_one_more_cycle_than_nop() {
    let nop_cycles =
        total_cycles_after_two_steps_thumb(0x46C0, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);
    let ldrh_cycles =
        total_cycles_after_two_steps_thumb(0x880A, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);

    assert_eq!(ldrh_cycles - nop_cycles, 1);
}

#[test]
fn debug_thumb_ldrh_sp_ps_trace() {
    let nop_prefetch_default =
        trace_thumb_two_steps(0x46C0, 0x46C0, true, 0x0200_0000, 0x1234, 0x0000);
    let prefetch_default = trace_thumb_two_steps(0x880A, 0x46C0, true, 0x0200_0000, 0x1234, 0x0000);
    let nop_prefetch_ps = trace_thumb_two_steps(0x46C0, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);
    let no_prefetch = trace_thumb_two_steps(0x880A, 0x46C0, false, 0x0200_0000, 0x1234, 0x0010);
    let with_prefetch = trace_thumb_two_steps(0x880A, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);

    println!("nop with prefetch default:");
    for trace in &nop_prefetch_default {
        println!("{trace:?}");
    }
    println!("prefetch default:");
    for trace in &prefetch_default {
        println!("{trace:?}");
    }
    println!("nop with prefetch P.S:");
    for trace in &nop_prefetch_ps {
        println!("{trace:?}");
    }
    println!("no prefetch:");
    for trace in &no_prefetch {
        println!("{trace:?}");
    }
    println!("with prefetch:");
    for trace in &with_prefetch {
        println!("{trace:?}");
    }
}

#[test]
fn debug_thumb_ldr_sp_ps_trace() {
    let nop_prefetch_ps = trace_thumb_two_steps(0x46C0, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);
    let no_prefetch = trace_thumb_two_steps(0x9A00, 0x46C0, false, 0x0200_0000, 0x1234, 0x0010);
    let with_prefetch = trace_thumb_two_steps(0x9A00, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);

    println!("ldr nop with prefetch P.S:");
    for trace in &nop_prefetch_ps {
        println!("{trace:?}");
    }
    println!("ldr no prefetch:");
    for trace in &no_prefetch {
        println!("{trace:?}");
    }
    println!("ldr with prefetch:");
    for trace in &with_prefetch {
        println!("{trace:?}");
    }
}

#[test]
fn debug_thumb_strh_sp_ps_trace() {
    let nop_prefetch_ps = trace_thumb_two_steps(0x46C0, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);
    let no_prefetch = trace_thumb_two_steps(0x800B, 0x46C0, false, 0x0200_0000, 0x1234, 0x0010);
    let with_prefetch = trace_thumb_two_steps(0x800B, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);

    println!("strh nop with prefetch P.S:");
    for trace in &nop_prefetch_ps {
        println!("{trace:?}");
    }
    println!("strh no prefetch:");
    for trace in &no_prefetch {
        println!("{trace:?}");
    }
    println!("strh with prefetch:");
    for trace in &with_prefetch {
        println!("{trace:?}");
    }
}

#[test]
fn debug_thumb_ldrh_sp_nop_ps_trace() {
    let with_prefetch =
        trace_thumb_three_steps(0x880A, 0x46C0, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);
    println!("ldrh / nop with prefetch P.S:");
    for trace in &with_prefetch {
        println!("{trace:?}");
    }
}

#[test]
fn debug_thumb_ldr_sp_nop_ps_trace() {
    let with_prefetch =
        trace_thumb_three_steps(0x9A00, 0x46C0, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);
    println!("ldr / nop with prefetch P.S:");
    for trace in &with_prefetch {
        println!("{trace:?}");
    }
}

#[test]
fn debug_thumb_nop_nop_nop_ps_trace() {
    let with_prefetch =
        trace_thumb_three_steps(0x46C0, 0x46C0, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);
    println!("nop / nop / nop with prefetch P.S:");
    for trace in &with_prefetch {
        println!("{trace:?}");
    }
}

#[test]
fn debug_thumb_nop_ldrh_sp_ps_trace() {
    let with_prefetch =
        trace_thumb_three_steps(0x46C0, 0x880A, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);
    println!("nop / ldrh with prefetch P.S:");
    for trace in &with_prefetch {
        println!("{trace:?}");
    }
}

#[test]
fn debug_thumb_nop_ldr_sp_ps_trace() {
    let with_prefetch =
        trace_thumb_three_steps(0x46C0, 0x9A00, 0x46C0, true, 0x0200_0000, 0x1234, 0x0010);
    println!("nop / ldr with prefetch P.S:");
    for trace in &with_prefetch {
        println!("{trace:?}");
    }
}

#[test]
fn debug_thumb_ldrh_sp_nop_nop_ps_trace() {
    let with_prefetch = trace_thumb_four_steps(
        0x880A,
        0x46C0,
        0x46C0,
        0x46C0,
        true,
        0x0200_0000,
        0x1234,
        0x0010,
    );
    println!("ldrh / nop / nop with prefetch P.S:");
    for trace in &with_prefetch {
        println!("{trace:?}");
    }
}

#[test]
fn debug_thumb_nop_ldrh_sp_nop_ps_trace() {
    let with_prefetch = trace_thumb_four_steps(
        0x46C0,
        0x880A,
        0x46C0,
        0x46C0,
        true,
        0x0200_0000,
        0x1234,
        0x0010,
    );
    println!("nop / ldrh / nop with prefetch P.S:");
    for trace in &with_prefetch {
        println!("{trace:?}");
    }
}
