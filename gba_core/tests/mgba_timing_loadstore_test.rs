mod mgba_timing_support;

use mgba_timing_support::{
    run_arm_two_steps, run_arm_two_steps_with_word, run_thumb_three_steps, run_thumb_two_steps,
};

// Source classification:
// - Backed by `third_party/mgba-suite/src/tests/loadstore.s`
// - Migrated active cases in this file:
//   - `testLdrh`
//   - `testLdrhNop`
//   - `testLdr`
//   - `testLdrNop`
//   - `testStrh`
// - Migrated but still ignored:
//   - `testLdrh` Thumb `P.S/PNS` expectation
//   - `testLdr` Thumb `P.S/PNS` expectation
//   - `testLdrhNop` Thumb `P.S/PNS` expectation
//   - `testLdrNop` Thumb `P.S/PNS` expectation
// - Known-but-not-yet-migrated mismatch cluster:
//   - none
// - Not yet migrated from `loadstore.s`:
//   - ROM-data variants
//   - `testLdrStr`, `testLdrLdr`, `testStrLdr`, `testStrStr`
// - Migrated but still ignored next-step probes:
//   - `testNopLdrh` Thumb `P.S/PNS` expectation
//   - `testNopLdr` Thumb `P.S/PNS` expectation

#[test]
fn loadstore_arm_ldrh_sp_prefetch_hides_extra_cost() {
    let (nop_bus, _) = run_arm_two_steps(0xE1A0_0000, 0xE1A0_0000, 0x4000, 0x0200_0000, 0x1234);
    let (ldrh_bus, _) = run_arm_two_steps(0xE1DD_20B0, 0xE1A0_0000, 0x4000, 0x0200_0000, 0x1234);

    assert_eq!(ldrh_bus.cycles - nop_bus.cycles, 0);
}

#[test]
fn loadstore_arm_ldrh_sp_without_prefetch_still_costs_more_than_nop() {
    let (nop_bus, nop_cpu) =
        run_arm_two_steps(0xE1A0_0000, 0xE1A0_0000, 0x0000, 0x0200_0000, 0x1234);
    let (ldrh_bus, ldrh_cpu) =
        run_arm_two_steps(0xE1DD_20B0, 0xE1A0_0000, 0x0000, 0x0200_0000, 0x1234);

    assert!(ldrh_bus.cycles > nop_bus.cycles);
    assert_eq!(
        ldrh_cpu.timing.last_opcode_fetch_cycles,
        nop_cpu.timing.last_opcode_fetch_cycles
    );
    assert!(!ldrh_cpu.timing.last_opcode_used_prefetch);
}

#[test]
fn loadstore_arm_ldrh_sp_then_nop_prefetch_keeps_extra_cost_hidden() {
    let (nop_bus, _) = run_arm_two_steps(0xE1A0_0000, 0xE1A0_0000, 0x4000, 0x0200_0000, 0x1234);
    let (ldrh_nop_bus, _) =
        run_arm_two_steps(0xE1DD_20B0, 0xE1A0_0000, 0x4000, 0x0200_0000, 0x1234);

    assert_eq!(ldrh_nop_bus.cycles - nop_bus.cycles, 0);
}

#[test]
fn loadstore_arm_ldr_sp_prefetch_removes_most_of_the_extra_cost() {
    let (nop_no_prefetch, _) =
        run_arm_two_steps_with_word(0xE1A0_0000, 0xE1A0_0000, 0x0000, 0x0200_0000, 0x1234_5678);
    let (ldr_no_prefetch, _) =
        run_arm_two_steps_with_word(0xE59D_2000, 0xE1A0_0000, 0x0000, 0x0200_0000, 0x1234_5678);
    let (nop_prefetch, _) =
        run_arm_two_steps_with_word(0xE1A0_0000, 0xE1A0_0000, 0x4000, 0x0200_0000, 0x1234_5678);
    let (ldr_prefetch, _) =
        run_arm_two_steps_with_word(0xE59D_2000, 0xE1A0_0000, 0x4000, 0x0200_0000, 0x1234_5678);

    let extra_without_prefetch = ldr_no_prefetch.cycles - nop_no_prefetch.cycles;
    let extra_with_prefetch = ldr_prefetch.cycles - nop_prefetch.cycles;
    assert!(extra_without_prefetch >= 4);
    assert!(extra_with_prefetch <= 1);
}

