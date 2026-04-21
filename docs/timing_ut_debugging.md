# Timing UT Debugging Workflow

This document records the current workflow for debugging GBA timing issues without depending on the full `mgba-suite` ROM run every time.

## Goal

When a timing cluster fails in `mgba-suite`, do not start by guessing a hardware rule.

Use this flow instead:

1. Identify the failing cluster in `run_suite`
2. Find the corresponding `mgba-suite` source file and test case
3. Migrate the smallest representative case into Rust tests
4. Add step-level trace output for that smallest case
5. Find the first state divergence
6. Change one timing rule
7. Re-run the local Rust tests
8. Re-run `run_suite` to validate the global effect

## Where The Migrated Tests Live

The migrated `mgba-suite` timing cases are grouped by source file:

- `gba_core/tests/mgba_timing_basic_test.rs`
  Source: `third_party/mgba-suite/src/tests/basic.s`
- `gba_core/tests/mgba_timing_loadstore_test.rs`
  Source: `third_party/mgba-suite/src/tests/loadstore.s`
- `gba_core/tests/mgba_timing_block_transfer_test.rs`
  Source: `third_party/mgba-suite/src/tests/ldmia.s`
  Source: `third_party/mgba-suite/src/tests/stmia.s`
- `gba_core/tests/mgba_timing_support.rs`
  Shared helpers for the suite-like Rust tests

Each file should keep a short header comment listing:

- which `mgba-suite` source file it mirrors
- which cases have been migrated
- which cases are still missing
- which cases are known-mismatch and currently `#[ignore]`

## How To Find The Right Source Case

Use `run_suite` first:

```bash
cargo run --quiet --bin run_suite
```

The harness prints grouped lines like:

```text
Timing test: ldrh r2, [sp]
Thumb/ROM P.S: Got 2 vs 3: FAIL
```

Then locate the source definition:

```bash
rg -n "ldrh r2, \\[sp\\]" third_party/mgba-suite/src/timing.c third_party/mgba-suite/src/tests
```

That tells you:

- the expected matrix in `third_party/mgba-suite/src/timing.c`
- the assembly source in `third_party/mgba-suite/src/tests/*.s`

## What To Migrate First

Always migrate the smallest representative case first.

Good examples:

- `testNop`
- `testNop2`
- `testLdrh`
- `testLdr`
- `testStrh`
- `testLdmia1`
- `testStmia1`

Avoid starting with:

- big `x2` variants
- overflow cases
- broad matrix cases

## What A Good Local Test Should Assert

Do not only assert total cycles.

A useful timing test should assert one or more of:

- total cycle delta relative to a nearby baseline
- `cpu.timing.last_opcode_fetch_cycles`
- `cpu.timing.last_opcode_used_prefetch`
- whether the final fetch was paid, partially prefetched, or fully prefetched

Use `#[ignore]` for a migrated case that documents a known mismatch but is not fixed yet.

## How To Log Step-Level Timing

For the smallest failing case, add a temporary trace helper in a focused test file.

Current example:

- `gba_core/tests/cpu_test.rs`
  `trace_thumb_two_steps(...)`
  `debug_thumb_ldrh_sp_ps_trace`

That helper logs, per step:

- cycle count before and after the step
- PC before and after
- `last_opcode_fetch_cycles`
- `last_opcode_used_prefetch`
- `gamepak_prefetch_count`
- `gamepak_prefetch_cycles`
- `gamepak_prefetch_head_addr`
- `gamepak_prefetch_fill_addr`
- `gamepak_prefetch_block_cycles`
- `next_gamepak_fetch_is_sequential`
- `last_data_used_gamepak_bus`

Run it with:

```bash
cargo test -q -p gba_core --test cpu_test debug_thumb_ldrh_sp_ps_trace -- --nocapture
```

For a higher-level "one line per instruction" summary, enable:

```bash
VIBE_TRACE_STEP_TIMING=1 cargo test -q -p gba_core --test cpu_test debug_thumb_ldrh_sp_ps_trace -- --nocapture
```

This prints:

- instruction `pc`
- raw instruction word
- ARM/Thumb mode
- total cycles consumed by that instruction
- final opcode-fetch cost seen by the step
- whether the final fetch used prefetch
- whether control flow changed
- final prefetch buffer state

Use this summary trace first. Only drop to `VIBE_TRACE_TIMING=1` when the step-level view is not enough.

## How To Interpret The Trace

The purpose of the trace is not "see the final number again".

The purpose is to find the first divergence point.

Typical examples:

- second fetch became `0-cycle full prefetch hit`, but should have been `1-cycle partial hit`
- a data access incorrectly broke the ROM sequential stream
- prefetch progressed during a cycle where it should have been blocked
- a post-load internal cycle overlapped too much or too little with prefetch

Once the first divergence is known, change only the rule that causes that divergence.

## Recommended Command Set

Local targeted case:

```bash
cargo test -q -p gba_core --test mgba_timing_loadstore_test
```

Focused trace:

```bash
cargo test -q -p gba_core --test cpu_test debug_thumb_ldrh_sp_ps_trace -- --nocapture
```

Instruction summary trace:

```bash
VIBE_TRACE_STEP_TIMING=1 cargo test -q -p gba_core --test cpu_test debug_thumb_ldrh_sp_ps_trace -- --nocapture
```

Bus/internal timing trace:

```bash
VIBE_TRACE_TIMING=1 cargo test -q -p gba_core --test cpu_test debug_thumb_ldrh_sp_ps_trace -- --nocapture
```

Full `gba_core` test pass:

```bash
cargo test -q -p gba_core
```

Global ROM validation:

```bash
cargo run --quiet --bin run_suite
```

## Practical Rules

- Do not start with a broad "maybe prefetch/internal overlap is wrong" change.
- First prove which fetch or access is misclassified.
- Prefer relative assertions over hard-coded suite totals unless the local helper really mirrors the suite wrapper.
- Keep suite-like Rust tests grouped by `mgba-suite` source file.
- If a migrated test is still a known mismatch, keep it as `#[ignore]` rather than deleting it.
- After a fix lands, move the case from `#[ignore]` to active if possible.

## Current Known Useful Files

- `cli_debugger/src/bin/run_suite.rs`
  ROM-level failure grouping
- `gba_core/tests/mgba_timing_basic_test.rs`
  `basic.s` cases
- `gba_core/tests/mgba_timing_loadstore_test.rs`
  `loadstore.s` cases
- `gba_core/tests/mgba_timing_block_transfer_test.rs`
  `ldmia.s` and `stmia.s` cases
- `gba_core/tests/cpu_test.rs`
  temporary focused timing traces
- `gba_core/src/bus.rs`
  prefetch, access timing, Game Pak stream state
- `gba_core/src/cpu/mod.rs`
  `step()`, fetch sequencing, internal cycle handling

## Maintenance Rule

Whenever a new `mgba-suite` timing cluster is investigated:

1. migrate at least one smallest representative case into the correctly classified Rust test file
2. record whether it is active or `#[ignore]`
3. add or update a trace helper if the failure still needs step-level diagnosis

## Current TODO

1. Resolve the `loadstore.s` Thumb `P.S/PNS` cluster first.
   Current anchor: `ldrh r2, [sp]`
   Next local anchor: `ldr r2, [sp]`
   Known divergence: step 1 fetch becomes a `0-cycle` full prefetch hit when `mgba-suite` still expects a residual paid fetch.
2. After the Thumb `loadstore` cluster is fixed, move the corresponding ignored tests to active.
3. Only then move on to the `ldmia.s` / `stmia.s` prefetched block-transfer mismatches.
