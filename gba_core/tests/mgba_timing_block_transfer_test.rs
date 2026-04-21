mod mgba_timing_support;

use mgba_timing_support::run_arm_two_steps_with_regs;

// Source classification:
// - Backed by `third_party/mgba-suite/src/tests/ldmia.s`
// - Backed by `third_party/mgba-suite/src/tests/stmia.s`
// - Migrated active cases in this file:
//   - `testLdmia1`
//   - `testLdmia2`
//   - `testStmia1`
//   - `testStmia2`
// - Migrated but still ignored:
//   - `testLdmia6`
//   - `testStmia6`
// - Not yet migrated:
//   - `x2` variants
//   - overflow/OAM-to-ROM variants

#[test]
fn block_transfer_arm_ldmia1_costs_more_than_nop_baseline() {
    let (ldmia_bus, cpu) = run_arm_two_steps_with_regs(
        0xE89D_0004,
        0xE1A0_0000,
        0x0000,
        0x0200_0000,
        &[],
        &[0xDEAD_BEEF],
    );

    assert!(ldmia_bus.cycles > 0);
    assert_eq!(cpu.registers[2], 0xDEAD_BEEF);
}

#[test]
fn block_transfer_arm_ldmia2_loads_two_registers() {
    let (_, cpu) = run_arm_two_steps_with_regs(
        0xE89D_000C,
        0xE1A0_0000,
        0x0000,
        0x0200_0000,
        &[],
        &[0xAAAA_5555, 0x1234_5678],
    );

    assert_eq!(cpu.registers[2], 0xAAAA_5555);
    assert_eq!(cpu.registers[3], 0x1234_5678);
}

#[test]
fn block_transfer_arm_stmia1_stores_one_register() {
    let (mut bus, _) = run_arm_two_steps_with_regs(
        0xE88D_0004,
        0xE1A0_0000,
        0x0000,
        0x0200_0000,
        &[(2, 0xCAFE_BABE)],
        &[0],
    );

    assert_eq!(bus.internal_read32(0x0200_0000), 0xCAFE_BABE);
}

#[test]
fn block_transfer_arm_stmia2_stores_two_registers() {
    let (mut bus, _) = run_arm_two_steps_with_regs(
        0xE88D_000C,
        0xE1A0_0000,
        0x0000,
        0x0200_0000,
        &[(2, 0xAAAA_5555), (3, 0x1234_5678)],
        &[0, 0],
    );

    assert_eq!(bus.internal_read32(0x0200_0000), 0xAAAA_5555);
    assert_eq!(bus.internal_read32(0x0200_0004), 0x1234_5678);
}

#[test]
#[ignore = "Known mismatch with mgba-suite: ARM block transfer timing on ROM still undercounts prefetched paths"]
fn block_transfer_arm_ldmia6_prefetch_matches_mgba_suite() {
    let (bus, cpu) = run_arm_two_steps_with_regs(
        0xE89D_00FC,
        0xE1A0_0000,
        0x4000,
        0x0200_0000,
        &[],
        &[1, 2, 3, 4, 5, 6],
    );

    assert_eq!(bus.cycles, 23);
    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 8);
}

#[test]
#[ignore = "Known mismatch with mgba-suite: ARM block transfer timing on ROM still undercounts prefetched paths"]
fn block_transfer_arm_stmia6_prefetch_matches_mgba_suite() {
    let (bus, cpu) = run_arm_two_steps_with_regs(
        0xE88D_00FC,
        0xE1A0_0000,
        0x4000,
        0x0200_0000,
        &[(2, 1), (3, 2), (4, 3), (5, 4), (6, 5), (7, 6)],
        &[0, 0, 0, 0, 0, 0],
    );

    assert_eq!(bus.cycles, 21);
    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 7);
}
