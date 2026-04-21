/// Runs suite.gba, auto-navigates through all test suites, then dumps FAIL lines from SRAM.
///
/// Navigation strategy:
///   - Wait for the main menu to appear
///   - For each suite with an automatic runner: press A (enter), wait for tests to finish,
///     press B (return to menu)
///   - After returning, press Down to move to the next suite
use cli_debugger::debug_support::{load_gba_with_bios, step_frames};
use gba_core::Gba;
use std::fs;

const KEY_A: u16 = 1 << 0;
const KEY_B: u16 = 1 << 1;
const KEY_DOWN: u16 = 1 << 7;
const ALL_RELEASED: u16 = 0x03FF;
const ACTIVE_INFO_MAGIC: [u8; 4] = *b"Info";
const IWRAM_START: u32 = 0x0300_0000;
const IWRAM_LEN: usize = 32 * 1024;
const EWRAM_START: u32 = 0x0200_0000;
const EWRAM_LEN: usize = 256 * 1024;
const MENU_WAIT_FRAMES: u32 = 420;
const SETTLE_FRAMES: u32 = 10;
const SUITE_TIMEOUT_FRAMES: u32 = 3_600;

struct SuiteSpec {
    name: &'static str,
    auto_run: bool,
}

const SUITES: [SuiteSpec; 14] = [
    SuiteSpec {
        name: "Memory tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "I/O read tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "Timing tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "Timer count-up tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "Timer IRQ tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "Shifter tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "Carry tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "Multiply long tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "BIOS math tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "DMA tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "SIO read tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "SIO timing tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "Misc edge tests",
        auto_run: true,
    },
    SuiteSpec {
        name: "Video tests",
        auto_run: false,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveInfo {
    addr: u32,
    suite_id: u8,
    test_id: u8,
    subtest_id: u16,
}

impl ActiveInfo {
    fn suite_matches(self, suite_idx: usize) -> bool {
        self.suite_id == suite_idx as u8
    }

    fn has_active_test(self) -> bool {
        self.test_id != 0xFF
    }
}

fn set_keys(gba: &mut Gba, keys: u16) {
    // key_state is active-low: 0 = pressed
    gba.bus.key_state = ALL_RELEASED & !keys;
}

fn release_keys(gba: &mut Gba) {
    gba.bus.key_state = ALL_RELEASED;
}

fn press(gba: &mut Gba, key: u16, hold_frames: u32, wait_frames: u32) {
    set_keys(gba, key);
    step_frames(gba, hold_frames);
    release_keys(gba);
    step_frames(gba, wait_frames);
}

fn find_active_info(gba: &Gba) -> Option<u32> {
    for offset in 0..=(IWRAM_LEN - ACTIVE_INFO_MAGIC.len()) {
        if gba.bus.on_chip_wram[offset..offset + ACTIVE_INFO_MAGIC.len()] == ACTIVE_INFO_MAGIC {
            return Some(IWRAM_START + offset as u32);
        }
    }

    for offset in 0..=(EWRAM_LEN - ACTIVE_INFO_MAGIC.len()) {
        if gba.bus.on_board_wram[offset..offset + ACTIVE_INFO_MAGIC.len()] == ACTIVE_INFO_MAGIC {
            return Some(EWRAM_START + offset as u32);
        }
    }

    None
}

fn read_active_info(gba: &mut Gba, addr: u32) -> Option<ActiveInfo> {
    if gba.bus.internal_read8(addr) != ACTIVE_INFO_MAGIC[0]
        || gba.bus.internal_read8(addr + 1) != ACTIVE_INFO_MAGIC[1]
        || gba.bus.internal_read8(addr + 2) != ACTIVE_INFO_MAGIC[2]
        || gba.bus.internal_read8(addr + 3) != ACTIVE_INFO_MAGIC[3]
    {
        return None;
    }

    Some(ActiveInfo {
        addr,
        subtest_id: gba.bus.internal_read16(addr + 4),
        test_id: gba.bus.internal_read8(addr + 6),
        suite_id: gba.bus.internal_read8(addr + 7),
    })
}

fn wait_for_suite_completion(
    gba: &mut Gba,
    active_info_addr: Option<u32>,
    suite_idx: usize,
    timeout_frames: u32,
) -> Result<bool, String> {
    let Some(active_info_addr) = active_info_addr else {
        step_frames(gba, timeout_frames);
        return Ok(false);
    };

    let mut stable_completion_frames = 0;

    for _ in 0..timeout_frames {
        step_frames(gba, 1);
        let Some(info) = read_active_info(gba, active_info_addr) else {
            return Err(format!(
                "activeTestInfo marker disappeared at {:08X}",
                active_info_addr
            ));
        };

        let suite_finished = !info.suite_matches(suite_idx) || !info.has_active_test();
        if suite_finished {
            stable_completion_frames += 1;
            if stable_completion_frames >= 2 {
                return Ok(true);
            }
        } else {
            stable_completion_frames = 0;
        }
    }

    Ok(false)
}

fn extract_sram_text(gba: &Gba) -> String {
    let sram = gba.bus.sram.as_ref();
    let end = sram.iter().position(|&b| b == 0).unwrap_or(sram.len());
    let bytes = &sram[..end];
    String::from_utf8_lossy(bytes).into_owned()
}

fn validate_rom(rom_path: &str) -> Result<(), String> {
    let rom = fs::read(rom_path).map_err(|err| format!("failed to read ROM {rom_path}: {err}"))?;
    if rom.len() < 192 || rom == b"Not Found" {
        return Err(format!(
            "ROM {rom_path} is not a valid GBA image ({} bytes). Current file looks like a placeholder, not a built mgba-suite ROM.",
            rom.len()
        ));
    }
    Ok(())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let rom_path = args
        .next()
        .unwrap_or_else(|| "roms/suite/suite.gba".to_string());
    let bios_path = args.next().unwrap_or_else(|| "gba_bios.bin".to_string());

    let suite_timeout_frames: u32 = args
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(SUITE_TIMEOUT_FRAMES);

    validate_rom(&rom_path).expect("invalid suite ROM");

    eprintln!("Loading {rom_path}...");
    let mut gba =
        load_gba_with_bios(&rom_path, Some(&bios_path), false).expect("failed to load GBA");

    // Wait for BIOS to finish and main menu to appear (~400 frames)
    eprintln!("Waiting for menu...");
    step_frames(&mut gba, MENU_WAIT_FRAMES);

    let active_info_addr = find_active_info(&gba);
    if let Some(addr) = active_info_addr {
        eprintln!("Found activeTestInfo at {addr:08X}");
    } else {
        eprintln!(
            "WARNING: failed to locate activeTestInfo in WRAM; falling back to fixed frame waits"
        );
    }

    for (suite_idx, suite) in SUITES.iter().enumerate() {
        eprintln!("Entering suite {suite_idx}: {}...", suite.name);
        press(&mut gba, KEY_A, 3, SETTLE_FRAMES);

        if suite.auto_run {
            match wait_for_suite_completion(
                &mut gba,
                active_info_addr,
                suite_idx,
                suite_timeout_frames,
            ) {
                Ok(true) => {}
                Ok(false) => {
                    if active_info_addr.is_some() {
                        eprintln!(
                            "WARNING: suite {suite_idx} ({}) timed out after {} frames",
                            suite.name, suite_timeout_frames
                        );
                    } else {
                        eprintln!(
                            "WARNING: suite {suite_idx} ({}) used fixed wait fallback ({} frames)",
                            suite.name, suite_timeout_frames
                        );
                    }
                }
                Err(err) => eprintln!(
                    "WARNING: suite {suite_idx} ({}) lost activity tracking: {}",
                    suite.name, err
                ),
            }
        } else {
            eprintln!(
                "Skipping automatic execution for suite {suite_idx} ({}): interactive-only",
                suite.name
            );
        }

        press(&mut gba, KEY_B, 3, SETTLE_FRAMES);
        if suite_idx + 1 < SUITES.len() {
            press(&mut gba, KEY_DOWN, 3, SETTLE_FRAMES);
        }
    }

    let text = extract_sram_text(&gba);

    println!("=== SRAM output ===");
    let show_all = std::env::var("SHOW_ALL").is_ok();
    let mut last_context: Option<&str> = None;
    let mut printed_context: Option<&str> = None;
    for line in text.lines() {
        if line.starts_with("Timing test: ")
            || line.starts_with("Memory test: ")
            || line.starts_with("I/O read test: ")
            || line.starts_with("DMA test: ")
            || line.starts_with("SIO test: ")
            || line.starts_with("Misc edge test: ")
        {
            last_context = Some(line);
            if show_all {
                println!("{line}");
            }
            continue;
        }

        if show_all || line.contains("FAIL") {
            if !show_all && line.contains("FAIL") && last_context != printed_context {
                if let Some(context) = last_context {
                    println!("{context}");
                    printed_context = Some(context);
                }
            }
            println!("{line}");
        }
    }

    let fail_count = text.lines().filter(|l| l.contains("FAIL")).count();
    eprintln!("FAIL lines: {fail_count}");
}

#[cfg(test)]
mod tests {
    use super::ActiveInfo;

    #[test]
    fn completion_treats_suite_local_idle_as_done_even_with_stale_subtest() {
        let running = ActiveInfo {
            addr: 0,
            suite_id: 3,
            test_id: 7,
            subtest_id: 42,
        };
        let idle = ActiveInfo {
            addr: 0,
            suite_id: 3,
            test_id: 0xFF,
            subtest_id: 42,
        };

        assert!(running.suite_matches(3));
        assert!(running.has_active_test());
        assert!(idle.suite_matches(3));
        assert!(!idle.has_active_test());
    }

    #[test]
    fn completion_treats_menu_return_as_done() {
        let menu = ActiveInfo {
            addr: 0,
            suite_id: 0xFF,
            test_id: 0xFF,
            subtest_id: 0xFFFF,
        };

        assert!(!menu.suite_matches(5));
        assert!(!menu.has_active_test());
    }
}
