use gba_core::Gba;

const ARM_NOP: u32 = 0xE1A0_0000;

fn write_arm_word(gba: &mut Gba, addr: u32, value: u32) {
    gba.bus.load_bios(&[]);
    gba.bus.bios[addr as usize] = (value & 0xFF) as u8;
    gba.bus.bios[addr as usize + 1] = ((value >> 8) & 0xFF) as u8;
    gba.bus.bios[addr as usize + 2] = ((value >> 16) & 0xFF) as u8;
    gba.bus.bios[addr as usize + 3] = ((value >> 24) & 0xFF) as u8;
}

fn setup_irq_test_gba(cpsr: u32, pc: u32) -> Gba {
    let mut gba = Gba::new();
    gba.cpu.cpsr = cpsr;
    gba.cpu.registers[15] = pc;
    gba.bus.ie = 1;
    gba.bus.if_ = 1;
    gba.bus.ime = 1;
    write_arm_word(&mut gba, 0x18, ARM_NOP);
    gba
}

#[test]
fn test_irq_entry_uses_pipeline_adjusted_lr_in_arm_state() {
    let mut gba = setup_irq_test_gba(0x0000_001F, 0x0800_0100);

    gba.step();

    assert_eq!(gba.cpu.spsr_irq, 0x0000_001F);
    assert_eq!(gba.cpu.registers[14], 0x0800_0104);
    assert_eq!(gba.cpu.registers[15], 0x0000_001C);
    assert_eq!(gba.cpu.cpsr & 0xBF, 0x0000_0092);
}

#[test]
fn test_irq_entry_uses_pipeline_adjusted_lr_in_thumb_state() {
    let mut gba = setup_irq_test_gba(0x0000_003F, 0x0800_0100);

    gba.step();

    assert_eq!(gba.cpu.spsr_irq, 0x0000_003F);
    assert_eq!(gba.cpu.registers[14], 0x0800_0104);
    assert_eq!(gba.cpu.registers[15], 0x0000_001C);
    assert_eq!(gba.cpu.cpsr & 0xBF, 0x0000_0092);
}
