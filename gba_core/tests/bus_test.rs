use gba_core::bus::{BackupType, Bus};
use gba_core::eeprom::EepromState;

#[test]
fn test_high_ws2_reads_and_writes_hit_eeprom() {
    let mut bus = Bus::new();
    bus.load_rom(&vec![0xAA; 4 * 1024 * 1024]);

    bus.internal_write16(0x0D00_0000, 1);

    assert_eq!(bus.eeprom.state, EepromState::Command);

    let bit = bus.internal_read16(0x0D00_0000);
    assert_eq!(bit, 1, "EEPROM default read value should be open/high");
}

#[test]
fn test_low_rom_reads_still_come_from_rom_data() {
    let mut bus = Bus::new();
    let mut rom = vec![0; 1024];
    rom[0] = 0x34;
    rom[1] = 0x12;
    bus.load_rom(&rom);

    assert_eq!(bus.internal_read16(0x0800_0000), 0x1234);
}

#[test]
fn reads_past_rom_size_use_open_bus_instead_of_mirroring() {
    let mut bus = Bus::new();
    let rom = vec![0x12, 0x34, 0x56, 0x78];
    bus.load_rom(&rom);
    bus.pc_at_access = 0x0000_0000;

    assert_eq!(bus.internal_read16(0x0800_0000), 0x3412);
    assert_eq!(bus.internal_read8(0x0800_0004), 0x02);
    assert_eq!(bus.internal_read16(0x0800_0004), 0x0002);
    assert_eq!(bus.internal_read32(0x0800_0004), 0x0003_0002);
}

#[test]
fn rom_out_of_bounds_reads_follow_gamepak_open_bus_pattern() {
    let mut bus = Bus::new();
    bus.load_rom(&vec![0; 4]);

    assert_eq!(bus.internal_read8(0x0924_68AC), 0x56);
    assert_eq!(bus.internal_read16(0x0924_68AC), 0x3456);
    assert_eq!(bus.internal_read32(0x0924_68AC), 0x3457_3456);
}

#[test]
fn unmapped_region_reads_follow_prefetched_open_bus_pattern() {
    let mut bus = Bus::new();
    bus.note_bios_prefetch(0x0800_0000, 0xE3A0_2007, false);

    assert_eq!(bus.internal_read8(0x0100_0000), 0x07);
    assert_eq!(bus.internal_read16(0x0100_0000), 0x2007);
    assert_eq!(bus.internal_read32(0x0100_0000), 0xE3A0_2007);
}

#[test]
fn bios_out_of_range_reads_follow_prefetched_open_bus_pattern() {
    let mut bus = Bus::new();
    bus.note_bios_prefetch(0x0800_0000, 0xE3A0_2007, false);

    assert_eq!(bus.internal_read8(0x0001_0000), 0x07);
    assert_eq!(bus.internal_read16(0x0001_0000), 0x2007);
    assert_eq!(bus.internal_read32(0x0001_0000), 0xE3A0_2007);
}

#[test]
fn dma_aligns_rom_source_before_transferring() {
    let mut bus = Bus::new();
    let rom = vec![0xEF, 0xBE, 0xAD, 0xDE, 0xCE, 0xFA, 0xED, 0xFE];
    bus.load_rom(&rom);

    bus.dma.channels[1].src = 0x0800_0001;
    bus.dma.channels[1].dst = 0x0200_0000;
    bus.dma.channels[1].count = 1;
    bus.dma.channels[1].cnt = 0;
    bus.write8(0x0400_00C7, 0x84);

    assert_eq!(bus.internal_read32(0x0200_0000), 0xDEAD_BEEF);
}

#[test]
fn dma0_keeps_previous_internal_source_when_gamepak_source_is_requested() {
    let mut bus = Bus::new();
    let rom = vec![0xEF, 0xBE, 0xAD, 0xDE];
    bus.load_rom(&rom);

    bus.on_board_wram[0] = 0xCE;
    bus.on_board_wram[1] = 0xFA;
    bus.on_board_wram[2] = 0xED;
    bus.on_board_wram[3] = 0xFE;

    bus.dma.channels[0].internal_src = 0x0200_0000;
    bus.dma.channels[0].internal_dst = 0x0200_0004;
    bus.dma.channels[0].src = 0x0800_0000;
    bus.dma.channels[0].dst = 0x0200_0008;
    bus.dma.channels[0].count = 1;
    bus.dma.channels[0].cnt = 0;
    bus.write8(0x0400_00BB, 0x84);

    assert_eq!(bus.internal_read32(0x0200_0008), 0xFEED_FACE);
}

