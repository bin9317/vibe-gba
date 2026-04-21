pub mod bus;
pub mod cpu;
pub mod dma;
pub mod eeprom;
pub mod ppu;
pub mod state;
pub mod timers;
pub mod timing;

pub const BIOS_IRQ_VECTOR: u32 = 0x0000_0018;
pub const GAMEPAK_ROM_START: u32 = 0x0800_0000;
pub const GAMEPAK_ROM_END: u32 = 0x0E00_0000;
pub const GBA_CYCLES_PER_FRAME: u64 = 280_896;

pub struct Gba {
    pub cpu: cpu::Cpu,
    pub bus: bus::Bus,
}

impl Gba {
    pub fn new() -> Self {
        Self {
            cpu: cpu::Cpu::new(),
            bus: bus::Bus::new(),
        }
    }

    pub fn step(&mut self) {
        let has_interrupt = (self.bus.ie & self.bus.if_) != 0;
        if has_interrupt {
            self.bus.halted = false;
            let interrupts_enabled = (self.cpu.cpsr & 0x80) == 0;
            if interrupts_enabled && (self.bus.ime & 1) != 0 {
                self.cpu_irq();
            }
        }

        if !self.bus.halted {
            self.cpu.step(&mut self.bus);
        } else {
            self.bus.clock(1);
        }
    }

    fn cpu_irq(&mut self) {
        trace_irq_event(
            "enter",
            &self.bus,
            self.cpu.registers[15],
            self.bus.ie,
            self.bus.if_,
            self.bus.ime,
        );
        self.cpu.spsr_irq = self.cpu.cpsr;
        self.cpu.set_cpsr((self.cpu.cpsr & !0x3F) | 0x12 | 0x80);
        self.cpu.registers[14] = self.cpu.registers[15].wrapping_add(4);

        // IRQ entry: 2S + 1N (refill pipeline at vector 0x18)
        let s_cycle = self.bus.get_access_time(BIOS_IRQ_VECTOR, true, true);
        self.bus.clock(s_cycle * 2);

        self.cpu.registers[15] = BIOS_IRQ_VECTOR;
        self.bus.invalidate_gamepak_prefetch();
        self.bus.note_control_flow_break();
        self.cpu.invalidate_pipeline();
        trace_irq_event(
            "vector",
            &self.bus,
            self.cpu.registers[15],
            self.bus.ie,
            self.bus.if_,
            self.bus.ime,
        );
    }
}

fn trace_irq_event(stage: &str, bus: &bus::Bus, pc: u32, ie: u16, if_: u16, ime: u32) {
    use std::sync::OnceLock;

    static TRACE: OnceLock<bool> = OnceLock::new();
    if !*TRACE.get_or_init(|| std::env::var_os("VIBE_TRACE_IRQ").is_some()) {
        return;
    }

    eprintln!(
        "[irq] stage={stage} pc={pc:08X} scanline={} line_cycle={} halt={} ie={ie:04X} if={if_:04X} ime={ime}",
        bus.ppu.current_scanline, bus.ppu.cycles, bus.halted,
    );
}
