use cli_debugger::debug_support::{
    load_gba, print_dma_summary, print_ppu_summary, step_frames, write_frame_image,
};
use gba_core::Gba;
use std::io::{self, Write};
use std::path::Path;

#[derive(Default)]
struct DebuggerState {
    trace_pcs: Vec<u32>,
    watches: Vec<Watch>,
}

#[derive(Clone, Copy)]
struct Watch {
    addr: u32,
    size: WatchSize,
}

#[derive(Clone, Copy)]
enum WatchSize {
    Byte,
    Halfword,
    Word,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut rom_path = None;
    let mut skip_bios = false;
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--skip-bios" => skip_bios = true,
            value if rom_path.is_none() => rom_path = Some(&value[..]),
            _ => {}
        }
    }
    let rom_path = rom_path.unwrap_or("tests/roms/armwrestler.gba");

    println!("GBA CLI Debugger");
    println!("Type 'help' for commands.");

    let mut gba = load_gba(rom_path, skip_bios).unwrap_or_else(|err| {
        eprintln!("Failed to initialize GBA: {err}");
        std::process::exit(1);
    });
    println!("Loaded {} (skip_bios={})", rom_path, skip_bios);
    let mut debugger = DebuggerState::default();

    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) | Err(_) => break,
            _ => {}
        }

        let input = input.trim();
        let mut parts = input.split_whitespace();
        let cmd = parts.next().unwrap_or("");

        match cmd {
            "help" => print_help(),
            "step" | "s" => {
                let count = parse_u32(parts.next(), 1);
                for _ in 0..count {
                    gba.step();
                    maybe_print_trace_hit(&mut gba, &debugger);
                }
                print_state(&mut gba);
            }
            "frame" | "f" => {
                let count = parse_u32(parts.next(), 1);
                step_frames(&mut gba, count);
                print_state(&mut gba);
            }
            "regs" => {
                print_regs(&gba);
            }
            "dis" => {
                print_state(&mut gba);
            }
            "ppu" => {
                print_ppu_summary(&gba);
            }
            "dma" => {
                print_dma_summary(&gba);
            }
            "run_until" | "ru" => {
                let Some(target) = parts.next() else {
                    println!("usage: run_until <pc_hex> [max_steps]");
                    continue;
                };
                let target_pc = parse_hex_u32(target).unwrap_or_else(|| {
                    println!("invalid pc: {target}");
                    0
                });
                if target_pc == 0 && target != "0" && target != "00000000" {
                    continue;
                }

                let max_steps = parse_u64(parts.next(), 1_000_000);
                let mut steps = 0u64;
                while gba.cpu.registers[15] != target_pc && steps < max_steps {
                    gba.step();
                    steps += 1;
                    maybe_print_trace_hit(&mut gba, &debugger);
                }

                if gba.cpu.registers[15] == target_pc {
                    println!(
                        "Stopped at {:08X} after {} steps",
                        gba.cpu.registers[15], steps
                    );
                } else {
                    println!(
                        "Target {:08X} not reached after {} steps; current PC {:08X}",
                        target_pc, steps, gba.cpu.registers[15]
                    );
                }
                print_state(&mut gba);
            }
            "trace_run" | "tr" => {
                let max_steps = parse_u64(parts.next(), 1_000_000);
                let max_hits = parse_u32(parts.next(), 1) as usize;
                if debugger.trace_pcs.is_empty() {
                    println!("trace_run requires at least one trace_pc");
                    continue;
                }

                let mut steps = 0u64;
                let mut hits = 0usize;
                while steps < max_steps && hits < max_hits {
                    gba.step();
                    steps += 1;
                    if maybe_print_trace_hit(&mut gba, &debugger) {
                        hits += 1;
                    }
                }

                println!(
                    "trace_run finished: steps={} hits={} current_pc={:08X}",
                    steps, hits, gba.cpu.registers[15]
                );
                print_state(&mut gba);
            }
            "snapshot" => {
                let Some(path) = parts.next() else {
                    println!("usage: snapshot <path.png|path.bmp>");
                    continue;
                };
                let path = Path::new(path);
                if let Err(err) = write_frame_image(path, &gba.bus.ppu.frame_buffer[..]) {
                    println!("failed to write snapshot {}: {}", path.display(), err);
                } else {
                    println!("Wrote {}", path.display());
                }
            }
            "dump" => {
                let path = Path::new("frame.png");
                if let Err(err) = write_frame_image(path, &gba.bus.ppu.frame_buffer[..]) {
                    println!("failed to write {}: {}", path.display(), err);
                } else {
                    println!("Wrote {}", path.display());
                }
            }
            "key" => {
                let key = parts.next().unwrap_or("0");
                let Some(mask) = parse_hex_u16(key) else {
                    println!("invalid key mask: {key}");
                    continue;
                };
                gba.bus.key_state = (!mask) & 0x03FF;
                println!("Key state updated to: {:04X}", gba.bus.key_state);
            }
            "trace_pc" => {
                let Some(pc) = parts.next() else {
                    println!("usage: trace_pc <pc_hex>");
                    continue;
                };
                let Some(pc) = parse_hex_u32(pc) else {
                    println!("invalid pc: {pc}");
                    continue;
                };
                if debugger.trace_pcs.contains(&pc) {
                    println!("trace point already set at {:08X}", pc);
                } else {
                    debugger.trace_pcs.push(pc);
                    debugger.trace_pcs.sort_unstable();
                    println!("Added trace point {:08X}", pc);
                }
            }
            "clear_trace_pc" => {
                debugger.trace_pcs.clear();
                println!("Cleared trace points");
            }
            "watch" => {
                let Some(addr) = parts.next() else {
                    println!("usage: watch <addr_hex> [1|2|4]");
                    continue;
                };
                let Some(addr) = parse_hex_u32(addr) else {
                    println!("invalid address: {addr}");
                    continue;
                };
                let Some(size) = parse_watch_size(parts.next()) else {
                    println!("invalid watch size; use 1, 2, or 4");
                    continue;
                };
                debugger.watches.push(Watch { addr, size });
                println!("Added watch {:08X} size {}", addr, size.bytes());
            }
            "clear_watch" => {
                debugger.watches.clear();
                println!("Cleared watches");
            }
            "trace_status" => {
                print_trace_status(&debugger);
            }
            "dump_mem" => {
                let Some(addr) = parts.next() else {
                    println!("usage: dump_mem <addr_hex>");
                    continue;
                };
                let Some(addr) = parse_hex_u32(addr) else {
                    println!("invalid address: {addr}");
                    continue;
                };
                for i in 0..16 {
                    print!("{:02X} ", gba.bus.internal_read8(addr + i));
                }
                println!();
            }
            "poke" => {
                let Some(addr) = parts.next() else {
                    println!("usage: poke <addr_hex> <value_hex> [1|2|4]");
                    continue;
                };
                let Some(addr) = parse_hex_u32(addr) else {
                    println!("invalid address: {addr}");
                    continue;
                };
                let Some(value) = parts.next() else {
                    println!("usage: poke <addr_hex> <value_hex> [1|2|4]");
                    continue;
                };
                let Some(value) = parse_hex_u32(value) else {
                    println!("invalid value: {value}");
                    continue;
                };
                let Some(size) = parse_watch_size(parts.next()) else {
                    println!("invalid write size; use 1, 2, or 4");
                    continue;
                };
                match size {
                    WatchSize::Byte => gba.bus.internal_write8(addr, value as u8),
                    WatchSize::Halfword => gba.bus.internal_write16(addr, value as u16),
                    WatchSize::Word => gba.bus.internal_write32(addr, value),
                }
                println!("Wrote {:08X} size {} to {:08X}", value, size.bytes(), addr);
            }
            "quit" | "q" => {
                break;
            }
            "" => {}
            _ => {
                println!("Unknown command: {}", cmd);
            }
        }
    }
}