#[test]
fn detects_eeprom_backup_type_from_rom_marker() {
    let mut bus = Bus::new();
    let mut rom = vec![0; 512];
    rom[0xA0..0xA0 + b"EEPROM_V124".len()].copy_from_slice(b"EEPROM_V124");

    bus.load_rom(&rom);

    assert_eq!(bus.backup_type, BackupType::Eeprom);
}

#[test]
fn eeprom_carts_do_not_expose_sram_window() {
    let mut bus = Bus::new();
    let mut rom = vec![0; 512];
    rom[0xA0..0xA0 + b"EEPROM_V124".len()].copy_from_slice(b"EEPROM_V124");
    bus.load_rom(&rom);

    bus.internal_write8(0x0E00_0000, 0x12);

    assert_eq!(bus.internal_read8(0x0E00_0000), 0xFF);
    assert!(!bus.sram_dirty);
}

#[test]
fn sram_carts_still_allow_sram_reads_and_writes() {
    let mut bus = Bus::new();
    let mut rom = vec![0; 512];
    rom[0xA0..0xA0 + b"SRAM_V113".len()].copy_from_slice(b"SRAM_V113");
    bus.load_rom(&rom);

    bus.internal_write8(0x0E00_0000, 0x34);

    assert_eq!(bus.internal_read8(0x0E00_0000), 0x34);
    assert!(bus.sram_dirty);
}

#[test]
fn sram_window_is_mirrored_into_region_0f() {
    let mut bus = Bus::new();
    let mut rom = vec![0; 512];
    rom[0xA0..0xA0 + b"SRAM_V113".len()].copy_from_slice(b"SRAM_V113");
    bus.load_rom(&rom);

    bus.internal_write8(0x0E00_0020, 0x5A);

    assert_eq!(bus.internal_read8(0x0F00_0020), 0x5A);
}

#[test]
fn absent_backup_reads_return_ff_from_region_0f() {
    let bus = Bus::new();

    assert_eq!(bus.internal_read8(0x0F00_0000), 0xFF);
}

#[test]
fn sram_halfword_and_word_reads_repeat_the_selected_byte() {
    let mut bus = Bus::new();
    let mut rom = vec![0; 512];
    rom[0xA0..0xA0 + b"SRAM_V113".len()].copy_from_slice(b"SRAM_V113");
    bus.load_rom(&rom);

    bus.internal_write8(0x0E00_0040, 0x12);

    assert_eq!(bus.internal_read16(0x0E00_0040), 0x1212);
    assert_eq!(bus.internal_read32(0x0E00_0040), 0x1212_1212);
}

#[test]
fn sram_halfword_and_word_writes_store_only_the_addressed_lane_byte() {
    let mut bus = Bus::new();
    let mut rom = vec![0; 512];
    rom[0xA0..0xA0 + b"SRAM_V113".len()].copy_from_slice(b"SRAM_V113");
    bus.load_rom(&rom);

    bus.internal_write16(0x0E00_0060, 0xAABB);
    bus.internal_write16(0x0E00_0061, 0xAABB);
    bus.internal_write32(0x0E00_0064, 0xAABB_CCDD);
    bus.internal_write32(0x0E00_0065, 0xAABB_CCDD);
    bus.internal_write32(0x0E00_0066, 0xAABB_CCDD);
    bus.internal_write32(0x0E00_0067, 0xAABB_CCDD);

    assert_eq!(bus.internal_read8(0x0E00_0060), 0xBB);
    assert_eq!(bus.internal_read8(0x0E00_0061), 0xAA);
    assert_eq!(bus.internal_read8(0x0E00_0064), 0xDD);
    assert_eq!(bus.internal_read8(0x0E00_0065), 0xCC);
    assert_eq!(bus.internal_read8(0x0E00_0066), 0xBB);
    assert_eq!(bus.internal_read8(0x0E00_0067), 0xAA);
}

#[test]
fn dma0_writes_to_sram_are_ignored() {
    let mut bus = Bus::new();
    let mut rom = vec![0; 512];
    rom[0xA0..0xA0 + b"SRAM_V113".len()].copy_from_slice(b"SRAM_V113");
    bus.load_rom(&rom);

    bus.on_board_wram[0] = 0xD8;
    bus.on_board_wram[1] = 0xC7;
    bus.on_board_wram[2] = 0xB6;
    bus.on_board_wram[3] = 0xA5;
    bus.internal_write8(0x0E00_0000, 0x66);

    bus.dma.channels[0].internal_src = 0x0200_0000;
    bus.dma.channels[0].internal_dst = 0x0E00_0000;
    bus.dma.channels[0].internal_count = 1;
    bus.dma.channels[0].cnt = 0x8400;

    bus.run_dma(0);

    assert_eq!(bus.internal_read32(0x0E00_0000), 0x6666_6666);
}

