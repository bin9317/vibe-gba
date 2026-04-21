use gba_core::ppu::{
    DISPCNT_BG0_EN, DISPCNT_BG1_EN, DISPCNT_BG2_EN, DISPCNT_BG3_EN, DISPCNT_FORCED_BLANK,
    DISPCNT_OBJWIN_EN, DISPCNT_OBJ_EN, DISPCNT_WIN0_EN, DISPCNT_WIN1_EN,
};
use gba_core::{Gba, GBA_CYCLES_PER_FRAME};
use std::fs;
use std::path::Path;
use std::process::Command;

const SCREEN_WIDTH: usize = 240;
const SCREEN_HEIGHT: usize = 160;
const BYTES_PER_PIXEL: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Bmp,
    Png,
}

impl ImageFormat {
    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref()
        {
            Some("bmp") => Self::Bmp,
            _ => Self::Png,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Bmp => "bmp",
            Self::Png => "png",
        }
    }
}

pub fn load_gba(rom_path: &str, skip_bios: bool) -> Result<Gba, String> {
    load_gba_with_bios(rom_path, None, skip_bios)
}

pub fn load_gba_with_state(
    rom_path: &str,
    bios_path: Option<&str>,
    state_path: &str,
) -> Result<Gba, String> {
    let mut gba = load_gba_with_bios(rom_path, bios_path, false)?;
    let bytes =
        fs::read(state_path).map_err(|err| format!("failed to read state {state_path}: {err}"))?;
    gba.load_state(&bytes)
        .map_err(|err| format!("failed to load state {state_path}: {err}"))?;
    Ok(gba)
}

pub fn load_gba_with_bios(
    rom_path: &str,
    bios_path: Option<&str>,
    skip_bios: bool,
) -> Result<Gba, String> {
    let mut gba = Gba::new();

    if let Some(path) = bios_path {
        let bios = fs::read(path).map_err(|err| format!("failed to read BIOS {path}: {err}"))?;
        gba.bus.load_bios(&bios);
    } else if !skip_bios {
        let bios = fs::read("gba_bios.bin")
            .or_else(|_| fs::read("../gba_bios.bin"))
            .map_err(|_| "failed to read gba_bios.bin".to_string())?;
        gba.bus.load_bios(&bios);
    }

    let rom = fs::read(rom_path).map_err(|err| format!("failed to read ROM {rom_path}: {err}"))?;
    gba.bus.load_rom(&rom);

    if skip_bios {
        gba.cpu.skip_bios(&mut gba.bus);
    }

    Ok(gba)
}

pub fn step_frames(gba: &mut Gba, frames: u32) {
    for _ in 0..frames {
        let start_cycles = gba.bus.cycles;
        while (gba.bus.cycles - start_cycles) < GBA_CYCLES_PER_FRAME {
            gba.step();
        }
    }
}

pub fn write_frame_image(path: &Path, rgba: &[u8]) -> std::io::Result<()> {
    let format = ImageFormat::from_path(path);
    match format {
        ImageFormat::Bmp => fs::write(path, encode_bmp(rgba)),
        ImageFormat::Png => write_png_via_sips(path, rgba),
    }
}

pub fn diff_pixels(previous: &[u8], current: &[u8]) -> usize {
    previous
        .chunks_exact(BYTES_PER_PIXEL)
        .zip(current.chunks_exact(BYTES_PER_PIXEL))
        .filter(|(a, b)| a != b)
        .count()
}

pub fn print_ppu_summary(gba: &Gba) {
    let regs = &gba.bus.ppu.registers;
    let dispcnt = regs.dispcnt;
    println!(
        "PPU: scanline={} dispcnt={:04X} dispstat={:04X} vcount={}",
        gba.bus.ppu.current_scanline, dispcnt, regs.dispstat, regs.vcount
    );
    println!(
        "  mode={} forced_blank={} obj_1d={} bg=[{} {} {} {}] obj={} win0={} win1={} objwin={}",
        dispcnt & 0x7,
        bit(dispcnt, DISPCNT_FORCED_BLANK),
        bit(dispcnt, 0x40),
        bit(dispcnt, DISPCNT_BG0_EN),
        bit(dispcnt, DISPCNT_BG1_EN),
        bit(dispcnt, DISPCNT_BG2_EN),
        bit(dispcnt, DISPCNT_BG3_EN),
        bit(dispcnt, DISPCNT_OBJ_EN),
        bit(dispcnt, DISPCNT_WIN0_EN),
        bit(dispcnt, DISPCNT_WIN1_EN),
        bit(dispcnt, DISPCNT_OBJWIN_EN),
    );
    print_bg_summary("BG0", regs.bg0cnt, regs.bg0hofs, regs.bg0vofs);
    print_bg_summary("BG1", regs.bg1cnt, regs.bg1hofs, regs.bg1vofs);
    print_bg_summary("BG2", regs.bg2cnt, regs.bg2hofs, regs.bg2vofs);
    print_bg_summary("BG3", regs.bg3cnt, regs.bg3hofs, regs.bg3vofs);
    println!(
        "  blend: bldcnt={:04X} bldalpha={:04X} bldy={:04X}",
        regs.bldcnt, regs.bldalpha, regs.bldy
    );
    println!(
        "  window: win0h={:04X} win1h={:04X} win0v={:04X} win1v={:04X} winin={:04X} winout={:04X}",
        regs.win0h, regs.win1h, regs.win0v, regs.win1v, regs.winin, regs.winout
    );
    println!(
        "  affine: bg2x={} bg2y={} pa={} pb={} pc={} pd={}",
        regs.bg2x, regs.bg2y, regs.bg2pa, regs.bg2pb, regs.bg2pc, regs.bg2pd
    );
    println!(
        "          bg3x={} bg3y={} pa={} pb={} pc={} pd={}",
        regs.bg3x, regs.bg3y, regs.bg3pa, regs.bg3pb, regs.bg3pc, regs.bg3pd
    );
}

