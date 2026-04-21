use gba_core::bus::Bus;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavePaths {
    pub legacy: PathBuf,
    pub sram: PathBuf,
    pub eeprom: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveKind {
    Sram,
    Eeprom,
}

pub fn save_paths(rom_path: &Path) -> SavePaths {
    let rom = rom_path.to_string_lossy();
    SavePaths {
        legacy: PathBuf::from(format!("{rom}.sav")),
        sram: PathBuf::from(format!("{rom}.sram.sav")),
        eeprom: PathBuf::from(format!("{rom}.eeprom.sav")),
    }
}

pub fn load_save(bus: &mut Bus, rom_path: &Path) -> io::Result<()> {
    let paths = save_paths(rom_path);

    if try_load_save_file(bus, &paths.sram, SaveKind::Sram)? {
        return Ok(());
    }
    if try_load_save_file(bus, &paths.eeprom, SaveKind::Eeprom)? {
        return Ok(());
    }

    if !paths.legacy.exists() {
        return Ok(());
    }

    let save_data = fs::read(&paths.legacy)?;
    match detect_legacy_save_kind(save_data.len()) {
        SaveKind::Eeprom => {
            let len = save_data.len().min(bus.eeprom.data.len());
            bus.eeprom.data[..len].copy_from_slice(&save_data[..len]);
            println!(
                "Loaded legacy EEPROM save ({} bytes) from {}",
                save_data.len(),
                paths.legacy.display()
            );
        }
        SaveKind::Sram => {
            let len = save_data.len().min(bus.sram.len());
            bus.sram[..len].copy_from_slice(&save_data[..len]);
            println!(
                "Loaded legacy SRAM save ({} bytes) from {}",
                save_data.len(),
                paths.legacy.display()
            );
        }
    }

    Ok(())
}

pub fn save_dirty_data(bus: &mut Bus, rom_path: &Path) -> io::Result<()> {
    let paths = save_paths(rom_path);

    if bus.sram_dirty {
        atomic_write(&paths.sram, &bus.sram[..])?;
        bus.sram_dirty = false;
        println!("SRAM saved to {}", paths.sram.display());
    }

    if bus.eeprom.dirty {
        atomic_write(&paths.eeprom, &bus.eeprom.data[..])?;
        bus.eeprom.dirty = false;
        println!("EEPROM saved to {}", paths.eeprom.display());
    }

    Ok(())
}

fn try_load_save_file(bus: &mut Bus, path: &Path, kind: SaveKind) -> io::Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let save_data = fs::read(path)?;
    match kind {
        SaveKind::Sram => {
            let len = save_data.len().min(bus.sram.len());
            bus.sram[..len].copy_from_slice(&save_data[..len]);
            println!(
                "Loaded SRAM save ({} bytes) from {}",
                save_data.len(),
                path.display()
            );
        }
        SaveKind::Eeprom => {
            let len = save_data.len().min(bus.eeprom.data.len());
            bus.eeprom.data[..len].copy_from_slice(&save_data[..len]);
            println!(
                "Loaded EEPROM save ({} bytes) from {}",
                save_data.len(),
                path.display()
            );
        }
    }

    Ok(true)
}

fn detect_legacy_save_kind(len: usize) -> SaveKind {
    match len {
        512 | 8192 => SaveKind::Eeprom,
        _ => SaveKind::Sram,
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp_path = PathBuf::from(format!("{}.tmp", path.to_string_lossy()));
    let mut file = fs::File::create(&tmp_path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::save_paths;
    use std::path::Path;

    #[test]
    fn save_paths_are_split_by_save_type() {
        let paths = save_paths(Path::new("tests/roms/mario.gba"));
        assert_eq!(paths.legacy, Path::new("tests/roms/mario.gba.sav"));
        assert_eq!(paths.sram, Path::new("tests/roms/mario.gba.sram.sav"));
        assert_eq!(paths.eeprom, Path::new("tests/roms/mario.gba.eeprom.sav"));
    }
}
