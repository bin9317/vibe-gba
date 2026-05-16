# vibe-gba

`vibe-gba` is an experimental Game Boy Advance emulator written in Rust.

![Demo](demo.mp4)


The repository is organized as a small Cargo workspace:

- `gba_core`: emulator core for CPU, bus, PPU, DMA, timers, backup media, and save states.
- `frontend`: desktop frontend built with `winit` and `pixels`.
- `cli_debugger`: headless debugging tools for stepping, screenshots, and test-suite automation.

## Project Status

This is a work-in-progress emulator focused on correctness and debuggability. It can run a number of test ROMs and some commercial games, but compatibility is incomplete.

Implemented or partially implemented:

- ARM and Thumb CPU execution
- memory bus, waitstates, and Game Pak prefetch timing
- PPU backgrounds, sprites, windows, blending, and affine modes
- DMA, timers, IRQs, keypad input
- SRAM, EEPROM, Flash, and save states
- Direct Sound FIFO timing and timer-triggered sound DMA

Not implemented yet:

- audio output
- complete GBA hardware edge cases
- polished end-user UI

## BIOS And Legal Assets

Normal frontend runs require a real GBA BIOS. This repository does not include one, and it does not include commercial ROMs. Place your own legally obtained BIOS at:

```text
gba_bios.bin
```

If `gba_bios.bin` is missing, the frontend exits with an error instead of using an implicit fallback. The CLI debugger has an explicit `--skip-bios` flag for development-only experiments.

ROMs, BIOS files, save states, and generated screenshots are intentionally ignored by Git.

## Requirements

- Rust toolchain with Cargo
- A desktop environment supported by `winit`

## Quick Start

Run the desktop frontend with a ROM:

```bash
cargo run --release -p frontend -- <path-to-rom.gba>
```

Run the core regression tests used most often during development:

```bash
cargo test -q -p gba_core --test mgba_timing_loadstore_test
cargo test -q -p gba_core --test bus_test
cargo test -q -p gba_core --test ppu_test
```

Run the CLI debugger:

```bash
cargo run -p cli_debugger --bin cli_debugger -- <path-to-rom.gba>
```

Capture frames from a ROM or save state:

```bash
cargo run --release -p cli_debugger --bin snapshot_frames -- \
  <path-to-rom.gba> \
  --output-dir artifacts/snapshots \
  --image-format png \
  --snapshot-every-frames 60 \
  --max-frames 600
```

## Frontend Controls

- `Z`: A
- `X`: B
- `Backspace`: Select
- `Enter`: Start
- Arrow keys: D-pad
- `A`: L
- `S`: R
- `F5`: save state
- `F8`: load state
- `Cmd/Ctrl + 0..9`: select save-state slot
- `Cmd/Ctrl + S`: save selected slot
- `Cmd/Ctrl + L`: load selected slot

## Test ROMs

The repository includes small test ROMs under `tests/roms/` and references external test suites as submodules under `third_party/`.

Initialize submodules after cloning:

```bash
git submodule update --init --recursive
```

Run the suite helper:

```bash
cargo run --release -p cli_debugger --bin run_suite -- <path-to-mgba-suite.gba>
```

## Documentation

- [Documentation index](docs/README.md)
- [System architecture](docs/system_design.md)
- [Debugging guide](docs/debugging_guide.md)
- [GBA test-suite notes](docs/gba-tests.md)
- [Timing system design](docs/timing_system_design.md)

## License

No license has been selected yet. Add a `LICENSE` file before publishing if you want others to use, modify, or redistribute the code under explicit terms.