#[test]
fn gamepak_data_access_keeps_rom_stream_broken_across_non_gamepak_accesses() {
    let mut bus = Bus::new();
    let rom = vec![0x11; 0x20];
    bus.load_rom(&rom);

    assert!(bus.next_gamepak_fetch_is_sequential());

    bus.read32(0x0800_0000);
    assert!(!bus.next_gamepak_fetch_is_sequential());
    assert!(bus.last_data_used_gamepak_bus());

    bus.read32(0x0200_0000);
    assert!(!bus.next_gamepak_fetch_is_sequential());
    assert!(!bus.last_data_used_gamepak_bus());

    let _ = bus.fetch32_timed(0x0800_0000, bus.next_gamepak_fetch_is_sequential());
    assert!(bus.next_gamepak_fetch_is_sequential());
}

#[test]
fn control_flow_break_marks_next_gamepak_fetch_non_sequential() {
    let mut bus = Bus::new();
    bus.load_rom(&vec![0x11; 0x20]);

    assert!(bus.next_gamepak_fetch_is_sequential());

    bus.note_control_flow_break();
    assert!(!bus.next_gamepak_fetch_is_sequential());

    let _ = bus.fetch32_timed(0x0800_0000, bus.next_gamepak_fetch_is_sequential());
    assert!(bus.next_gamepak_fetch_is_sequential());
}

#[test]
fn dma3_writes_to_sram_use_sram_byte_lane_semantics() {
    let mut bus = Bus::new();
    let mut rom = vec![0; 512];
    rom[0xA0..0xA0 + b"SRAM_V113".len()].copy_from_slice(b"SRAM_V113");
    bus.load_rom(&rom);

    bus.on_board_wram[0] = 0xD8;
    bus.on_board_wram[1] = 0xC7;
    bus.on_board_wram[2] = 0xB6;
    bus.on_board_wram[3] = 0xA5;
    bus.internal_write8(0x0E00_0000, 0x66);

    bus.dma.channels[3].internal_src = 0x0200_0000;
    bus.dma.channels[3].internal_dst = 0x0E00_0000;
    bus.dma.channels[3].internal_count = 1;
    bus.dma.channels[3].cnt = 0x8400;

    bus.run_dma(3);

    assert_eq!(bus.internal_read32(0x0E00_0000), 0xD8D8_D8D8);
}

#[test]
fn keyinput_exposes_only_low_ten_bits() {
    let mut bus = Bus::new();

    assert_eq!(bus.internal_read16(0x0400_0130), 0x03FF);

    bus.key_state = 0x0000;
    assert_eq!(bus.internal_read16(0x0400_0130), 0x0000);
}

#[test]
fn vram_byte_writes_replicate_across_the_halfword() {
    let mut bus = Bus::new();

    bus.internal_write8(0x0600_0001, 0x3C);

    assert_eq!(bus.vram[0], 0x3C);
    assert_eq!(bus.vram[1], 0x3C);
}

#[test]
fn obj_vram_byte_writes_are_ignored() {
    let mut bus = Bus::new();
    bus.internal_write16(0x0601_7FE0, 0xBB66);
    bus.internal_write8(0x0601_7FE0, 0xD8);

    assert_eq!(bus.internal_read8(0x0601_7FE0), 0x66);
    assert_eq!(bus.internal_read16(0x0601_7FE0), 0xBB66);
}

#[test]
fn palette_byte_writes_replicate_across_the_halfword() {
    let mut bus = Bus::new();

    bus.internal_write8(0x0500_0001, 0x5A);

    assert_eq!(bus.palette_ram[0], 0x5A);
    assert_eq!(bus.palette_ram[1], 0x5A);
}