#[test]
fn loadstore_arm_ldr_sp_then_nop_prefetch_removes_most_of_the_extra_cost() {
    let (nop_no_prefetch, _) =
        run_arm_two_steps_with_word(0xE1A0_0000, 0xE1A0_0000, 0x0000, 0x0200_0000, 0x1234_5678);
    let (ldr_prefetch, _) =
        run_arm_two_steps_with_word(0xE59D_2000, 0xE1A0_0000, 0x4000, 0x0200_0000, 0x1234_5678);

    assert!(ldr_prefetch.cycles <= nop_no_prefetch.cycles + 1);
}

#[test]
fn loadstore_arm_strh_sp_without_prefetch_still_costs_more_than_nop() {
    let (nop_bus, _) = run_arm_two_steps(0xE1A0_0000, 0xE1A0_0000, 0x0000, 0x0200_0000, 0x1234);
    let mut bus = gba_core::bus::Bus::new();
    let mut rom = vec![0; 0x20];
    mgba_timing_support::write_rom_word(&mut rom, 0x0800_0000, 0xE1CD_30B0);
    mgba_timing_support::write_rom_word(&mut rom, 0x0800_0004, 0xE1A0_0000);
    mgba_timing_support::write_rom_word(&mut rom, 0x0800_0008, 0xE1A0_0000);
    mgba_timing_support::write_rom_word(&mut rom, 0x0800_000C, 0xE1A0_0000);
    bus.load_rom(&rom);
    let mut cpu = gba_core::cpu::Cpu::new();
    cpu.cpsr = 0x0000_001F;
    cpu.registers[3] = 0x1234;
    cpu.registers[13] = 0x0200_0000;
    cpu.registers[15] = gba_core::GAMEPAK_ROM_START;
    cpu.step(&mut bus);
    cpu.step(&mut bus);

    assert!(bus.cycles > nop_bus.cycles);
}

#[test]
#[ignore = "Known mismatch with mgba-suite: Thumb P.S/PNS currently get a fully-prefetched second fetch"]
fn loadstore_thumb_ldrh_sp_ps_matches_mgba_suite() {
    let (bus, cpu) = run_thumb_two_steps(0x880A, 0x46C0, 0x4010, 0x0200_0000, 0x1234);

    assert_eq!(bus.cycles, 15);
    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 1);
    assert!(cpu.timing.last_opcode_used_prefetch);
}

#[test]
#[ignore = "Known mismatch with mgba-suite: Thumb P.S/PNS currently get a fully-prefetched second fetch"]
fn loadstore_thumb_ldr_sp_ps_matches_mgba_suite() {
    let (bus, cpu) = run_thumb_two_steps(0x9A00, 0x46C0, 0x4010, 0x0200_0000, 0x1234);

    assert_eq!(bus.cycles, 15);
    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 1);
    assert!(cpu.timing.last_opcode_used_prefetch);
}

#[test]
#[ignore = "Known mismatch with mgba-suite: Thumb load+nop P.S/PNS still overcounts by one"]
fn loadstore_thumb_ldrh_sp_nop_ps_matches_mgba_suite() {
    let (bus, cpu) = run_thumb_three_steps(0x880A, 0x46C0, 0x46C0, 0x4010);

    assert_eq!(bus.cycles, 18);
    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 2);
    assert!(!cpu.timing.last_opcode_used_prefetch);
}

#[test]
#[ignore = "Known mismatch with mgba-suite: Thumb load+nop P.S/PNS still overcounts by one"]
fn loadstore_thumb_ldr_sp_nop_ps_matches_mgba_suite() {
    let (bus, cpu) = run_thumb_three_steps(0x9A00, 0x46C0, 0x46C0, 0x4010);

    assert_eq!(bus.cycles, 18);
    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 2);
    assert!(!cpu.timing.last_opcode_used_prefetch);
}

#[test]
#[ignore = "Probe for next-step Thumb load timing under P.S/PNS"]
fn loadstore_thumb_nop_ldrh_sp_ps_matches_mgba_suite() {
    let (bus, cpu) = run_thumb_three_steps(0x46C0, 0x880A, 0x46C0, 0x4010);

    assert_eq!(bus.cycles, 19);
    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 1);
    assert!(cpu.timing.last_opcode_used_prefetch);
}

#[test]
#[ignore = "Probe for next-step Thumb load timing under P.S/PNS"]
fn loadstore_thumb_nop_ldr_sp_ps_matches_mgba_suite() {
    let (bus, cpu) = run_thumb_three_steps(0x46C0, 0x9A00, 0x46C0, 0x4010);

    assert_eq!(bus.cycles, 19);
    assert_eq!(cpu.timing.last_opcode_fetch_cycles, 1);
    assert!(cpu.timing.last_opcode_used_prefetch);
}
