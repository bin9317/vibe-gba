use gba_core::Gba;

#[test]
fn test_save_state_round_trip_preserves_machine_state() {
    let mut gba = Gba::new();
    gba.bus.load_bios(&vec![0xAA; 16 * 1024]);
    gba.bus.load_rom(&vec![0x55; 1024]);

    gba.cpu.registers[0] = 0x1234_5678;
    gba.cpu.registers[15] = 0x0800_1234;
    gba.cpu.cpsr = 0x6000_001F;
    gba.bus.ie = 0x0003;
    gba.bus.if_ = 0x0001;
    gba.bus.ime = 1;
    gba.bus.waitcnt = 0x4317;
    gba.bus.key_state = 0xFF7F;
    gba.bus.on_board_wram[0x20] = 0x11;
    gba.bus.on_chip_wram[0x40] = 0x22;
    gba.bus.palette_ram[2] = 0x33;
    gba.bus.vram[4] = 0x44;
    gba.bus.oam[6] = 0x55;
    gba.bus.sram[8] = 0x66;
    gba.bus.eeprom.data[10] = 0x77;
    gba.bus.eeprom.command = 0x123;
    gba.bus.eeprom.bit_count = 5;
    gba.bus.ppu.registers.dispcnt = 0x1400;
    gba.bus.ppu.registers.bg0hofs = 0x0088;
    gba.bus.ppu.current_scanline = 37;
    gba.bus.ppu.frame_buffer[0] = 0x99;

    let bytes = gba.save_state().expect("save state");

    let mut restored = Gba::new();
    restored.bus.load_bios(&vec![0xAA; 16 * 1024]);
    restored.bus.load_rom(&vec![0x55; 1024]);
    restored.load_state(&bytes).expect("load state");

    assert_eq!(restored.cpu.registers[0], 0x1234_5678);
    assert_eq!(restored.cpu.registers[15], 0x0800_1234);
    assert_eq!(restored.cpu.cpsr, 0x6000_001F);
    assert_eq!(restored.bus.ie, 0x0003);
    assert_eq!(restored.bus.if_, 0x0001);
    assert_eq!(restored.bus.ime, 1);
    assert_eq!(restored.bus.waitcnt, 0x4317);
    assert_eq!(restored.bus.key_state, 0xFF7F);
    assert_eq!(restored.bus.on_board_wram[0x20], 0x11);
    assert_eq!(restored.bus.on_chip_wram[0x40], 0x22);
    assert_eq!(restored.bus.palette_ram[2], 0x33);
    assert_eq!(restored.bus.vram[4], 0x44);
    assert_eq!(restored.bus.oam[6], 0x55);
    assert_eq!(restored.bus.sram[8], 0x66);
    assert_eq!(restored.bus.eeprom.data[10], 0x77);
    assert_eq!(restored.bus.eeprom.command, 0x123);
    assert_eq!(restored.bus.eeprom.bit_count, 5);
    assert_eq!(restored.bus.ppu.registers.dispcnt, 0x1400);
    assert_eq!(restored.bus.ppu.registers.bg0hofs, 0x0088);
    assert_eq!(restored.bus.ppu.current_scanline, 37);
    assert_eq!(restored.bus.ppu.frame_buffer[0], 0x99);
}
