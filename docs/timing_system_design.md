# GBA Timing System Design

## Purpose

This document defines a timing architecture for `vibe-gba` that can support:

- stable per-access cycle accounting
- explicit ARM7TDMI pipeline timing behavior
- Game Pak ROM sequential/non-sequential timing
- Game Pak prefetch behavior
- instruction-specific internal cycles
- test-driven debugging against `mgba-suite`

The immediate goal is not "full cycle accuracy in one rewrite". The goal is to make timing failures debuggable by separating:

- static access cost
- dynamic execution state
- derived timing outcomes

## Current Situation

The current emulator already has important timing pieces:

- `Bus::clock()` is the single hardware time source
- `Bus::get_access_time()` and `Bus::get_fetch_access_time()` model region waitstates
- CPU code explicitly inserts some internal cycles via `clock_internal()`
- Game Pak prefetch has a partial model in `Bus`
- CPU has a partial timing scaffold in `gba_core/src/cpu/timing.rs`

However, timing logic is still spread across several places:

- bus access cost
- CPU fetch scheduling
- prefetch fill/consume rules
- load/store post-access cycles
- multiply timing
- branch/pipeline refill behavior

This creates two problems:

1. A failing test does not clearly tell us whether the bug is:
   - wrong access cost
   - wrong access classification
   - wrong pipeline state transition
   - wrong prefetch behavior

2. Fixes are easy to make locally but hard to validate as a coherent timing model.

`mgba-suite` is currently exposing exactly this weakness. Many failures are not isolated value bugs. They are timing relationship bugs.

## Design Principles

### 1. Separate "cost" from "state"

Timing should not be represented as a single pile of heuristics.

We need two distinct layers:

- a static access timing layer
- a dynamic timing state machine

The static layer answers:

"If this access is an opcode fetch from WS0 ROM, 32-bit, sequential, with this WAITCNT, how many base bus cycles does it cost?"

The dynamic layer answers:

"Is this fetch sequential?"
"Can prefetch satisfy it?"
"Did the previous instruction insert internal cycles that block the next fetch?"
"Does this instruction force a pipeline refill?"

### 2. Make timing classification explicit

Every timed event should be classifiable into a small set of categories instead of being inferred ad hoc in many call sites.

Examples:

- opcode fetch
- data read
- data write
- internal cycle
- pipeline refill
- exception flush
- DMA read
- DMA write

### 3. Prefer a few explicit state transitions over many scattered special cases

Most timing bugs should be fixable by changing one of:

- access classification
- access timing table entry
- pipeline/prefetch state transition

If a fix requires touching several unrelated call sites, the model is still too implicit.

### 4. Use `mgba-suite` as a behavioral spec, not just a regression ROM

`mgba-suite` timing failures should map onto named model parts.

For example:

- `ARM/ROM ...` should primarily exercise baseline ROM fetch and execute timing
- `P..` variants should primarily exercise prefetch behavior
- `.N.` and `..S` variants should primarily exercise WAITCNT-derived access cost
- `LDR/STR/LDM/STM` timing failures should primarily narrow to execution-phase bus interactions

## Target Architecture

## Layer 1: Static Timing Tables

The bus should expose one canonical timing lookup interface for all non-DMA CPU accesses.

Example shape:

```rust
enum AccessKind {
    OpcodeFetch,
    DataRead,
    DataWrite,
}

enum AccessWidth {
    Byte,
    Halfword,
    Word,
}

struct AccessDescriptor {
    addr: u32,
    kind: AccessKind,
    width: AccessWidth,
    sequential: bool,
}

struct AccessTiming {
    cycles: u32,
    uses_gamepak_bus: bool,
}
```

The lookup should resolve:

- memory region
- width
- sequential vs non-sequential
- WAITCNT bits
- access type when relevant

This layer should be purely functional. It should not mutate pipeline or prefetch state.

### Responsibilities of Layer 1

- EWRAM/IWRAM/VRAM/OAM/Palette/ROM/SRAM base timing
- ROM WS0/WS1/WS2 N/S timing
- 16-bit vs 32-bit ROM access decomposition
- SRAM wait timing
- display-dependent VRAM/OAM penalties if needed

