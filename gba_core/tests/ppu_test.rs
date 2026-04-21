use gba_core::bus::Bus;
use gba_core::ppu::Ppu;

#[test]
fn test_ppu_vcount_and_vblank() {
    let mut ppu = Ppu::new();
    let vram = vec![0u8; 96 * 1024];
    let palette = vec![0u8; 1024];
    let oam = vec![0u8; 1024];

    // Initially at scanline 0
    assert_eq!(ppu.registers.vcount, 0);
    assert_eq!(ppu.registers.dispstat & 1, 0);

    // Step to just before VBlank (159 scanlines * 1232 cycles + 1231 cycles)
    ppu.step(159 * 1232 + 1231, &vram, &palette, &oam);
    assert_eq!(ppu.registers.vcount, 159);
    assert_eq!(ppu.registers.dispstat & 1, 0);

    // Step 1 more cycle to hit scanline 160 (start of VBlank)
    let (vblank, _, _) = ppu.step(1, &vram, &palette, &oam);
    assert!(vblank);
    assert_eq!(ppu.registers.vcount, 160);
    assert_eq!(ppu.registers.dispstat & 1, 1);

    // Step to end of frame (227 scanlines * 1232 cycles + 1231 cycles total from start of VBlank)
    ppu.step(67 * 1232 + 1231, &vram, &palette, &oam);
    assert_eq!(ppu.registers.vcount, 227);
    assert_eq!(ppu.registers.dispstat & 1, 1);

    // Step 1 more cycle to wrap around to scanline 0
    ppu.step(1, &vram, &palette, &oam);
    assert_eq!(ppu.registers.vcount, 0);
    assert_eq!(ppu.registers.dispstat & 1, 0);
}

#[test]
fn test_ppu_mode3_render() {
    let mut bus = Bus::new();
    bus.internal_write16(0x04000000, 0x0403); // DISPCNT mode 3 + BG2 enable
    bus.internal_write16(0x06000000, 0x001F); // Red
    bus.internal_write16(0x06000002, 0x03E0); // Green

    // Step PPU to render first scanline
    bus.ppu.step(
        1232,
        bus.vram.as_ref(),
        bus.palette_ram.as_ref(),
        bus.oam.as_ref(),
    );

    let r_pixel = &bus.ppu.frame_buffer[0..4];
    assert_eq!(r_pixel, &[255, 0, 0, 255]);

    let g_pixel = &bus.ppu.frame_buffer[4..8];
    assert_eq!(g_pixel, &[0, 255, 0, 255]);
}

#[test]
fn test_ppu_mode3_respects_bg2_enable() {
    let mut bus = Bus::new();
    bus.write16(0x04000000, 0x0003); // Mode 3, BG2 disabled
    bus.write16(0x05000000, 0x7C00); // Backdrop blue
    bus.write16(0x06000000, 0x001F); // Pixel 0 would be red if BG2 rendered

    bus.ppu.step(
        1232,
        bus.vram.as_ref(),
        bus.palette_ram.as_ref(),
        bus.oam.as_ref(),
    );

    let first_pixel = &bus.ppu.frame_buffer[0..4];
    assert_eq!(first_pixel, &[0, 0, 255, 255]);
}

#[test]
fn test_ppu_mode4_palette_index_zero_is_visible() {
    let mut bus = Bus::new();
    bus.write16(0x04000000, 0x0404); // Mode 4 + BG2 enable
    bus.write16(0x05000000, 0x03E0); // Palette index 0 = green
    bus.write8(0x06000000, 0x00); // First pixel uses palette index 0

    bus.ppu.step(
        1232,
        bus.vram.as_ref(),
        bus.palette_ram.as_ref(),
        bus.oam.as_ref(),
    );

    let first_pixel = &bus.ppu.frame_buffer[0..4];
    assert_eq!(first_pixel, &[0, 255, 0, 255]);
}

#[test]
fn test_ppu_affine_reference_byte_writes_sign_extend_on_final_byte() {
    let mut bus = Bus::new();

    bus.write8(0x04000028, 0x78);
    bus.write8(0x04000029, 0x56);
    bus.write8(0x0400002A, 0x34);

    assert_eq!(bus.ppu.registers.bg2x_latch, 0x0034_5678);
    assert_eq!(bus.ppu.registers.bg2x, 0);

    bus.write8(0x0400002B, 0x08);

    assert_eq!(bus.ppu.registers.bg2x_latch, -0x07CB_A988);
    assert_eq!(bus.ppu.registers.bg2x, -0x07CB_A988);
}

