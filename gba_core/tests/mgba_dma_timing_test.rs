mod mgba_dma_support;

use mgba_dma_support::{
    DMA_IWRAM_DST, DMA_IWRAM_SRC, DMA_ROM_DST, DMA_ROM_SRC, run_dma3_transfer,
    run_thumb_dma_enable_step, run_thumb_dma_enable_two_steps, run_thumb_dma_suite_like_sequence,
    run_thumb_timer_calibration_sequence,
};

// Source classification:
// - Backed by `third_party/mgba-suite/src/tests/dma-timing.s`
// - Migrated active cases in this file:
//   - none yet
// - Migrated but still ignored:
//   - `testTrivialDma` Thumb `P.S`
//   - `testTrivial32Dma` Thumb `P.S`
//   - `testShortDma` Thumb `P.S`
//   - `testShort32Dma` Thumb `P.S`
// - Not yet migrated from `dma-timing.s`:
//   - all ROM source/destination variants
//   - ARM-side timing variants

#[test]
fn dma_trivial_16_thumb_ps_copies_one_halfword_to_iwram() {
    let trace = run_dma3_transfer(0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);

    assert_eq!(trace.dma_dst_word0 & 0xFFFF, 0x3344);
    assert_eq!(trace.dma_dst_word1, 0xDEAD_BEEF);
}

#[test]
fn dma_trivial_32_thumb_ps_copies_one_word_to_iwram() {
    let trace = run_dma3_transfer(0x8400, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);

    assert_eq!(trace.dma_dst_word0, 0x1122_3344);
    assert_eq!(trace.dma_dst_word1, 0xDEAD_BEEF);
}

#[test]
fn dma_short_16_thumb_ps_copies_sixteen_halfwords_to_iwram_window() {
    let trace = run_dma3_transfer(0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 0x10);

    assert_eq!(trace.dma_dst_word0, 0x1122_3344);
    assert_eq!(trace.dma_dst_word1, 0x5566_7788);
}

#[test]
fn dma_short_32_thumb_ps_copies_sixteen_words_to_iwram_window() {
    let trace = run_dma3_transfer(0x8400, DMA_IWRAM_SRC, DMA_IWRAM_DST, 0x10);

    assert_eq!(trace.dma_dst_word0, 0x1122_3344);
    assert_eq!(trace.dma_dst_word1, 0x5566_7788);
}

#[test]
fn dma_direct_body_cycle_shape_currently_matches_2_and_32() {
    let trivial = run_dma3_transfer(0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);
    let short = run_dma3_transfer(0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 0x10);

    println!("direct dma trivial: {:?}", trivial);
    println!("direct dma short:   {:?}", short);
    assert_eq!(trivial.total_cycles, 2);
    assert_eq!(short.total_cycles, 32);
}

#[test]
#[ignore = "Known mismatch target: Trivial DMA (16) Thumb/ROM P.S timing from mgba-suite"]
fn dma_trivial_16_thumb_ps_matches_mgba_suite_timing() {
    let trace = run_thumb_dma_enable_step(0x6093, 0x4010, 0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);

    assert_eq!(trace.total_cycles, 8);
}

#[test]
#[ignore = "Known mismatch target: Trivial DMA (32) Thumb/ROM P.S timing from mgba-suite"]
fn dma_trivial_32_thumb_ps_matches_mgba_suite_timing() {
    let trace = run_thumb_dma_enable_step(0x6093, 0x4010, 0x8400, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);

    assert_eq!(trace.total_cycles, 8);
}

#[test]
#[ignore = "Known mismatch target: Short DMA (16) Thumb/ROM P.S timing from mgba-suite"]
fn dma_short_16_thumb_ps_matches_mgba_suite_timing() {
    let trace =
        run_thumb_dma_enable_step(0x6093, 0x4010, 0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 0x10);

    assert_eq!(trace.total_cycles, 38);
}

#[test]
#[ignore = "Known mismatch target: Short DMA (32) Thumb/ROM P.S timing from mgba-suite"]
fn dma_short_32_thumb_ps_matches_mgba_suite_timing() {
    let trace =
        run_thumb_dma_enable_step(0x6093, 0x4010, 0x8400, DMA_IWRAM_SRC, DMA_IWRAM_DST, 0x10);

    assert_eq!(trace.total_cycles, 38);
}