### Things Layer 1 should not decide

- whether the access is sequential
- whether prefetch hits
- whether a branch refills the pipeline
- whether an internal cycle delays a later fetch

Those belong to Layer 2.

## Layer 2: CPU Timing State Machine

The CPU timing model should explicitly track the minimum state needed to classify the next access correctly.

Required state:

- current execution mode: ARM or Thumb
- pipeline validity
- fetch/decode/execute PCs
- whether the next fetch is sequential or non-sequential
- whether the last data access used the Game Pak bus
- whether the pipeline was flushed by branch/exception
- whether internal cycles are currently consuming time
- current Game Pak prefetch state

This state machine should drive when accesses happen, in what order, and with what classification.

### Responsibilities of Layer 2

- schedule opcode fetches
- determine sequential vs non-sequential fetch classification
- handle pipeline refill after control flow changes
- account for internal cycles
- coordinate ROM fetch vs Game Pak data access interaction
- update prefetch stream state

### Non-goal

Layer 2 should not contain raw waitstate constants. It should ask Layer 1 for the cost of already-classified accesses.

## Layer 3: Game Pak Prefetch Model

Game Pak prefetch is important enough to remain a distinct sub-model, even if it physically lives in `Bus`.

It should answer:

- is prefetch enabled?
- what ROM address is being prefetched?
- how many halfwords are fully buffered?
- how many partial cycles have been accumulated toward the next buffered halfword?
- which events invalidate or stall the prefetch stream?

### Required behavior

- prefetch is keyed by ROM address progression, not only by a generic sequential flag
- Game Pak bus use should stall or invalidate in-progress prefetch where appropriate
- non-Game Pak internal cycles may allow prefetch to continue
- fetches may consume full or partial prefetch credit

This model should be observable in tests. If needed, add internal debugging hooks rather than burying more heuristics in CPU execution code.

## Layer 4: DMA Timing

DMA timing should not reuse CPU heuristics directly.

DMA has its own access stream and bus ownership rules. It should still reuse the same static access lookup layer, but with a separate driver.

DMA timing responsibilities:

- source and destination access classification
- transfer width
- transfer count
- start timing mode
- Game Pak bus occupancy
- interaction with CPU fetch/pre-fetch blocking

This should remain separate from CPU pipeline logic.

## Mapping Failures to the Model

Once the timing system is split this way, any failure should be triaged into one of three buckets.

### Bucket A: Wrong access cost

Symptoms:

- many failures move together when only WAITCNT bits change
- failures are stable regardless of instruction type
- `.N.` or `..S` variants are wrong in the same direction for many tests

Fix location:

- static timing table or timing lookup function

### Bucket B: Wrong access classification

Symptoms:

- correct base timing in some contexts, wrong in others
- one access is treated as sequential when it should be non-sequential
- or vice versa

Fix location:

- CPU timing state machine

### Bucket C: Wrong inter-access behavior

Symptoms:

- prefetch-enabled cases diverge from non-prefetch cases
- load/store/multiply timing is wrong while pure `nop` baseline is closer
- LDM/STM and pipeline-heavy instructions fail in groups

Fix location:

- pipeline state machine
- prefetch model
- internal cycle scheduling

## Why "Just a Timing Table" Is Not Enough

A timing table is necessary, but not sufficient.

It can encode:

- ROM sequential access costs
- EWRAM vs IWRAM wait differences
- width-specific penalties

It cannot by itself encode:

- whether the next fetch is sequential
- whether prefetch is allowed to satisfy a fetch
- whether a branch forces a refill
- whether an internal cycle overlaps with prefetch progress
- whether a ROM data access should serialize a later opcode fetch

So the intended architecture is:

1. state model classifies the access
2. timing table provides base cost
3. dynamic rules such as prefetch hits or pipeline refill modify the result

This means some failures will be fixed by changing a table entry, and others will be fixed by changing state transitions.

The design goal is not "only edit tables". The goal is "always know whether this failure belongs to the table or to the state machine".

## `mgba-suite` Timing Interpretation

`mgba-suite` timing tests are useful because they separate several concerns:

- calibration baseline
- ROM vs WRAM vs IWRAM execution
- prefetch enabled vs disabled
- WAITCNT N/S combinations
- instruction families with different internal cycle behavior

Important consequence:

If `Calibration` is wrong, the baseline fetch/measurement path is wrong.

If `nop` is close but `ldr/str/mul` are wrong, the likely issue is not the base table alone. It is the interaction between execute-stage timing and later fetch timing.

If `P..` differs sharply from `...`, prefetch behavior is implicated.

This makes `mgba-suite` a good validation target for the architecture above.

## Proposed Refactor Plan

### Phase 1: Normalize static access timing

- Introduce an explicit access descriptor and lookup result type
- Move all base CPU bus timing into one path
- Keep behavior identical where possible
- Add unit tests for ROM WS0/WS1/WS2 N/S timing lookup

Deliverable:

- one canonical timing lookup API for CPU accesses

### Phase 2: Make fetch classification explicit

- Stop deriving "sequential" from scattered local heuristics
- Add explicit CPU-side tracking for next fetch classification
- Distinguish:
  - pipeline refill fetch
  - normal sequential fetch
  - fetch after non-Game Pak data access
  - fetch after Game Pak data access

Deliverable:

- CPU can explain why a fetch was classified as sequential or not

### Phase 3: Isolate Game Pak prefetch transitions

- Centralize invalidation and fill/consume rules
- Add focused unit tests for:
  - partial prefetch credit
  - Game Pak data access blocking
  - non-Game Pak internal cycle prefetch progress

Deliverable:

- prefetch failures become debuggable without stepping through full ROMs

### Phase 4: Instruction-family timing microtests

Add small CPU-level regression tests for representative instruction families:

- `nop`
- `ldr/ldrh`
- `str/strh`
- `ldm/stm`
- `mul/mla`
- `umull/smull`
- branch/refill

These tests should measure deltas and patterns, not only absolute totals.

Deliverable:

- fewer changes need full `suite.gba` reruns to validate

### Phase 5: DMA timing model cleanup

- move DMA onto the same static timing lookup
- keep DMA scheduling separate from CPU pipeline state
- add DMA-specific timing regressions

Deliverable:

- CPU and DMA share cost tables but not control rules

## Suggested File Ownership

This is a design target, not a strict final layout.

- `gba_core/src/timing/access.rs`
  - static timing tables
  - access descriptors
  - WAITCNT decoding helpers

- `gba_core/src/timing/gamepak.rs`
  - Game Pak prefetch state
  - consume/fill/invalidate rules

- `gba_core/src/cpu/timing.rs`
  - CPU timing state machine
  - pipeline classification

- `gba_core/src/bus.rs`
  - actual clock progression
  - hardware stepping
  - memory effects

The important part is separation of responsibility, not exact file names.

## Implementation Rules

When modifying timing behavior:

1. First decide whether the issue is:
   - access cost
   - access classification
   - pipeline/prefetch interaction

2. Add or update the smallest regression test that proves the issue.

3. Change one layer only if possible.

4. Validate locally with:
   - targeted unit tests
   - targeted CPU timing microtests
   - `run_suite` only after the local signal is clear

## Current Known Risk Areas

Based on current `suite.gba` behavior, the highest-risk areas remain:

- Game Pak ROM fetch timing under different WAITCNT combinations
- prefetch-enabled ROM execution
- ARM/Thumb `LDM/STM`
- ARM/Thumb multiply timing
- timing interaction between execute-stage internal cycles and later ROM fetches

## Success Criteria

This design is succeeding when:

- most timing bugs can be localized before editing code
- timing fixes change one model layer at a time
- `mgba-suite` failures shrink in coherent groups rather than randomly
- adding a new timing regression does not require understanding unrelated timing code

## Summary

The target system is:

- table-driven for base access cost
- state-driven for access classification
- explicitly modeled for pipeline and prefetch behavior

That is the right balance for this emulator.

It avoids two bad extremes:

- "everything is a hand-coded special case"
- "everything should be a table lookup"

The correct design is a small number of explicit states feeding a small number of explicit timing tables.
