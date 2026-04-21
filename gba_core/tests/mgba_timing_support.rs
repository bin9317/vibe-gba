#![allow(dead_code)]

use gba_core::GAMEPAK_ROM_START;
use gba_core::bus::Bus;
use gba_core::cpu::Cpu;

pub fn write_rom_word(rom: &mut [u8], addr: u32, value: u32) {
    let offset = (addr - GAMEPAK_ROM_START) as usize;
    rom[offset] = (value & 0xFF) as u8;
    rom[offset + 1] = ((value >> 8) & 0xFF) as u8;
    rom[offset + 2] = ((value >> 16) & 0xFF) as u8;
    rom[offset + 3] = ((value >> 24) & 0xFF) as u8;
}

pub fn run_arm_two_steps(
    instr0: u32,
    instr1: u32,
    waitcnt: u16,
    sp: u32,
    halfword_at_sp: u16,
) -> (Bus, Cpu) {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(&mut rom, 0x0800_0000, instr0);
    write_rom_word(&mut rom, 0x0800_0004, instr1);
    write_rom_word(&mut rom, 0x0800_0008, 0xE1A0_0000);
    write_rom_word(&mut rom, 0x0800_000C, 0xE1A0_0000);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;
    bus.on_board_wram[0] = (halfword_at_sp & 0xFF) as u8;
    bus.on_board_wram[1] = (halfword_at_sp >> 8) as u8;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[13] = sp;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    (bus, cpu)
}

pub fn run_arm_three_steps(instr0: u32, instr1: u32, instr2: u32, waitcnt: u16) -> (Bus, Cpu) {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(&mut rom, 0x0800_0000, instr0);
    write_rom_word(&mut rom, 0x0800_0004, instr1);
    write_rom_word(&mut rom, 0x0800_0008, instr2);
    write_rom_word(&mut rom, 0x0800_000C, 0xE1A0_0000);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    (bus, cpu)
}

pub fn run_thumb_two_steps(
    instr0: u16,
    instr1: u16,
    waitcnt: u16,
    sp: u32,
    halfword_at_sp: u16,
) -> (Bus, Cpu) {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(
        &mut rom,
        0x0800_0000,
        (instr0 as u32) | ((instr1 as u32) << 16),
    );
    write_rom_word(&mut rom, 0x0800_0004, 0x46C0_46C0);
    write_rom_word(&mut rom, 0x0800_0008, 0x46C0_46C0);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;
    bus.on_board_wram[0] = (halfword_at_sp & 0xFF) as u8;
    bus.on_board_wram[1] = (halfword_at_sp >> 8) as u8;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_003F;
    cpu.registers[13] = sp;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    (bus, cpu)
}

pub fn run_thumb_three_steps(instr0: u16, instr1: u16, instr2: u16, waitcnt: u16) -> (Bus, Cpu) {
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

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_003F;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    (bus, cpu)
}

pub fn run_arm_two_steps_with_word(
    instr0: u32,
    instr1: u32,
    waitcnt: u16,
    sp: u32,
    word_at_sp: u32,
) -> (Bus, Cpu) {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(&mut rom, 0x0800_0000, instr0);
    write_rom_word(&mut rom, 0x0800_0004, instr1);
    write_rom_word(&mut rom, 0x0800_0008, 0xE1A0_0000);
    write_rom_word(&mut rom, 0x0800_000C, 0xE1A0_0000);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;
    bus.on_board_wram[0] = (word_at_sp & 0xFF) as u8;
    bus.on_board_wram[1] = ((word_at_sp >> 8) & 0xFF) as u8;
    bus.on_board_wram[2] = ((word_at_sp >> 16) & 0xFF) as u8;
    bus.on_board_wram[3] = ((word_at_sp >> 24) & 0xFF) as u8;

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[13] = sp;
    cpu.registers[15] = GAMEPAK_ROM_START;

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    (bus, cpu)
}

pub fn run_arm_two_steps_with_regs(
    instr0: u32,
    instr1: u32,
    waitcnt: u16,
    sp: u32,
    regs: &[(usize, u32)],
    stack_words: &[u32],
) -> (Bus, Cpu) {
    let mut bus = Bus::new();
    let mut rom = vec![0; 0x20];
    write_rom_word(&mut rom, 0x0800_0000, instr0);
    write_rom_word(&mut rom, 0x0800_0004, instr1);
    write_rom_word(&mut rom, 0x0800_0008, 0xE1A0_0000);
    write_rom_word(&mut rom, 0x0800_000C, 0xE1A0_0000);
    bus.load_rom(&rom);
    bus.waitcnt = waitcnt;
    for (i, word) in stack_words.iter().enumerate() {
        let addr = 0x0200_0000 + (i as u32) * 4;
        bus.internal_write32(addr, *word);
    }

    let mut cpu = Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[13] = sp;
    cpu.registers[15] = GAMEPAK_ROM_START;
    for (reg, value) in regs {
        cpu.registers[*reg] = *value;
    }

    cpu.step(&mut bus);
    cpu.step(&mut bus);

    (bus, cpu)
}