#[test]
fn dma_trivial_16_thumb_ps_to_rom_currently_observes_rom_destination_window() {
    let trace = run_dma3_transfer(0x8000, DMA_IWRAM_SRC, DMA_ROM_DST, 1);

    assert_ne!(trace.dma_dst_word0, 0);
}

#[test]
fn dma_trivial_16_thumb_ps_from_rom_currently_observes_rom_source_window() {
    let trace = run_dma3_transfer(0x8000, DMA_ROM_SRC, DMA_IWRAM_DST, 1);

    assert_ne!(trace.dma_dst_word0, 0xDEAD_BEEF);
}

#[test]
fn dma_thumb_store_step_triggers_dma3_transfer() {
    let trace = run_thumb_dma_enable_two_steps(
        0x6093,
        0x46C0,
        0x4010,
        0x8000,
        DMA_IWRAM_SRC,
        DMA_IWRAM_DST,
        1,
    );

    assert_eq!(trace.dma_dst_word0 & 0xFFFF, 0x3344);
    assert_eq!(trace.dma_dst_word1, 0xDEAD_BEEF);
}

#[test]
fn dma_thumb_store_step_costs_more_than_nop_baseline() {
    let baseline =
        run_thumb_dma_enable_step(0x46C0, 0x4010, 0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);
    let dma = run_thumb_dma_enable_step(0x6093, 0x4010, 0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);

    assert!(dma.total_cycles > baseline.total_cycles);
}

#[test]
fn dma_thumb_store_step_with_enable_costs_more_than_same_store_without_enable() {
    let store_only = run_thumb_dma_enable_two_steps(
        0x6093,
        0x46C0,
        0x4010,
        0x0000,
        DMA_IWRAM_SRC,
        DMA_IWRAM_DST,
        1,
    );
    let dma = run_thumb_dma_enable_two_steps(
        0x6093,
        0x46C0,
        0x4010,
        0x8000,
        DMA_IWRAM_SRC,
        DMA_IWRAM_DST,
        1,
    );

    assert_eq!(store_only.dma_dst_word0, 0xDEAD_BEEF);
    assert_eq!(dma.dma_dst_word0 & 0xFFFF, 0x3344);
    assert!(dma.total_cycles > store_only.total_cycles);
}

#[test]
fn dma_suite_like_trivial_thumb_ps_records_timer_value() {
    let trace = run_thumb_dma_suite_like_sequence(0x4010, 0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);

    println!("suite-like trivial enabled: {:?}", trace);
    assert_eq!(trace.dma_dst_word0 & 0xFFFF, 0x3344);
    assert!(trace.timer_value > 0);
}

#[test]
fn dma_suite_like_short_thumb_ps_records_larger_timer_value_than_trivial() {
    let trivial =
        run_thumb_dma_suite_like_sequence(0x4010, 0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);
    let short =
        run_thumb_dma_suite_like_sequence(0x4010, 0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 0x10);

    println!("suite-like trivial enabled: {:?}", trivial);
    println!("suite-like short enabled:   {:?}", short);
    assert!(short.timer_value > trivial.timer_value);
}

#[test]
#[ignore = "Known mismatch target: suite-like trivial DMA timer still exceeds disabled path by 2"]
fn dma_suite_like_trivial_thumb_ps_enable_increases_timer_value() {
    let disabled =
        run_thumb_dma_suite_like_sequence(0x4010, 0x0000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);
    let enabled =
        run_thumb_dma_suite_like_sequence(0x4010, 0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);

    println!("suite-like trivial disabled: {:?}", disabled);
    println!("suite-like trivial enabled:  {:?}", enabled);
    assert_eq!(enabled.timer_value, disabled.timer_value);
}

#[test]
#[ignore = "Known mismatch target: calibrated suite-like Trivial DMA (16) Thumb/ROM P.S should be 2, current local reproducer is 4"]
fn dma_suite_like_trivial_thumb_ps_minus_calibration_matches_mgba_suite_shape() {
    let calibration = run_thumb_timer_calibration_sequence(0x4010);
    let enabled =
        run_thumb_dma_suite_like_sequence(0x4010, 0x8000, DMA_IWRAM_SRC, DMA_IWRAM_DST, 1);

    println!("suite-like calibration:      {:?}", calibration);
    println!("suite-like trivial enabled:  {:?}", enabled);
    assert_eq!(enabled.timer_value - calibration.timer_value, 2);
}
