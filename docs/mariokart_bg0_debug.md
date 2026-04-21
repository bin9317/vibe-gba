# Mario Kart Cup Preview BG0 Debug Notes

This note records the current state of the Mario Kart cup preview corruption investigation.

## Problem Summary

- ROM: `roms/mario kart/rom.gba`
- State: `roms/mario kart/rom.gba.state`
- From this state, pressing `A` enters `CHOOSE A CUP`
- Reference screenshot from mGBA: `/tmp/bg.png`

The visible bug is on the right-side cup preview panel. The preview image is garbled in this emulator but correct in mGBA.

## High-Level Conclusion

The issue is not primarily caused by bad ROM data, bad state data, broken DMA upload, palette corruption, or blend/window composition.

The strongest current conclusion is:

- the final correct result behaves as if `BG0` uses `0x4684` for the upper portion of the panel
- then switches to `0x4604` around the middle of the screen

But the emulator currently traces:

- `line 0 = 0x4684`
- `line 1..159 = 0x4604`

That mismatch is the core problem still to explain.

## Confirmed Facts

### Data Path

The preview image data path is valid:

- `SWI 0x11` decompresses into `0x02004400`
- `DMA3` copies to `0x06004000`

The decompressed payload size is `5184` bytes, which is exactly `81 * 64`, i.e. 81 `8bpp` tiles.

This strongly supports the preview asset itself being `8bpp`.

### BG0 4bpp vs 8bpp Evidence

Offline rendering of dumped `BG0` data showed:

- full `4bpp`: matches the current broken emulator output
- full `8bpp`: fixes the preview image, but breaks the lower course-name area

This led to split-line experiments.

Relevant images:

- split at 40: `/tmp/mk_split40_test/frame_000180.png`
- split at 80: `/tmp/mk_split80_test/frame_000180.png`
- split at 100: `/tmp/mk_split100_test/frame_000180.png`
- mGBA reference: `/tmp/bg.png`

Observed quality:

- split 40: too early
- split 80: closest to mGBA
- split 100: too late

This is a strong phenomenological result, but not a hardware rule yet.

### Actual BG0CNT Trace

Using `trace_scanlines_after_frames`, the emulator currently renders the target frame with:

- `y=0`: `bg0cnt=4684`
- `y=1..159`: `bg0cnt=4604`

So the current emulator really is switching almost immediately.

### BG0CNT Writers

Known `BG0CNT` write sites:

- `0x08001EB8` writes `4684`
- `0x0800C69E / 0x0800C6A4 / 0x0800C6BC` read-modify-write `BG0CNT` down to `4604`

The `0x0800C680..0x0800C6BC` code does not read `VCOUNT`.

It clears bit 7 of `BG0CNT` with mask `0xFF7F`, and also touches `MOSAIC` high-byte via `0x0400004C`.

### Layer Findings

The right panel is not on `BG1` or `BG2`.

Exact masking checks showed:

- `BG1`: background mushrooms only
- `BG2`: circular panel backing only
- removing `BG0` makes the course list disappear

So the course list is primarily on `BG0`.

There are also semi-transparent `OBJ` pixels over the course-list region, but they are not the primary text source.

Pixel inspection showed:

- `BG0` present at many course-list pixels
- `OBJ` also present, `prio=1`, semi-transparent
- `BG0` has `prio=0`, so incorrect `BG0` content can cover what should show through from `OBJ`

## VCOUNT / DISPSTAT / IRQ Findings

### VCOUNT

`VCOUNT` currently appears to be updating correctly.

Implementation:

- `ppu.current_scanline` advances at scanline boundaries
- `ppu.registers.vcount = current_scanline`
- reads of `0x04000006` return `regs.vcount`

Observed reads matched the current scanline numerically.

### DISPSTAT

Observed value in the relevant states:

- `dispstat = 0x0039`

Decoded:

- VBlank IRQ allow = on
- HBlank IRQ allow = on
- VCOUNT IRQ allow = on
- VCOUNT compare target = `0`

So this is not a `VCOUNT=80` setup.

### IE / Actual Interrupt Handling

Observed:

- `IE = 0x2001`

Decoded:

- VBlank IRQ enabled
- GamePak IRQ enabled
- HBlank IRQ not enabled
- VCOUNT IRQ not enabled

