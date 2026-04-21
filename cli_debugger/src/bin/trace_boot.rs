use gba_core::Gba;

fn main() {
    let mut gba = Gba::new();

    let bios = std::fs::read("gba_bios.bin").expect("failed to read gba_bios.bin");
    gba.bus.load_bios(&bios);

    let rom_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "tests/roms/mario.gba".to_string());
    let rom = std::fs::read(&rom_path).expect("failed to read ROM");
    gba.bus.load_rom(&rom);

    println!("Tracing boot for {}", rom_path);

    let mut last_halted = gba.bus.halted;
    let mut last_ie = gba.bus.ie;
    let mut last_if = gba.bus.if_;
    let mut last_ime = gba.bus.ime;
    let mut last_dispstat = gba.bus.ppu.registers.dispstat;
    let mut in_bios_loop_count = 0u32;
    let mut entered_rom = false;
    let max_steps = 30_000_000u32;

    for step in 0..max_steps {
        let pc = gba.cpu.registers[15];
        let thumb = gba.cpu.get_thumb_mode();

        if pc == 0x0000_0018 {
            log_state("irq-entry", step, &mut gba);
        }

        if (0x0000_0B40..=0x0000_0C20).contains(&pc) {
            in_bios_loop_count += 1;
            if in_bios_loop_count <= 8 || in_bios_loop_count % 20_000 == 0 {
                log_state("bios-wait", step, &mut gba);
            }
        }

        if !entered_rom && (0x0800_0000..0x0E00_0000).contains(&pc) {
            entered_rom = true;
            log_state("entered-rom", step, &mut gba);
        }

        gba.step();

        if gba.bus.halted != last_halted {
            last_halted = gba.bus.halted;
            log_state("halt-change", step, &mut gba);
        }
        if gba.bus.ie != last_ie || gba.bus.if_ != last_if || gba.bus.ime != last_ime {
            last_ie = gba.bus.ie;
            last_if = gba.bus.if_;
            last_ime = gba.bus.ime;
            log_state("irq-reg-change", step, &mut gba);
        }
        if gba.bus.ppu.registers.dispstat != last_dispstat {
            let diff = gba.bus.ppu.registers.dispstat ^ last_dispstat;
            let vcount = gba.bus.ppu.registers.vcount;
            let near_vblank = (158..=162).contains(&vcount);
            if (diff & 0x0007) != 0 && near_vblank {
                last_dispstat = gba.bus.ppu.registers.dispstat;
                log_state("dispstat-change", step, &mut gba);
            } else {
                last_dispstat = gba.bus.ppu.registers.dispstat;
            }
        }

        if entered_rom && pc == gba.cpu.registers[15] && thumb == gba.cpu.get_thumb_mode() {
            log_state("no-forward-progress", step, &mut gba);
            break;
        }
    }

    if !entered_rom {
        println!("ROM entry not observed within {max_steps} instructions");
        log_state("final-state", max_steps, &mut gba);
    }
}

fn log_state(label: &str, step: u32, gba: &mut Gba) {
    let pc = gba.cpu.registers[15];
    let instr = if gba.cpu.get_thumb_mode() {
        format!("{:04X}", gba.bus.internal_read16(pc & !1))
    } else {
        format!("{:08X}", gba.bus.internal_read32(pc & !3))
    };
    println!(
        "[{label}] step={step} pc={pc:08X} instr={instr} thumb={} cpsr={:08X} halt={} ie={:04X} if={:04X} ime={} dispstat={:04X} vcount={} cycles={}",
        gba.cpu.get_thumb_mode(),
        gba.cpu.cpsr,
        gba.bus.halted,
        gba.bus.ie,
        gba.bus.if_,
        gba.bus.ime,
        gba.bus.ppu.registers.dispstat,
        gba.bus.ppu.registers.vcount,
        gba.bus.cycles
    );
}