fn print_help() {
    println!("Commands:");
    println!("  step (s) [n]             - Step n CPU instructions (default 1)");
    println!("  frame (f) [n]            - Advance n video frames (default 1)");
    println!("  run_until (ru) <pc> [n]  - Step until PC matches <pc> or n steps");
    println!("  trace_run (tr) [n] [m]   - Run up to n steps and log up to m trace hits");
    println!("  trace_pc <pc_hex>        - Log registers/watch values when PC matches");
    println!("  clear_trace_pc           - Remove all trace PCs");
    println!("  watch <addr_hex> [1|2|4] - Add a memory watch printed on trace hits");
    println!("  clear_watch              - Remove all memory watches");
    println!("  trace_status             - Show current trace PCs and watches");
    println!("  regs                     - Print CPU registers");
    println!("  dis                      - Print current instruction");
    println!("  ppu                      - Print PPU register summary");
    println!("  dma                      - Print DMA channel summary");
    println!("  key <mask_hex>           - Set pressed keys with GBA KEYINPUT bitmask");
    println!("  snapshot <path>          - Write framebuffer to PNG/BMP");
    println!("  dump                     - Write framebuffer to frame.png");
    println!("  dump_mem <addr_hex>      - Dump 16 bytes from memory");
    println!("  poke <addr> <value> [1|2|4] - Write a byte/halfword/word to memory");
    println!("  quit (q)                 - Exit debugger");
}