#[test]
fn test_ppu_affine_reference_word_writes_keep_signed_value() {
    let mut bus = Bus::new();

    bus.write32(0x0400003C, 0x0800_0001);

    assert_eq!(bus.ppu.registers.bg3y_latch, -0x07FF_FFFF);
    assert_eq!(bus.ppu.registers.bg3y, -0x07FF_FFFF);
}

#[test]
fn test_ppu_mosaic_register_reads_and_writes() {
    let mut bus = Bus::new();

    bus.write8(0x0400004C, 0x21);
    bus.write8(0x0400004D, 0x43);

    assert_eq!(bus.ppu.registers.mosaic, 0x4321);
    assert_eq!(bus.read8(0x0400004C), 0x21);
    assert_eq!(bus.read8(0x0400004D), 0x43);
}

#[test]
fn test_ppu_word_writes_update_scroll_and_affine_pairs() {
    let mut bus = Bus::new();

    bus.write32(0x04000010, 0x5678_1234);
    assert_eq!(bus.ppu.registers.bg0hofs, 0x1234);
    assert_eq!(bus.ppu.registers.bg0vofs, 0x5678);

    bus.write32(0x04000020, 0xFEDC_1357);
    assert_eq!(bus.ppu.registers.bg2pa, 0x1357);
    assert_eq!(bus.ppu.registers.bg2pb, -0x0124);
}

#[test]
fn test_dispstat_low_byte_write_preserves_vcount_compare() {
    let mut bus = Bus::new();

    bus.internal_write8(0x04000005, 0x49);
    bus.internal_write8(0x04000004, 0x38);

    assert_eq!(bus.ppu.registers.dispstat, 0x4938);
}

#[test]
fn test_dispstat_vcount_irq_triggers_at_nonzero_compare() {
    let mut bus = Bus::new();

    bus.internal_write8(0x04000005, 73);
    bus.internal_write8(0x04000004, 0x20);

    bus.clock(73 * 1232);

    assert_ne!(bus.if_ & (1 << 2), 0);
}

#[test]
fn test_affine_bg_out_of_range_is_transparent_instead_of_reading_garbage() {
    let mut bus = Bus::new();

    bus.write16(0x04000000, 0x0401); // Mode 1 + BG2 enable
    bus.write16(0x0400000C, 0x0000); // BG2CNT: char base 0, screen base 0, size 0, no wrap
    bus.write16(0x05000000, 0x001F); // backdrop = red
    bus.write16(0x04000020, 0x0100); // BG2PA
    bus.write16(0x04000026, 0x0100); // BG2PD
    bus.write32(0x04000028, 0x0001_0000); // x_ref far outside 128x128 map
    bus.write32(0x0400002C, 0x0001_0000); // y_ref far outside 128x128 map

    bus.ppu.step(
        1232,
        bus.vram.as_ref(),
        bus.palette_ram.as_ref(),
        bus.oam.as_ref(),
    );

    let first_pixel = &bus.ppu.frame_buffer[0..4];
    assert_eq!(first_pixel, &[255, 0, 0, 255]);
}

#[test]
fn test_affine_internal_reference_advances_without_mutating_register_value() {
    let mut bus = Bus::new();

    bus.write16(0x04000000, 0x0401); // Mode 1 + BG2 enable
    bus.write16(0x04000020, 0x0100); // BG2PA
    bus.write16(0x04000022, 0x0001); // BG2PB
    bus.write16(0x04000024, 0x0000); // BG2PC
    bus.write16(0x04000026, 0x0100); // BG2PD
    bus.write32(0x04000028, 0x00001234);
    bus.write32(0x0400002C, 0x00005678);

    bus.ppu.step(
        1232,
        bus.vram.as_ref(),
        bus.palette_ram.as_ref(),
        bus.oam.as_ref(),
    );

    assert_eq!(bus.ppu.registers.bg2x, 0x0000_1234);
    assert_eq!(bus.ppu.registers.bg2y, 0x0000_5678);
}

#[test]
fn test_text_bg_256x256_scroll_wraps_within_single_screen_block() {
    let mut bus = Bus::new();

    bus.internal_write16(0x04000000, 0x0100); // Mode 0 + BG0 enable
    bus.internal_write16(0x04000008, 0x0100); // BG0CNT: char base 0, screen base 1, size 0, 4bpp
    bus.internal_write16(0x04000010, 256); // BG0HOFS wraps on a 256px-wide map

    bus.internal_write16(0x05000002, 0x001F); // Palette index 1 = red
    bus.internal_write16(0x06000000, 0x1111); // Tile 0 first four pixels use color 1
    bus.internal_write16(0x06000000 + 0x800, 0x0000); // Screen block 1, entry 0 -> tile 0

    bus.ppu.step(
        1232,
        bus.vram.as_ref(),
        bus.palette_ram.as_ref(),
        bus.oam.as_ref(),
    );

    let first_pixel = &bus.ppu.frame_buffer[0..4];
    assert_eq!(first_pixel, &[255, 0, 0, 255]);
}

