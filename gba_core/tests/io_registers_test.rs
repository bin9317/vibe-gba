use gba_core::bus::Bus;

#[test]
fn test_dma_enable_latches_internal_state() {
    let mut bus = Bus::new();

    bus.write8(0x040000B0, 0x00);
    bus.write8(0x040000B1, 0x00);
    bus.write8(0x040000B2, 0x00);
    bus.write8(0x040000B3, 0x02);
    bus.write8(0x040000B4, 0x00);
    bus.write8(0x040000B5, 0x00);
    bus.write8(0x040000B6, 0x00);
    bus.write8(0x040000B7, 0x03);
    bus.write8(0x040000B8, 0x01);
    bus.write8(0x040000B9, 0x00);

    // HBlank timing keeps the channel armed without running it immediately,
    // so the test can observe the internal latched state after enable.
    bus.write8(0x040000BB, 0xA0);

    let ch = &bus.dma.channels[0];
    assert_eq!(ch.src, 0x0200_0000);
    assert_eq!(ch.dst, 0x0300_0000);
    assert_eq!(ch.count, 0x0001);
    assert_eq!(ch.internal_src, ch.src);
    assert_eq!(ch.internal_dst, ch.dst);
    assert_eq!(ch.internal_count, 0x0001);
}

#[test]
fn test_timer_enable_reloads_current_value() {
    let mut bus = Bus::new();

    bus.timers.timers[0].current_value = 0xFFFF;
    bus.write8(0x04000100, 0xCD);
    bus.write8(0x04000101, 0xAB);

    bus.write8(0x04000102, 0x80);

    assert_eq!(bus.timers.timers[0].reload, 0xABCD);
    assert_eq!(bus.timers.timers[0].current_value, 0xABCD);
    assert_eq!(bus.timers.timers[0].cnt & 0x80, 0x80);
}
