mod mgba_timing_support;

use mgba_timing_support::{
    run_arm_three_steps, run_arm_two_steps, run_thumb_three_steps, run_thumb_two_steps,
};

// Source classification:
// - Backed by `third_party/mgba-suite/src/tests/basic.s`
// - Migrated cases in this file:
//   - `testNop`
//   - `testNop2`
// - Not yet migrated from `basic.s`:
//   - `calibrate`

#[test]
fn basic_arm_nop_keeps_second_fetch_at_rom_default_baseline() {
    let (_, cpu) = run_arm_two_steps(0xE1A0_0000, 0xE1A0_0000, 0x0000, 0x0200_0000, 0x1234);

    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 6);
    assert!(!cpu.timing.last_opcode_used_prefetch);
}

#[test]
fn basic_arm_nop_prefetch_keeps_second_fetch_at_rom_baseline() {
    let (_, cpu) = run_arm_two_steps(0xE1A0_0000, 0xE1A0_0000, 0x4000, 0x0200_0000, 0x1234);

    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 6);
    assert!(!cpu.timing.last_opcode_used_prefetch);
}

#[test]
fn basic_thumb_nop_keeps_second_fetch_at_rom_default_baseline() {
    let (_, cpu) = run_thumb_two_steps(0x46C0, 0x46C0, 0x0000, 0x0200_0000, 0x1234);

    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 3);
    assert!(!cpu.timing.last_opcode_used_prefetch);
}

#[test]
fn basic_thumb_nop_prefetch_keeps_second_fetch_at_rom_baseline() {
    let (_, cpu) = run_thumb_two_steps(0x46C0, 0x46C0, 0x4000, 0x0200_0000, 0x1234);

    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 3);
    assert!(!cpu.timing.last_opcode_used_prefetch);
}

#[test]
fn basic_arm_nop_nop_prefetch_keeps_third_fetch_at_rom_baseline() {
    let (_, cpu) = run_arm_three_steps(0xE1A0_0000, 0xE1A0_0000, 0xE1A0_0000, 0x4000);

    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 6);
    assert!(!cpu.timing.last_opcode_used_prefetch);
}

#[test]
fn basic_thumb_nop_nop_prefetch_keeps_third_fetch_at_rom_baseline() {
    let (_, cpu) = run_thumb_three_steps(0x46C0, 0x46C0, 0x46C0, 0x4000);

    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 3);
    assert!(!cpu.timing.last_opcode_used_prefetch);
}