So although HBlank request conditions are generated every visible line, they do not actually enter CPU IRQ handling in this scenario.

This investigation therefore does not currently support:

- HBlank IRQ handler driving the mid-screen `BG0CNT` change
- VCOUNT IRQ at line 80 driving the change

## CPU/PPU Timing Findings

One major hypothesis was that CPU code was simply running too fast relative to PPU.

That hypothesis currently looks weak.

Measured using `trace_bg0cnt_timing_after_frames`:

- VBlank start at `scanline 160`
- `0x08001EB8` writes `4684` at `scanline 170 line_cycle 682`
- `0x0800C69E` reads `BG0CNT` at next-frame `scanline 1 line_cycle 772`
- `0x0800C6A4` clears `BG0CNT` to `4604` at next-frame `scanline 1 line_cycle 798`

Measured total cycles from VBlank start to `0x0800C6A4`:

- actual: `85812`
- theoretical PPU timeline to next-frame `scanline 1 line_cycle 798`: `85806`

This is only a `6`-cycle difference, so global CPU/PPU timing does not currently look grossly wrong.

## Current Best Interpretation

The following have largely been ruled out as the primary cause:

- bad asset upload
- bad decompression
- bad DMA destination/source
- palette corruption
- HBlank IRQ driven update logic
- VCOUNT register returning obviously wrong values
- CPU globally running much too fast

The remaining high-value possibilities are:

1. The emulator still misunderstands when `BG0CNT` changes become visible to text BG rendering.
2. The relationship between traced register writes and the actual displayed frame is missing one more layer of display semantics.

## Temporary Debug Helpers

Several one-off debugger binaries and experiment hooks were added during this investigation and removed again during cleanup.

They were useful for narrowing the issue down to scanline timing and `DISPSTAT`, but they are not part of the final fix.

## Useful Artifacts

- mGBA reference: `/tmp/bg.png`
- current broken frame: `/tmp/mk_after_framefix/frame_000180.png`
- split 40: `/tmp/mk_split40_test/frame_000180.png`
- split 80: `/tmp/mk_split80_test/frame_000180.png`
- split 100: `/tmp/mk_split100_test/frame_000180.png`

## Suggested Next Steps

1. Continue from the display-semantics angle, not DMA or IRQ.
2. Verify whether public hardware references specifically describe `BGxCNT` visibility timing for text BG mode changes mid-frame.
3. If no definitive hardware statement exists, keep using the split-line experiment only as a diagnostic tool, not as a fix.
4. Focus on reconciling:
   - traced `line 1` switch to `4604`
   - correct visual result that behaves like a mid-screen switch

## Follow-Up: Scanline-Start Latching Fix

The emulator previously refreshed `render_registers` at the first `ppu.step()` call of a scanline, even if CPU work had already consumed part of the visible period. That made early scanline writes visible too soon.

This was tightened so visible-period render state is only snapped at scanline start (`cycles == 0`) and then held for the rest of the line.

Observed effect after that change, using:

- `trace_scanlines_after_frames roms/mario kart/rom.gba roms/mario kart/rom.gba.state --frames 180 --press-a-at-frame 0 2 --start-line 0 --end-line 12`

Result:

- `y=0`: `bg0cnt=4684`
- `y=1`: `bg0cnt=4684`
- `y=2..`: `bg0cnt=4604`

So the old `line 1` switch was definitely too early by one line.
However, this still does **not** explain the much later apparent switch implied by the visual split experiments around line ~80.

Updated conclusion:

- scanline-start latching was one real bug
- but it is only a partial fix
- the remaining mismatch is still in display semantics, not DMA upload or gross CPU/PPU desync

## Follow-Up: DISPSTAT Low-Byte Write Bug

The next hard bug turned out to be in `DISPSTAT` byte writes, not in the road table itself.

Observed behavior before the fix:

- the game repeatedly wrote `0x04000005 = 0x49` and then `0x04000004 = 0x38`
- this should program `DISPSTAT = 0x4938`, meaning VCOUNT compare `0x49` with IRQ enables in the low byte
- in the emulator, the low-byte write to `0x04000004` incorrectly cleared the high byte
- as a result, the effective VCOUNT compare value became `0`

This explained the earlier trace anomaly:

- VCOUNT IRQs were only firing at `scanline=0`
- the road DMA callback at `0x0804AF74` was being run from the frame top
- the `BG2` HBlank DMA stream started consuming the road table immediately instead of mid-screen

The bug was in `gba_core/src/ppu/io.rs`:

- old low-byte mask: `0xFFF8`
- correct low-byte mask: `0x00F8`

The high byte of `DISPSTAT` holds the VCOUNT compare target and must survive writes to `0x04000004`.

After this fix:

- `DISPSTAT` correctly preserves `0x49` after the low-byte write
- regression tests cover both byte-write preservation and nonzero VCOUNT IRQ triggering
- `VCOUNT` IRQ trace now fires at `scanline=73`
- `trace_pc_after_frames` shows `0x0804AF74` running at `scanline=73`, not `scanline=0`
- `BG2` HBlank DMA now begins at `y=73` instead of from the top of the frame

This is the first change that directly lines up the road-effect scheduling with the game's intended mid-screen start behavior.

Current verification artifacts:

- `artifacts/mk_state0_dispstat_fix/state_frame.png`
- `artifacts/mk_state0_dispstat_fix/composite_next_frame.png`
- `artifacts/mk_state1_dispstat_fix/state_frame.png`
- `artifacts/mk_state1_dispstat_fix/composite_next_frame.png`

## Cleanup

After the root cause was confirmed, the temporary debugger bins and the `BG0CNT` split-line experiment hook were removed again.

The permanent changes that remain are:

- the scanline-start render register latch fix
- the `DISPSTAT` low-byte write fix
- the regression tests that cover both behaviors

## Follow-Up: Vehicle Disappearance / GP Slowdown

After the road scheduling fix, a separate Mario Kart issue remained:

- in Mario GP mode, frames with other racers on screen could become very slow
- when the player car moved fast enough, the player/racer sprites sometimes disappeared
- when the car was stopped or moving slowly, sprites were usually rendered correctly
- `roms/mario kart/rom.gba.state.1` could render cars correctly, while `roms/mario kart/rom.gba.state` reproduced the bad frame

The BG layer captures were misleading for this issue because BG0/BG1/BG2/BG3 were individually sane. The failure was later in the per-frame object/OAM build path: heavy frames missed enough CPU-side work that the object list was not populated in time.

Important trace points during the investigation:

- object/near-object preprocessing around `0x0802F224..0x0802F394`
- object/OAM list work around `0x08049B0C` and `0x08049D78`
- sound scheduler around `0x0805F800..0x0805FC00`
- IWRAM mixer routines around `0x03002600..0x03002940`, especially `0x03002898`

The useful conclusion was that audio still matters even though the emulator does not output sound. Mario Kart runs its own sound scheduler and mixer on the CPU. If Direct Sound timer/FIFO DMA behavior is missing or timed poorly, the game loop cadence changes and the CPU budget for object rendering becomes wrong.

Permanent fixes from this phase:

- CPU pipeline tail fetch now happens after executing the current non-branch instruction, so memory accesses inside the instruction are ordered before the next opcode fetch.
- Game Pak opcode stream tracking keeps a one-halfword overlap path even when WAITCNT prefetch is disabled.
- Game Pak data sequentiality is tracked separately from generic data sequentiality, so non-Game Pak traffic does not accidentally drive ROM sequential timing.
- Direct Sound FIFO A/B state was added, including FIFO reset bits, timer overflow sample consumption, and special DMA requests when FIFO depth reaches the refill threshold.
- Sound special DMA now transfers four words to the fixed FIFO address and keeps repeat DMA source/destination bookkeeping compatible with FIFO mode.
- Save-state load reconstructs transient Game Pak prefetch state from the CPU pipeline and clears Direct Sound FIFO state, instead of restoring stale runtime internals.
- Save writes in the frontend now use an atomic temp-file-and-rename path to avoid corrupt partial `.state` / save files.

User-facing result:

- the road starts in the correct screen region
- the duplicated/garbled road effect is gone
- Mario GP frames with other racers now render player/racer sprites correctly in local frontend testing

Verification used for the final cleanup:

- `cargo test -q -p gba_core --test mgba_timing_loadstore_test`
- `cargo test -q -p gba_core --test bus_test`
- `cargo test -q -p gba_core --test ppu_test`
- `cargo build -q -p frontend`
