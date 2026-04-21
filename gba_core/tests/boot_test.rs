use gba_core::{GAMEPAK_ROM_END, GAMEPAK_ROM_START, GBA_CYCLES_PER_FRAME, Gba};
use std::fs;
use std::path::Path;

fn try_load_file(path: &str) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

fn try_load_bios() -> Option<Vec<u8>> {
    try_load_file("../gba_bios.bin")
}

fn try_load_armwrestler_rom() -> Option<Vec<u8>> {
    let candidates = [
        "../roms/armwrestler/armwrestler.gba",
        "../tests/roms/armwrestler.gba",
    ];
    let path = candidates
        .into_iter()
        .find(|path| Path::new(path).exists())
        .unwrap_or("../roms/armwrestler/armwrestler.gba");
    try_load_file(path)
}

fn try_load_mario_rom() -> Option<Vec<u8>> {
    let candidates = ["../roms/mario/mario.gba"];
    let path = candidates
        .into_iter()
        .find(|path| Path::new(path).exists())
        .unwrap_or("../roms/mario/mario.gba");
    try_load_file(path)
}

fn try_load_mario_kart_rom() -> Option<Vec<u8>> {
    let candidates = ["../roms/mario kart/rom.gba"];
    let path = candidates
        .into_iter()
        .find(|path| Path::new(path).exists())
        .unwrap_or("../roms/mario kart/rom.gba");
    try_load_file(path)
}

fn external_asset_or_skip(label: &str, value: Option<Vec<u8>>) -> Option<Vec<u8>> {
    if value.is_none() {
        eprintln!("skipping external boot test: missing {label}");
    }
    value
}

fn run_until_rom_entry(gba: &mut Gba, max_frames: u32) -> Option<u32> {
    for frame in 0..max_frames {
        let start_cycles = gba.bus.cycles;
        while (gba.bus.cycles - start_cycles) < GBA_CYCLES_PER_FRAME {
            gba.step();
        }

        if (GAMEPAK_ROM_START..GAMEPAK_ROM_END).contains(&gba.cpu.registers[15]) {
            return Some(frame);
        }
    }

    None
}

#[test]
fn test_real_bios_reaches_armwrestler_rom() {
    let Some(bios) = external_asset_or_skip("gba_bios.bin", try_load_bios()) else {
        return;
    };
    let Some(rom) = external_asset_or_skip("armwrestler ROM", try_load_armwrestler_rom()) else {
        return;
    };

    let mut gba = Gba::new();
    gba.bus.load_bios(&bios);
    gba.bus.load_rom(&rom);

    let frame = run_until_rom_entry(&mut gba, 320)
        .unwrap_or_else(|| panic!("did not enter ROM, final PC={:08X}", gba.cpu.registers[15]));

    assert!(frame <= 271, "ROM entry regressed to frame {frame}");
    assert!((GAMEPAK_ROM_START..GAMEPAK_ROM_END).contains(&gba.cpu.registers[15]));
}

#[test]
fn test_real_bios_reaches_mario_rom() {
    let Some(bios) = external_asset_or_skip("gba_bios.bin", try_load_bios()) else {
        return;
    };
    let Some(rom) = external_asset_or_skip("Mario ROM", try_load_mario_rom()) else {
        return;
    };

    let mut gba = Gba::new();
    gba.bus.load_bios(&bios);
    gba.bus.load_rom(&rom);

    let frame = run_until_rom_entry(&mut gba, 320).unwrap_or_else(|| {
        panic!(
            "did not enter Mario ROM, final PC={:08X}",
            gba.cpu.registers[15]
        )
    });

    assert!(frame <= 271, "Mario ROM entry regressed to frame {frame}");
    assert!((GAMEPAK_ROM_START..GAMEPAK_ROM_END).contains(&gba.cpu.registers[15]));
}

#[test]
fn test_real_bios_reaches_mario_kart_rom() {
    let Some(bios) = external_asset_or_skip("gba_bios.bin", try_load_bios()) else {
        return;
    };
    let Some(rom) = external_asset_or_skip("Mario Kart ROM", try_load_mario_kart_rom()) else {
        return;
    };

    let mut gba = Gba::new();
    gba.bus.load_bios(&bios);
    gba.bus.load_rom(&rom);

    let frame = run_until_rom_entry(&mut gba, 320).unwrap_or_else(|| {
        panic!(
            "did not enter Mario Kart ROM, final PC={:08X}",
            gba.cpu.registers[15]
        )
    });

    assert!(
        frame <= 271,
        "Mario Kart ROM entry regressed to frame {frame}"
    );
    assert!((GAMEPAK_ROM_START..GAMEPAK_ROM_END).contains(&gba.cpu.registers[15]));
}