fn parse_u32(value: Option<&str>, default: u32) -> u32 {
    value.and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn parse_u64(value: Option<&str>, default: u64) -> u64 {
    value.and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn parse_hex_u16(value: &str) -> Option<u16> {
    u16::from_str_radix(value.trim_start_matches("0x"), 16).ok()
}

fn parse_hex_u32(value: &str) -> Option<u32> {
    u32::from_str_radix(value.trim_start_matches("0x"), 16).ok()
}

fn parse_watch_size(value: Option<&str>) -> Option<WatchSize> {
    match value.unwrap_or("4") {
        "1" => Some(WatchSize::Byte),
        "2" => Some(WatchSize::Halfword),
        "4" => Some(WatchSize::Word),
        _ => None,
    }
}

fn print_regs(gba: &Gba) {
    println!("Registers:");
    for i in 0..4 {
        println!(
            "R{:<2}: {:08X}   R{:<2}: {:08X}   R{:<2}: {:08X}   R{:<2}: {:08X}",
            i * 4,
            gba.cpu.registers[i * 4],
            i * 4 + 1,
            gba.cpu.registers[i * 4 + 1],
            i * 4 + 2,
            gba.cpu.registers[i * 4 + 2],
            i * 4 + 3,
            gba.cpu.registers[i * 4 + 3],
        );
    }
    println!(
        "CPSR: {:08X}  (Thumb: {})",
        gba.cpu.cpsr,
        gba.cpu.get_thumb_mode()
    );
}

fn print_state(gba: &mut Gba) {
    let pc = gba.cpu.registers[15];
    let thumb = gba.cpu.get_thumb_mode();
    if thumb {
        let instr = gba.bus.internal_read16(pc & !1);
        println!("PC: {:08X}  Instr: {:04X}  (THUMB)", pc, instr);
    } else {
        let instr = gba.bus.internal_read32(pc & !3);
        println!("PC: {:08X}  Instr: {:08X}  (ARM)", pc, instr);
    }
    print_regs(gba);
}

fn maybe_print_trace_hit(gba: &mut Gba, debugger: &DebuggerState) -> bool {
    let pc = gba.cpu.registers[15];
    if !debugger.trace_pcs.contains(&pc) {
        return false;
    }

    let thumb = gba.cpu.get_thumb_mode();
    if thumb {
        let instr = gba.bus.internal_read16(pc & !1);
        println!(
            "trace hit pc={:08X} instr={:04X} mode=THUMB sp={:08X} lr={:08X} cpsr={:08X}",
            pc, instr, gba.cpu.registers[13], gba.cpu.registers[14], gba.cpu.cpsr
        );
    } else {
        let instr = gba.bus.internal_read32(pc & !3);
        println!(
            "trace hit pc={:08X} instr={:08X} mode=ARM sp={:08X} lr={:08X} cpsr={:08X}",
            pc, instr, gba.cpu.registers[13], gba.cpu.registers[14], gba.cpu.cpsr
        );
    }

    println!(
        "r0={:08X} r1={:08X} r2={:08X} r3={:08X}",
        gba.cpu.registers[0], gba.cpu.registers[1], gba.cpu.registers[2], gba.cpu.registers[3]
    );

    for watch in &debugger.watches {
        match watch.size {
            WatchSize::Byte => println!(
                "watch {:08X} [1] = {:02X}",
                watch.addr,
                gba.bus.internal_read8(watch.addr)
            ),
            WatchSize::Halfword => println!(
                "watch {:08X} [2] = {:04X}",
                watch.addr,
                gba.bus.internal_read16(watch.addr)
            ),
            WatchSize::Word => println!(
                "watch {:08X} [4] = {:08X}",
                watch.addr,
                gba.bus.internal_read32(watch.addr)
            ),
        }
    }

    true
}

fn print_trace_status(debugger: &DebuggerState) {
    if debugger.trace_pcs.is_empty() {
        println!("trace PCs: none");
    } else {
        print!("trace PCs:");
        for pc in &debugger.trace_pcs {
            print!(" {:08X}", pc);
        }
        println!();
    }

    if debugger.watches.is_empty() {
        println!("watches: none");
    } else {
        println!("watches:");
        for watch in &debugger.watches {
            println!("  {:08X} size {}", watch.addr, watch.size.bytes());
        }
    }
}

impl WatchSize {
    fn bytes(self) -> u8 {
        match self {
            Self::Byte => 1,
            Self::Halfword => 2,
            Self::Word => 4,
        }
    }
}