pub fn print_dma_summary(gba: &Gba) {
    println!(
        "DMA: pending_dma={:04X} dma_running={}",
        gba.bus.pending_dma, gba.bus.dma_running
    );
    for (index, channel) in gba.bus.dma.channels.iter().enumerate() {
        let enabled = (channel.cnt & 0x8000) != 0;
        let timing = match (channel.cnt >> 12) & 0x3 {
            0 => "immediate",
            1 => "vblank",
            2 => "hblank",
            3 => {
                if index == 3 {
                    "video-capture/special"
                } else {
                    "special"
                }
            }
            _ => unreachable!(),
        };
        let src_ctrl = match (channel.cnt >> 7) & 0x3 {
            0 => "inc",
            1 => "dec",
            2 => "fixed",
            3 => "prohibited",
            _ => unreachable!(),
        };
        let dst_ctrl = match (channel.cnt >> 5) & 0x3 {
            0 => "inc",
            1 => "dec",
            2 => "fixed",
            3 => "reload",
            _ => unreachable!(),
        };
        println!(
            "  DMA{}: en={} timing={} width={} repeat={} irq={} src_ctrl={} dst_ctrl={} src={:08X} dst={:08X} count={} internal_src={:08X} internal_dst={:08X} internal_count={}",
            index,
            enabled,
            timing,
            if (channel.cnt & 0x0400) != 0 { 32 } else { 16 },
            (channel.cnt & 0x0200) != 0,
            (channel.cnt & 0x4000) != 0,
            src_ctrl,
            dst_ctrl,
            channel.src,
            channel.dst,
            channel.count,
            channel.internal_src,
            channel.internal_dst,
            channel.internal_count,
        );
    }
}

fn bit(value: u16, mask: u16) -> bool {
    (value & mask) != 0
}

fn print_bg_summary(label: &str, bgcnt: u16, hofs: u16, vofs: u16) {
    println!(
        "  {}: prio={} char_base={} screen_base={} mosaic={} bpp={} size={} hofs={} vofs={}",
        label,
        bgcnt & 0x3,
        (bgcnt >> 2) & 0x3,
        (bgcnt >> 8) & 0x1F,
        (bgcnt & 0x40) != 0,
        if (bgcnt & 0x80) != 0 { 8 } else { 4 },
        (bgcnt >> 14) & 0x3,
        hofs,
        vofs
    );
}

fn encode_bmp(rgba: &[u8]) -> Vec<u8> {
    let pixel_data_len = SCREEN_WIDTH * SCREEN_HEIGHT * BYTES_PER_PIXEL;
    let file_size = 14 + 40 + pixel_data_len;
    let mut out = Vec::with_capacity(file_size);

    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(14u32 + 40u32).to_le_bytes());

    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(SCREEN_WIDTH as i32).to_le_bytes());
    out.extend_from_slice(&(-(SCREEN_HEIGHT as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(pixel_data_len as u32).to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    for pixel in rgba.chunks_exact(BYTES_PER_PIXEL) {
        out.push(pixel[2]);
        out.push(pixel[1]);
        out.push(pixel[0]);
        out.push(pixel[3]);
    }

    out
}

fn write_png_via_sips(path: &Path, rgba: &[u8]) -> std::io::Result<()> {
    let temp_bmp = path.with_extension("bmp");
    fs::write(&temp_bmp, encode_bmp(rgba))?;

    let status = Command::new("sips")
        .args(["-s", "format", "png"])
        .arg(&temp_bmp)
        .arg("--out")
        .arg(path)
        .status()?;

    let _ = fs::remove_file(&temp_bmp);

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("sips failed to convert BMP to PNG"))
    }
}