#[test]
fn ready_gamepak_prefetch_can_satisfy_sequential_thumb_fetch_without_cycles() {
    let mut bus = Bus::new();
    bus.load_rom(&vec![0x34, 0x12, 0x78, 0x56]);
    bus.waitcnt |= 1 << 14;
    bus.gamepak_prefetch_count = 1;
    bus.gamepak_prefetch_head_addr = 0x0800_0000;
    bus.gamepak_prefetch_fill_addr = 0x0800_0002;

    let fetch = bus
        .try_fetch_prefetched16(0x0800_0000, true)
        .expect("prefetched halfword should be consumed");

    assert_eq!(fetch.value, 0x1234);
    assert_eq!(fetch.cycles, 0);
    assert!(fetch.used_prefetch);
    assert_eq!(bus.gamepak_prefetch_count, 0);
    assert_eq!(bus.gamepak_prefetch_head_addr, 0x0800_0002);
}

#[test]
fn paid_thumb_fetch_primes_prefetch_for_the_following_halfword() {
    let mut bus = Bus::new();
    bus.load_rom(&vec![0x34, 0x12, 0x78, 0x56, 0xBC, 0x9A]);
    bus.waitcnt |= 1 << 14;
    bus.pc_at_access = 0x0800_0000;
    bus.set_gamepak_prefetch_stream(0x0800_0000);

    let fetch = bus.fetch16_timed(0x0800_0000, true);

    assert!(fetch.cycles > 0);
    assert_eq!(bus.gamepak_prefetch_head_addr, 0x0800_0002);
    assert_eq!(bus.gamepak_prefetch_fill_addr, 0x0800_0002);
    assert_eq!(bus.gamepak_prefetch_count, 0);
    assert_eq!(bus.gamepak_prefetch_cycles, 0);
}

#[test]
fn ready_gamepak_prefetch_needs_two_halfwords_for_arm_fetch() {
    let mut bus = Bus::new();
    bus.load_rom(&vec![0x34, 0x12, 0x78, 0x56]);
    bus.waitcnt |= 1 << 14;
    bus.gamepak_prefetch_count = 1;
    bus.gamepak_prefetch_head_addr = 0x0800_0000;
    bus.gamepak_prefetch_fill_addr = 0x0800_0002;

    assert!(bus.try_fetch_prefetched32(0x0800_0000, true).is_none());

    bus.gamepak_prefetch_count = 2;
    let fetch = bus
        .try_fetch_prefetched32(0x0800_0000, true)
        .expect("two prefetched halfwords should satisfy an ARM fetch");

    assert_eq!(fetch.value, 0x5678_1234);
    assert_eq!(fetch.cycles, 0);
    assert!(fetch.used_prefetch);
    assert_eq!(bus.gamepak_prefetch_count, 0);
    assert_eq!(bus.gamepak_prefetch_head_addr, 0x0800_0004);
}

#[test]
fn arm_fetch_keeps_partial_credit_for_second_prefetch_halfword() {
    let mut bus = Bus::new();
    bus.load_rom(&vec![0x34, 0x12, 0x78, 0x56]);
    bus.waitcnt |= 1 << 14;
    bus.gamepak_prefetch_count = 1;
    bus.gamepak_prefetch_head_addr = 0x0800_0000;
    bus.gamepak_prefetch_fill_addr = 0x0800_0002;
    bus.gamepak_prefetch_cycles = 1;

    let fetch = bus.fetch32_timed(0x0800_0000, true);

    assert_eq!(fetch.value, 0x5678_1234);
    assert_eq!(fetch.cycles, 2);
}

#[test]
fn special_sound_dma_feeds_fifo_a_on_timer0_overflow() {
    let mut bus = Bus::new();
    for (index, byte) in bus.on_board_wram.iter_mut().take(32).enumerate() {
        *byte = index as u8;
    }

    bus.internal_write16(0x0400_0082, 0x0B00); // Direct Sound A -> L/R, reset FIFO A
    bus.internal_write16(0x0400_0084, 0x0080); // Master sound enable

    bus.internal_write32(0x0400_00BC, 0x0200_0000);
    bus.internal_write32(0x0400_00C0, 0x0400_00A0);
    bus.internal_write16(0x0400_00C4, 4);
    bus.internal_write16(0x0400_00C6, 0xB600); // special, repeat, 32-bit, enabled

    bus.internal_write16(0x0400_0100, 0xFFFC);
    bus.internal_write16(0x0400_0102, 0x0083); // enable + /1024 prescaler

    bus.clock(4098);

    assert_eq!(bus.dma.channels[1].internal_src, 0x0200_0010);
    assert_eq!(bus.dma.channels[1].internal_dst, 0x0400_00A0);
    assert_eq!(bus.dma.channels[1].internal_count, 4);
    assert_eq!(bus.pending_dma, 0);
}