#[test]
fn test_obj_wins_over_bg_at_same_priority() {
    let mut bus = Bus::new();

    bus.internal_write16(0x04000000, 0x1100); // Mode 0 + BG0 + OBJ enable
    bus.internal_write16(0x04000008, 0x0100); // BG0CNT: char base 0, screen base 1, priority 0

    bus.internal_write16(0x05000002, 0x001F); // BG palette index 1 = red
    bus.internal_write16(0x05000202, 0x03E0); // OBJ palette index 1 = green

    bus.internal_write16(0x06000000, 0x1111); // BG tile 0 starts with color 1
    bus.internal_write16(0x06000000 + 0x800, 0x0000); // Screen block 1 entry 0 -> tile 0
    bus.internal_write16(0x06010000, 0x1111); // OBJ tile 0 starts with color 1

    for i in 0..128 {
        let base = i * 8;
        bus.oam[base] = 0x00;
        bus.oam[base + 1] = 0x0C; // attr0 object mode 3 (disabled)
    }
    bus.internal_write16(0x07000000, 0x0000); // attr0: y=0, regular 8x8
    bus.internal_write16(0x07000002, 0x0000); // attr1: x=0, size 0
    bus.internal_write16(0x07000004, 0x0000); // attr2: tile 0, priority 0, palette 0

    bus.ppu.step(
        1232,
        bus.vram.as_ref(),
        bus.palette_ram.as_ref(),
        bus.oam.as_ref(),
    );

    let first_pixel = &bus.ppu.frame_buffer[0..4];
    assert_eq!(first_pixel, &[0, 255, 0, 255]);
}

fn make_window_offscreen_bus(win0v: u16, win1v: u16) -> Bus {
    let mut bus = Bus::new();

    bus.write16(0x04000000, 0x6100); // Mode 0 + BG0 + WIN0 + WIN1
    bus.write16(0x04000008, 0x4800); // BG0CNT: char base 2, screen base 1
    bus.write16(0x04000048, 0xFFFF); // WININ
    bus.write16(0x0400004A, 0x0010); // WINOUT
    bus.write16(0x04000040, 0x0078); // WIN0H
    bus.write16(0x04000044, win0v); // WIN0V
    bus.write16(0x04000042, 0x78F0); // WIN1H
    bus.write16(0x04000046, win1v); // WIN1V

    bus.write16(0x05000000, 0x7FFF); // backdrop white
    bus.write16(0x05000002, 0x3DEF); // BG palette index 1 gray

    // Fill tile 0 at char base 2 with palette index 1.
    for i in 0..16u32 {
        bus.write8(0x06008000 + i, 0x11);
    }
    // Fill screen block 1 with tile 0.
    for i in 0..1024u32 {
        bus.write16(0x06000800 + i * 2, 0);
    }

    bus
}

#[test]
fn test_window_offscreen_reset_matches_expected_framebuffer() {
    let mut actual = make_window_offscreen_bus(0x50E3, 0x50E4);
    let mut expected = make_window_offscreen_bus(0x50A0, 0x00A0);

    actual.ppu.step(
        160 * 1232,
        actual.vram.as_ref(),
        actual.palette_ram.as_ref(),
        actual.oam.as_ref(),
    );
    expected.ppu.step(
        160 * 1232,
        expected.vram.as_ref(),
        expected.palette_ram.as_ref(),
        expected.oam.as_ref(),
    );

    assert_eq!(&actual.ppu.frame_buffer[..], &expected.ppu.frame_buffer[..]);
}

#[test]
fn test_bg0cnt_changes_latch_on_next_scanline() {
    let mut bus = Bus::new();

    bus.ppu.registers.bg0cnt = 0x4684;

    bus.ppu.step(
        1,
        bus.vram.as_ref(),
        bus.palette_ram.as_ref(),
        bus.oam.as_ref(),
    );
    assert_eq!(bus.ppu.debug_render_registers().bg0cnt, 0x4684);

    bus.ppu.registers.bg0cnt = 0x4604;
    assert_eq!(bus.ppu.registers.bg0cnt, 0x4604);
    assert_eq!(bus.ppu.debug_render_registers().bg0cnt, 0x4684);

    bus.ppu.step(
        1231,
        bus.vram.as_ref(),
        bus.palette_ram.as_ref(),
        bus.oam.as_ref(),
    );
    assert_eq!(bus.ppu.current_scanline, 1);
    assert_eq!(bus.ppu.debug_render_registers().bg0cnt, 0x4604);
}
