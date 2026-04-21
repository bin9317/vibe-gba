# `gba-tests` Results

## Batch 1 Smoke Run

| ROM | Status | First Failed Test | Notes |
| --- | --- | --- | --- |
| `memory.gba` | pass | - | final screen shows `All tests passed` |
| `bios.gba` | pass | - | BIOS/open-bus latch model fixed |
| `arm.gba` | pass | - | data-processing and block-transfer edge cases fixed |
| `thumb.gba` | pass | - | Thumb `POP {pc}` and empty/base-in-rlist LDM/STM fixed |

## Batch 2 PPU and Save

| ROM | Status | First Failed Test | Notes |
| --- | --- | --- | --- |
| `ppu/hello.gba` | pass | - | final screen shows `Hello world!` |
| `ppu/shades.gba` | pass | - | gradient output matches expected shades ramp |
| `ppu/stripes.gba` | pass | - | stripe output matches expected pattern |
| `save/none.gba` | pass | - | required `0x0Fxxxxxx` mirror reads returning `0xFF` |
| `save/sram.gba` | pass | - | required save-memory mirror plus byte-lane semantics for halfword/word access |
| `save/flash64.gba` | pass | - | required minimal flash command state machine with erase support |
| `save/flash128.gba` | pass | - | required flash bank switching and full-chip/sector erase semantics |

## Notes

- Screenshots were generated with `snapshot_frames`.
- `bios.gba` test `001` is defined in `third_party/gba-tests/bios/bios.asm`.
- That test checks BIOS read behavior immediately after startup, so the current failure points at BIOS/open-bus semantics rather than generic rendering.
- `arm.gba` initially exposed rotated-immediate carry handling, bad `CMP/CMN/TST/TEQ` with `Rd=R15`, and block-transfer empty/base-in-rlist semantics.
- `thumb.gba` initially exposed incorrect state switching on `POP {pc}` and incorrect empty-rlist / base-in-rlist Thumb `LDMIA`/`STMIA` behavior.
- `save/*.gba` exposed three distinct gaps: `0x0Fxxxxxx` SRAM/FLASH mirroring, save-memory byte-lane behavior for halfword/word CPU accesses, and missing flash command/erase/bank-switch handling.
