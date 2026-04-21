pub mod io;
pub mod registers;

use registers::PpuRegisters;
use std::sync::OnceLock;

pub const SCREEN_WIDTH: usize = 240;
pub const SCREEN_HEIGHT: usize = 160;
const SCANLINE_CYCLES: u32 = 1232;
const HBLANK_START_CYCLES: u32 = 960;

pub const DISPCNT_MODE_MASK: u16 = 0x7;
pub const DISPCNT_PAGE: u16 = 0x10;
pub const DISPCNT_HBLANK_FREE: u16 = 0x20;
pub const DISPCNT_OBJ_1D: u16 = 0x40;
pub const DISPCNT_FORCED_BLANK: u16 = 0x80;
pub const DISPCNT_BG0_EN: u16 = 0x100;
pub const DISPCNT_BG1_EN: u16 = 0x200;
pub const DISPCNT_BG2_EN: u16 = 0x400;
pub const DISPCNT_BG3_EN: u16 = 0x800;
pub const DISPCNT_OBJ_EN: u16 = 0x1000;
pub const DISPCNT_WIN0_EN: u16 = 0x2000;
pub const DISPCNT_WIN1_EN: u16 = 0x4000;
pub const DISPCNT_OBJWIN_EN: u16 = 0x8000;

const WIN_0: u8 = 1;
const WIN_1: u8 = 2;
const WIN_OBJ: u8 = 3;
const WIN_OUT: u8 = 4;

#[derive(Clone, Copy, PartialEq)]
enum Layer {
    BG0 = 0,
    BG1 = 1,
    BG2 = 2,
    BG3 = 3,
    OBJ = 4,
    Backdrop = 5,
}

impl Layer {
    fn debug_mask(self) -> u8 {
        match self {
            Layer::BG0 => 0x01,
            Layer::BG1 => 0x02,
            Layer::BG2 => 0x04,
            Layer::BG3 => 0x08,
            Layer::OBJ => 0x10,
            Layer::Backdrop => 0x20,
        }
    }
}

pub struct Ppu {
    pub registers: PpuRegisters,
    render_registers: PpuRegisters,
    current_bg2x: i32,
    current_bg2y: i32,
    current_bg3x: i32,
    current_bg3y: i32,
    bg2x_written_this_line: bool,
    bg2y_written_this_line: bool,
    bg3x_written_this_line: bool,
    bg3y_written_this_line: bool,
    pub frame_buffer: Box<[u8; SCREEN_WIDTH * SCREEN_HEIGHT * 4]>,
    pub current_scanline: u16,
    pub(crate) cycles: u32,
    pub(crate) scanline_rendered: bool,

    // Per-scanline visible layer caches
    scanline_layers: [[Option<u16>; SCREEN_WIDTH]; 4],
    scanline_obj: [Option<(u16, u8, bool)>; SCREEN_WIDTH], // color, priority, semi-transparent
    scanline_backdrop: [u16; SCREEN_WIDTH],
    scanline_windows: [u8; SCREEN_WIDTH], // 0=None, 1=Win0, 2=Win1, 3=ObjWin, 4=Out
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            registers: PpuRegisters::new(),
            render_registers: PpuRegisters::new(),
            current_bg2x: 0,
            current_bg2y: 0,
            current_bg3x: 0,
            current_bg3y: 0,
            bg2x_written_this_line: false,
            bg2y_written_this_line: false,
            bg3x_written_this_line: false,
            bg3y_written_this_line: false,
            frame_buffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT * 4]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
            current_scanline: 0,
            cycles: 0,
            scanline_rendered: false,
            scanline_layers: [[None; SCREEN_WIDTH]; 4],
            scanline_obj: [None; SCREEN_WIDTH],
            scanline_backdrop: [0; SCREEN_WIDTH],
            scanline_windows: [0; SCREEN_WIDTH],
        }
    }

    pub fn step(
        &mut self,
        cycles: u32,
        vram: &[u8],
        palette_ram: &[u8],
        oam: &[u8],
    ) -> (bool, bool, bool) {
        // Visible-period register state is latched at the start of each scanline.
        // Mid-scanline writes affect future lines, but do not retroactively change
        // the line that is already being drawn.
        if !self.scanline_rendered && self.cycles == 0 {
            self.snapshot_render_registers();
        }
        self.cycles += cycles;
        let mut vblank_triggered = false;
        let mut hblank_triggered = false;
        let mut vmatch_triggered = false;

        loop {
            if !self.scanline_rendered && self.cycles >= HBLANK_START_CYCLES {
                self.scanline_rendered = true;

                // Render visible lines (0-159) when HBlank begins, using the
                // register state from the just-finished draw period.
                if self.current_scanline < 160 {
                    self.render_scanline(vram, palette_ram, oam);
                }
            }

            if self.cycles < SCANLINE_CYCLES {
                break;
            }

            let finished_scanline = self.current_scanline;
            self.cycles -= SCANLINE_CYCLES;
            self.scanline_rendered = false;

            // Affine internal references advance at the scanline boundary, after
            // HBlank writes for the finished line have had a chance to update PB/PD.
            if finished_scanline < 160 {
                if !self.bg2x_written_this_line {
                    self.current_bg2x = self.current_bg2x.wrapping_add(self.registers.bg2pb as i32);
                }
                if !self.bg2y_written_this_line {
                    self.current_bg2y = self.current_bg2y.wrapping_add(self.registers.bg2pd as i32);
                }
                if !self.bg3x_written_this_line {
                    self.current_bg3x = self.current_bg3x.wrapping_add(self.registers.bg3pb as i32);
                }
                if !self.bg3y_written_this_line {
                    self.current_bg3y = self.current_bg3y.wrapping_add(self.registers.bg3pd as i32);
                }
            }
            self.bg2x_written_this_line = false;
            self.bg2y_written_this_line = false;
            self.bg3x_written_this_line = false;
            self.bg3y_written_this_line = false;

            // Advance to the next scanline after the current line fully ends.
            self.current_scanline += 1;
            if self.current_scanline >= 228 {
                self.current_scanline = 0;

                // Reload Affine Latches at the start of frame (line 0)
                self.current_bg2x = self.registers.bg2x_latch;
                self.current_bg2y = self.registers.bg2y_latch;
                self.current_bg3x = self.registers.bg3x_latch;
                self.current_bg3y = self.registers.bg3y_latch;
            }
            self.registers.vcount = self.current_scanline;
            self.snapshot_render_registers();

            let vcount_setting = (self.registers.dispstat >> 8) as u16;
            if self.current_scanline == vcount_setting {
                if (self.registers.dispstat & 4) == 0 {
                    vmatch_triggered = true;
                }
                self.registers.dispstat |= 4;
            } else {
                self.registers.dispstat &= !4;
            }
        }

        // VBlank flag should be set for scanlines 160-227
        if self.current_scanline >= 160 {
            if (self.registers.dispstat & 1) == 0 {
                vblank_triggered = true;
            }
            self.registers.dispstat |= 1;
        } else {
            self.registers.dispstat &= !1;
        }

        // HBlank set at 960 cycles into the current scanline
        if self.cycles >= HBLANK_START_CYCLES {
            if (self.registers.dispstat & 2) == 0 {
                hblank_triggered = true;
            }
            self.registers.dispstat |= 2;
        } else {
            self.registers.dispstat &= !2;
        }

        (vblank_triggered, hblank_triggered, vmatch_triggered)
    }

    fn render_scanline(&mut self, vram: &[u8], palette_ram: &[u8], oam: &[u8]) {
        let y = self.current_scanline as usize;
        trace_scanline_state(self, y);

        // 0. Render logic (only if not in Forced Blank)
        if (self.render_registers.dispcnt & DISPCNT_FORCED_BLANK) == 0 {
            // 1. Calculate Windows for this scanline
            self.calculate_windows();

            // 2. Clear caches
            for bg in 0..4 {
                self.scanline_layers[bg] = [None; SCREEN_WIDTH];
            }
            self.scanline_obj = [None; SCREEN_WIDTH];

            let backdrop = (palette_ram[0] as u16) | ((palette_ram[1] as u16) << 8);
            self.scanline_backdrop = [backdrop; SCREEN_WIDTH];

            // 3. Fill layers based on mode
            let mode = self.render_registers.dispcnt & 0x7;
            match mode {
                0 => {
                    for bg in 0..4 {
                        self.fill_text_bg(bg, vram, palette_ram);
                    }
                }
                1 => {
                    self.fill_text_bg(0, vram, palette_ram);
                    self.fill_text_bg(1, vram, palette_ram);
                    self.fill_affine_bg(2, vram, palette_ram);
                }
                2 => {
                    self.fill_affine_bg(2, vram, palette_ram);
                    self.fill_affine_bg(3, vram, palette_ram);
                }
                3 => self.fill_mode3(vram),
                4 => self.fill_mode4(vram, palette_ram),
                5 => self.fill_mode5(vram),
                _ => {}
            }

            self.fill_sprites(vram, palette_ram, oam);

            // 4. Composite final pixels
            for x in 0..SCREEN_WIDTH {
                let color = self.get_top_pixel(x);
                self.draw_pixel_raw(x, y, color);
            }
        } else {
            // White screen during Forced Blank
            for x in 0..SCREEN_WIDTH {
                self.draw_pixel_raw(x, y, 0x7FFF);
            }
        }
    }

    fn snapshot_render_registers(&mut self) {
        self.render_registers = self.registers.clone();
        self.render_registers.bg2x = self.current_bg2x;
        self.render_registers.bg2y = self.current_bg2y;
        self.render_registers.bg3x = self.current_bg3x;
        self.render_registers.bg3y = self.current_bg3y;
    }

    pub fn sync_internal_affine_reference(&mut self, offset: u32) {
        match offset {
            0x0028..=0x002B => {
                self.current_bg2x = self.registers.bg2x;
                self.bg2x_written_this_line = true;
            }
            0x002C..=0x002F => {
                self.current_bg2y = self.registers.bg2y;
                self.bg2y_written_this_line = true;
            }
            0x0038..=0x003B => {
                self.current_bg3x = self.registers.bg3x;
                self.bg3x_written_this_line = true;
            }
            0x003C..=0x003F => {
                self.current_bg3y = self.registers.bg3y;
                self.bg3y_written_this_line = true;
            }
            _ => {}
        }
    }

    fn calculate_windows(&mut self) {
        let y = self.current_scanline as usize;
        let win0_en = (self.render_registers.dispcnt & DISPCNT_WIN0_EN) != 0;
        let win1_en = (self.render_registers.dispcnt & DISPCNT_WIN1_EN) != 0;

        for x in 0..SCREEN_WIDTH {
            let mut win_idx = WIN_OUT;

            if win0_en {
                let x1 = (self.render_registers.win0h >> 8) as usize;
                let x2 = (self.render_registers.win0h & 0xFF) as usize;
                let y1 = (self.render_registers.win0v >> 8) as usize;
                let y2 = (self.render_registers.win0v & 0xFF) as usize;

                let in_x = if x1 <= x2 {
                    x >= x1 && x < x2
                } else {
                    x >= x1 || x < x2
                };
                let in_y = window_contains_scanline(y, y1, y2);
                if in_x && in_y {
                    win_idx = WIN_0;
                }
            }

            if win_idx == WIN_OUT && win1_en {
                let x1 = (self.render_registers.win1h >> 8) as usize;
                let x2 = (self.render_registers.win1h & 0xFF) as usize;
                let y1 = (self.render_registers.win1v >> 8) as usize;
                let y2 = (self.render_registers.win1v & 0xFF) as usize;

                let in_x = if x1 <= x2 {
                    x >= x1 && x < x2
                } else {
                    x >= x1 || x < x2
                };
                let in_y = window_contains_scanline(y, y1, y2);
                if in_x && in_y {
                    win_idx = WIN_1;
                }
            }

            self.scanline_windows[x] = win_idx;
        }
    }

    fn get_top_pixel(&self, x: usize) -> u16 {
        let layer_mask = debug_visible_layer_mask();
        let any_win_en = (self.render_registers.dispcnt
            & (DISPCNT_WIN0_EN | DISPCNT_WIN1_EN | DISPCNT_OBJWIN_EN))
            != 0;
        let objwin_en = (self.render_registers.dispcnt & DISPCNT_OBJWIN_EN) != 0;
        let win_cnt = if any_win_en {
            let win_idx = self.scanline_windows[x];
            match win_idx {
                WIN_0 => self.render_registers.winin & 0xFF,
                WIN_1 => self.render_registers.winin >> 8,
                WIN_OBJ if objwin_en => self.render_registers.winout >> 8,
                _ => self.render_registers.winout & 0xFF,
            }
        } else {
            0x3F // All layers visible if no windows enabled
        };

        // Find top two visible layers for blending
        let mut first: Option<(Layer, u16)> = None;
        let mut second: Option<(Layer, u16)> = None;

        for priority in 0..4 {
            // OBJ wins over BGs at the same priority level on GBA hardware.
            if let Some((color, prio, is_semi)) = self.scanline_obj[x] {
                if prio == priority as u8
                    && (win_cnt & 0x10) != 0
                    && (layer_mask & Layer::OBJ.debug_mask()) != 0
                {
                    if first.is_none() {
                        first = Some((Layer::OBJ, color));
                    } else if second.is_none() {
                        second = Some((Layer::OBJ, color));
                    }
                }
                let _ = is_semi;
            }

            // BG0 has highest priority among BGs with the same priority value.
            for bg in 0..4 {
                if (self.render_registers.dispcnt & (1 << (8 + bg))) != 0 {
                    let bgcnt = self.get_render_bgcnt(bg);
                    if (bgcnt & 0x3) == priority as u16 && (win_cnt & (1 << bg)) != 0 {
                        if let Some(color) = self.scanline_layers[bg][x] {
                            let layer = match bg {
                                0 => Layer::BG0,
                                1 => Layer::BG1,
                                2 => Layer::BG2,
                                _ => Layer::BG3,
                            };
                            if (layer_mask & layer.debug_mask()) == 0 {
                                continue;
                            }
                            if first.is_none() {
                                first = Some((layer, color));
                            } else if second.is_none() {
                                second = Some((layer, color));
                            }
                        }
                    }
                }
            }

            if second.is_some() {
                break;
            }
        }
        let backdrop = if (layer_mask & Layer::Backdrop.debug_mask()) != 0 {
            (Layer::Backdrop, self.scanline_backdrop[x])
        } else {
            (Layer::Backdrop, 0)
        };
        let (layer1, color1) = first.unwrap_or(backdrop);

        // Target 1 check for effects
        let mut is_target1 = (self.render_registers.bldcnt & (1 << (layer1 as u16))) != 0;
        let mut effect = (self.render_registers.bldcnt >> 6) & 0x3;

        // Special case: Semi-transparent OBJ always acts as Target 1 for Alpha Blending
        if let Some((_, _, true)) = self.scanline_obj[x] {
            if layer1 == Layer::OBJ {
                is_target1 = true;
                effect = 1;
            }
        }

        if is_target1 && (win_cnt & 0x20) != 0 {
            match effect {
                1 => {
                    // Alpha Blending
                    if second.is_none() {
                        second = Some((Layer::Backdrop, self.scanline_backdrop[x]));
                    }
                    let (layer2, color2) = second.unwrap();

                    // For forced Alpha Blending (semi-transparent OBJ), we still check if layer2 is Target 2
                    if (self.render_registers.bldcnt & (1 << (8 + layer2 as u16))) != 0 {
                        let eva = (self.render_registers.bldalpha & 0x1F).min(16) as u32;
                        let evb = ((self.render_registers.bldalpha >> 8) & 0x1F).min(16) as u32;
                        return self.blend(color1, color2, eva, evb);
                    }
                }
                2 => {
                    // Brighten
                    let evy = (self.render_registers.bldy & 0x1F).min(16) as u32;
                    return self.apply_brightness(color1, evy, true);
                }
                3 => {
                    // Darken
                    let evy = (self.render_registers.bldy & 0x1F).min(16) as u32;
                    return self.apply_brightness(color1, evy, false);
                }
                _ => {}
            }
        }

        color1
    }

    fn apply_brightness(&self, color: u16, evy: u32, brighten: bool) -> u16 {
        if evy == 0 {
            return color;
        }
        let r = (color & 0x1F) as u32;
        let g = ((color >> 5) & 0x1F) as u32;
        let b = ((color >> 10) & 0x1F) as u32;

        let (nr, ng, nb) = if brighten {
            (
                (r * (16 - evy) + 31 * evy) / 16,
                (g * (16 - evy) + 31 * evy) / 16,
                (b * (16 - evy) + 31 * evy) / 16,
            )
        } else {
            (
                (r * (16 - evy)) / 16,
                (g * (16 - evy)) / 16,
                (b * (16 - evy)) / 16,
            )
        };

        (nr.min(31) as u16) | ((ng.min(31) as u16) << 5) | ((nb.min(31) as u16) << 10)
    }

    fn blend(&self, c1: u16, c2: u16, eva: u32, evb: u32) -> u16 {
        let r1 = (c1 & 0x1F) as u32;
        let g1 = ((c1 >> 5) & 0x1F) as u32;
        let b1 = ((c1 >> 10) & 0x1F) as u32;
        let r2 = (c2 & 0x1F) as u32;
        let g2 = ((c2 >> 5) & 0x1F) as u32;
        let b2 = ((c2 >> 10) & 0x1F) as u32;
        let r = ((r1 * eva + r2 * evb) / 16).min(31);
        let g = ((g1 * eva + g2 * evb) / 16).min(31);
        let b = ((b1 * eva + b2 * evb) / 16).min(31);
        (r as u16) | ((g as u16) << 5) | ((b as u16) << 10)
    }

    fn get_render_bgcnt(&self, bg_idx: usize) -> u16 {
        match bg_idx {
            0 => self.render_registers.bg0cnt,
            1 => self.render_registers.bg1cnt,
            2 => self.render_registers.bg2cnt,
            3 => self.render_registers.bg3cnt,
            _ => 0,
        }
    }

    fn fill_text_bg(&mut self, bg_idx: usize, vram: &[u8], palette_ram: &[u8]) {
        if (self.render_registers.dispcnt & (1 << (8 + bg_idx))) == 0 {
            return;
        }
        let y = self.current_scanline as usize;
        let bgcnt = self.get_render_bgcnt(bg_idx);

        // Mosaic calculation for BG
        let (mos_h, mos_v) = if (bgcnt & 0x40) != 0 {
            (
                ((self.render_registers.mosaic & 0xF) + 1) as usize,
                (((self.render_registers.mosaic >> 4) & 0xF) + 1) as usize,
            )
        } else {
            (1, 1)
        };
        let render_y = (y / mos_v) * mos_v;

        let hofs = self.get_render_hofs(bg_idx) as usize;
        let vofs = self.get_render_vofs(bg_idx) as usize;
        let char_base = ((bgcnt >> 2) & 0x3) as usize * 16384;
        let screen_base = ((bgcnt >> 8) & 0x1F) as usize * 2048;
        let is_8bpp = (bgcnt & 0x80) != 0;
        let screen_size = (bgcnt >> 14) & 0x3;
        let (bg_width, bg_height) = text_bg_dimensions(screen_size);
        let scrolled_y = (render_y + vofs) % bg_height;

        for x in 0..SCREEN_WIDTH {
            let render_x = (x / mos_h) * mos_h;
            let scrolled_x = (render_x + hofs) % bg_width;
            let (tx, ty, block_offset) = match screen_size {
                0 => (scrolled_x / 8, scrolled_y / 8, 0),
                1 => (
                    (scrolled_x % 256) / 8,
                    scrolled_y / 8,
                    ((scrolled_x / 256) % 2) * 2048,
                ),
                2 => (
                    scrolled_x / 8,
                    (scrolled_y % 256) / 8,
                    ((scrolled_y / 256) % 2) * 2048,
                ),
                3 => (
                    (scrolled_x % 256) / 8,
                    (scrolled_y % 256) / 8,
                    (((scrolled_y / 256) % 2) * 2 + ((scrolled_x / 256) % 2)) * 2048,
                ),
                _ => (0, 0, 0),
            };
            let tile_offset = block_offset + (ty * 32 + tx) * 2;
            if screen_base + tile_offset + 1 >= vram.len() {
                continue;
            }
            let tile_info = (vram[screen_base + tile_offset] as u16)
                | ((vram[screen_base + tile_offset + 1] as u16) << 8);
            let tile_idx = (tile_info & 0x3FF) as usize;
            let flip_h = (tile_info & 0x0400) != 0;
            let flip_v = (tile_info & 0x0800) != 0;
            let palette_idx = ((tile_info & 0xF000) >> 12) as usize;
            let px = if flip_h {
                7 - (scrolled_x % 8)
            } else {
                scrolled_x % 8
            };
            let py = if flip_v {
                7 - (scrolled_y % 8)
            } else {
                scrolled_y % 8
            };
            let color_idx = if is_8bpp {
                let addr = char_base + tile_idx * 64 + py * 8 + px;
                if addr < vram.len() {
                    vram[addr] as usize
                } else {
                    0
                }
            } else {
                let addr = char_base + tile_idx * 32 + py * 4 + px / 2;
                if addr < vram.len() {
                    let pixel_data = vram[addr];
                    if px % 2 == 0 {
                        (pixel_data & 0xF) as usize
                    } else {
                        (pixel_data >> 4) as usize
                    }
                } else {
                    0
                }
            };
            if color_idx != 0 {
                let palette_offset = if is_8bpp {
                    color_idx * 2
                } else {
                    (palette_idx * 16 + color_idx) * 2
                };
                if palette_offset + 1 < palette_ram.len() {
                    let color = (palette_ram[palette_offset] as u16)
                        | ((palette_ram[palette_offset + 1] as u16) << 8);
                    self.scanline_layers[bg_idx][x] = Some(color);
                }
            }
        }
    }

    fn fill_affine_bg(&mut self, bg_idx: usize, vram: &[u8], palette_ram: &[u8]) {
        if (self.render_registers.dispcnt & (1 << (8 + bg_idx))) == 0 {
            return;
        }
        let y = self.current_scanline as usize;
        let bgcnt = self.get_render_bgcnt(bg_idx);

        // Mosaic calculation for Affine BG
        let (mos_h, mos_v) = if (bgcnt & 0x40) != 0 {
            (
                ((self.render_registers.mosaic & 0xF) + 1) as usize,
                (((self.render_registers.mosaic >> 4) & 0xF) + 1) as usize,
            )
        } else {
            (1, 1)
        };
        let render_y = (y / mos_v) * mos_v;

        let char_base = ((bgcnt >> 2) & 0x3) as usize * 16384;
        let screen_base = ((bgcnt >> 8) & 0x1F) as usize * 2048;
        let screen_size = (bgcnt >> 14) & 0x3;
        let (pa, pb, pc, pd, x_ref, y_ref) = if bg_idx == 2 {
            (
                self.render_registers.bg2pa,
                self.render_registers.bg2pb,
                self.render_registers.bg2pc,
                self.render_registers.bg2pd,
                self.render_registers.bg2x,
                self.render_registers.bg2y,
            )
        } else {
            (
                self.render_registers.bg3pa,
                self.render_registers.bg3pb,
                self.render_registers.bg3pc,
                self.render_registers.bg3pd,
                self.render_registers.bg3x,
                self.render_registers.bg3y,
            )
        };
        let size = 128 << screen_size;
        let line_delta_x = (render_y as i32 - y as i32) * pb as i32;
        let line_delta_y = (render_y as i32 - y as i32) * pd as i32;
        for x in 0..SCREEN_WIDTH {
            let render_x = (x / mos_h) * mos_h;
            let curr_x = x_ref + line_delta_x + (render_x as i32) * (pa as i32);
            let curr_y = y_ref + line_delta_y + (render_x as i32) * (pc as i32);

            let mut tx = curr_x >> 8;
            let mut ty = curr_y >> 8;
            let wrap = (bgcnt & 0x2000) != 0;
            if wrap {
                tx = tx.rem_euclid(size as i32);
                ty = ty.rem_euclid(size as i32);
            }
            if tx >= 0 && tx < size as i32 && ty >= 0 && ty < size as i32 {
                let tile_map_width = size / 8;
                let map_addr = screen_base + (ty / 8 * tile_map_width as i32 + tx / 8) as usize;
                if map_addr >= vram.len() {
                    trace_affine_sample(
                        bg_idx, y, x, curr_x, curr_y, tx, ty, None, None, None, None,
                    );
                    continue;
                }
                let tile_idx = vram[map_addr] as usize;
                let tile_addr = char_base + tile_idx * 64 + (ty % 8 * 8 + tx % 8) as usize;
                if tile_addr >= vram.len() {
                    trace_affine_sample(
                        bg_idx,
                        y,
                        x,
                        curr_x,
                        curr_y,
                        tx,
                        ty,
                        Some(map_addr),
                        Some(tile_idx),
                        None,
                        None,
                    );
                    continue;
                }
                let color_idx = vram[tile_addr] as usize;
                trace_affine_sample(
                    bg_idx,
                    y,
                    x,
                    curr_x,
                    curr_y,
                    tx,
                    ty,
                    Some(map_addr),
                    Some(tile_idx),
                    Some(tile_addr),
                    Some(color_idx),
                );
                if color_idx != 0 {
                    let palette_addr = color_idx * 2;
                    if palette_addr + 1 < palette_ram.len() {
                        let color = (palette_ram[palette_addr] as u16)
                            | ((palette_ram[palette_addr + 1] as u16) << 8);
                        self.scanline_layers[bg_idx][x] = Some(color);
                    }
                }
            }
        }
    }

    fn fill_mode3(&mut self, vram: &[u8]) {
        if (self.render_registers.dispcnt & DISPCNT_BG2_EN) == 0 {
            return;
        }
        let y = self.current_scanline as usize;
        for x in 0..SCREEN_WIDTH {
            let offset = (y * SCREEN_WIDTH + x) * 2;
            let color = (vram[offset] as u16) | ((vram[offset + 1] as u16) << 8);
            self.scanline_layers[Layer::BG2 as usize][x] = Some(color);
        }
    }

    fn fill_mode4(&mut self, vram: &[u8], palette_ram: &[u8]) {
        if (self.render_registers.dispcnt & DISPCNT_BG2_EN) == 0 {
            return;
        }
        let y = self.current_scanline as usize;
        let page_offset = if (self.render_registers.dispcnt & 0x0010) != 0 {
            0xA000
        } else {
            0
        };
        for x in 0..SCREEN_WIDTH {
            let color_idx = vram[page_offset + y * SCREEN_WIDTH + x] as usize;
            let palette_offset = color_idx * 2;
            if palette_offset + 1 < palette_ram.len() {
                let color = (palette_ram[palette_offset] as u16)
                    | ((palette_ram[palette_offset + 1] as u16) << 8);
                self.scanline_layers[Layer::BG2 as usize][x] = Some(color);
            }
        }
    }

    fn fill_mode5(&mut self, vram: &[u8]) {
        if (self.render_registers.dispcnt & DISPCNT_BG2_EN) == 0 {
            return;
        }
        let y = self.current_scanline as usize;
        if y >= 128 {
            return;
        }
        let page_offset = if (self.render_registers.dispcnt & 0x0010) != 0 {
            0xA000
        } else {
            0
        };
        for x in 0..160 {
            let offset = page_offset + (y * 160 + x) * 2;
            let color = (vram[offset] as u16) | ((vram[offset + 1] as u16) << 8);
            self.scanline_layers[Layer::BG2 as usize][x] = Some(color);
        }
    }

    fn fill_sprites(&mut self, vram: &[u8], palette_ram: &[u8], oam: &[u8]) {
        if (self.render_registers.dispcnt & 0x1000) == 0 {
            return;
        }
        let y = self.current_scanline as usize;
        let is_2d = (self.render_registers.dispcnt & 0x0040) == 0;
        let (mos_h_all, mos_v_all) = (
            (((self.render_registers.mosaic >> 8) & 0xF) + 1) as usize,
            (((self.render_registers.mosaic >> 12) & 0xF) + 1) as usize,
        );

        for i in (0..128).rev() {
            let offset = i * 8;
            let a0 = (oam[offset] as u16) | ((oam[offset + 1] as u16) << 8);
            let a1 = (oam[offset + 2] as u16) | ((oam[offset + 3] as u16) << 8);
            let a2 = (oam[offset + 4] as u16) | ((oam[offset + 5] as u16) << 8);

            let obj_mode = (a0 >> 10) & 0x3;
            if obj_mode == 3 {
                continue;
            }

            let is_affine = (a0 & 0x0100) != 0;
            let double_size = (a0 & 0x0200) != 0;

            if !is_affine && double_size {
                continue;
            } // OBJ Disable

            let obj_y = (a0 & 0xFF) as i32;
            let shape = (a0 >> 14) & 0x3;
            let size = (a1 >> 14) & 0x3;
            let (w, h) = match (shape, size) {
                (0, 0) => (8, 8),
                (0, 1) => (16, 16),
                (0, 2) => (32, 32),
                (0, 3) => (64, 64),
                (1, 0) => (16, 8),
                (1, 1) => (32, 8),
                (1, 2) => (32, 16),
                (1, 3) => (64, 32),
                (2, 0) => (8, 16),
                (2, 1) => (8, 32),
                (2, 2) => (16, 32),
                (2, 3) => (32, 64),
                _ => (8, 8),
            };

            let (mos_h, mos_v) = if (a0 & 0x1000) != 0 {
                (mos_h_all, mos_v_all)
            } else {
                (1, 1)
            };
            let render_y = (y / mos_v) * mos_v;

            let (draw_w, draw_h) = if is_affine && double_size {
                (w * 2, h * 2)
            } else {
                (w, h)
            };
            let mut rel_y = render_y as i32 - obj_y;
            if rel_y < 0 {
                rel_y += 256;
            }
            if rel_y >= draw_h as i32 {
                continue;
            }

            let obj_x = (a1 & 0x1FF) as i32;
            let is_8bpp = (a0 & 0x2000) != 0;
            let tile_idx = (a2 & 0x3FF) as usize;
            let pal_idx = ((a2 & 0xF000) >> 12) as usize;
            let is_semi_transparent = obj_mode == 1;

            if !is_affine {
                // Regular Sprite
                let flip_h = (a1 & 0x1000) != 0;
                let flip_v = (a1 & 0x2000) != 0;
                let mut py = rel_y as usize;
                if flip_v {
                    py = h - 1 - py;
                }

                for px in 0..w {
                    let mut x = obj_x + px as i32;
                    if x >= 512 {
                        x -= 512;
                    }
                    if x < 0 || x >= SCREEN_WIDTH as i32 {
                        continue;
                    }

                    let render_px = (px / mos_h) * mos_h;
                    let mut actual_px = render_px;
                    if flip_h {
                        actual_px = w - 1 - actual_px;
                    }

                    self.render_sprite_pixel(
                        x as usize,
                        actual_px,
                        py,
                        w,
                        h,
                        tile_idx,
                        is_8bpp,
                        pal_idx,
                        a2,
                        palette_ram,
                        vram,
                        is_2d,
                        is_semi_transparent,
                        obj_mode,
                    );
                }
            } else {
                // Affine Sprite
                let matrix_idx = ((a1 >> 9) & 0x1F) as usize;
                let m_off = matrix_idx * 32 + 6;
                let pa = i16::from_le_bytes([oam[m_off], oam[m_off + 1]]) as i32;
                let pb = i16::from_le_bytes([oam[m_off + 8], oam[m_off + 9]]) as i32;
                let pc = i16::from_le_bytes([oam[m_off + 16], oam[m_off + 17]]) as i32;
                let pd = i16::from_le_bytes([oam[m_off + 24], oam[m_off + 25]]) as i32;

                let cx = (draw_w / 2) as i32;
                let cy = (draw_h / 2) as i32;
                let tcx = (w / 2) as i32;
                let tcy = (h / 2) as i32;

                for px in 0..draw_w {
                    let mut x = obj_x + px as i32;
                    if x >= 512 {
                        x -= 512;
                    }
                    if x < 0 || x >= SCREEN_WIDTH as i32 {
                        continue;
                    }

                    let render_px = (px / mos_h) * mos_h;
                    let rx = render_px as i32 - cx;
                    let ry = rel_y as i32 - cy;

                    let tx = ((pa * rx + pb * ry) >> 8) + tcx;
                    let ty = ((pc * rx + pd * ry) >> 8) + tcy;

                    if tx >= 0 && tx < w as i32 && ty >= 0 && ty < h as i32 {
                        self.render_sprite_pixel(
                            x as usize,
                            tx as usize,
                            ty as usize,
                            w,
                            h,
                            tile_idx,
                            is_8bpp,
                            pal_idx,
                            a2,
                            palette_ram,
                            vram,
                            is_2d,
                            is_semi_transparent,
                            obj_mode,
                        );
                    }
                }
            }
        }
    }

    fn render_sprite_pixel(
        &mut self,
        x: usize,
        tx: usize,
        ty: usize,
        w: usize,
        _h: usize,
        mut tile_idx: usize,
        is_8bpp: bool,
        pal_idx: usize,
        a2: u16,
        palette_ram: &[u8],
        vram: &[u8],
        is_2d: bool,
        is_semi_transparent: bool,
        obj_mode: u16,
    ) {
        if is_8bpp {
            tile_idx &= !1;
        }

        let tile_addr = if is_2d {
            tile_idx + (ty / 8) * 32 + (tx / 8) * (if is_8bpp { 2 } else { 1 })
        } else {
            tile_idx + ((ty / 8 * (w / 8)) + (tx / 8)) * (if is_8bpp { 2 } else { 1 })
        };

        let color_idx = if is_8bpp {
            let addr = 0x10000 + tile_addr * 32 + (ty % 8) * 8 + (tx % 8);
            if addr < vram.len() {
                vram[addr] as usize
            } else {
                0
            }
        } else {
            let addr = 0x10000 + tile_addr * 32 + (ty % 8) * 4 + (tx % 8) / 2;
            if addr < vram.len() {
                let d = vram[addr];
                if tx % 2 == 0 {
                    (d & 0xF) as usize
                } else {
                    (d >> 4) as usize
                }
            } else {
                0
            }
        };

        if color_idx != 0 {
            if obj_mode == 2 {
                if self.scanline_windows[x] == WIN_OUT {
                    self.scanline_windows[x] = WIN_OBJ;
                }
            } else {
                let pal_off = 512
                    + if is_8bpp {
                        color_idx * 2
                    } else {
                        (pal_idx * 16 + color_idx) * 2
                    };
                if pal_off + 1 < palette_ram.len() {
                    let color =
                        (palette_ram[pal_off] as u16) | ((palette_ram[pal_off + 1] as u16) << 8);
                    let prio = ((a2 >> 10) & 0x3) as u8;
                    if self.scanline_obj[x].is_none() || prio <= self.scanline_obj[x].unwrap().1 {
                        self.scanline_obj[x] = Some((color, prio, is_semi_transparent));
                    }
                }
            }
        }
    }

    fn get_render_hofs(&self, i: usize) -> u16 {
        match i {
            0 => self.render_registers.bg0hofs,
            1 => self.render_registers.bg1hofs,
            2 => self.render_registers.bg2hofs,
            3 => self.render_registers.bg3hofs,
            _ => 0,
        }
    }

    fn get_render_vofs(&self, i: usize) -> u16 {
        match i {
            0 => self.render_registers.bg0vofs,
            1 => self.render_registers.bg1vofs,
            2 => self.render_registers.bg2vofs,
            3 => self.render_registers.bg3vofs,
            _ => 0,
        }
    }

    fn draw_pixel_raw(&mut self, x: usize, y: usize, color: u16) {
        let r = (color & 0x1F) as u32;
        let g = ((color >> 5) & 0x1F) as u32;
        let b = ((color >> 10) & 0x1F) as u32;

        // Stretch 5-bit to 8-bit: (c * 255 / 31) or (c << 3 | c >> 2)
        let r8 = (r << 3) | (r >> 2);
        let g8 = (g << 3) | (g >> 2);
        let b8 = (b << 3) | (b >> 2);

        let fb_offset = (y * SCREEN_WIDTH + x) * 4;
        self.frame_buffer[fb_offset] = r8 as u8;
        self.frame_buffer[fb_offset + 1] = g8 as u8;
        self.frame_buffer[fb_offset + 2] = b8 as u8;
        self.frame_buffer[fb_offset + 3] = 255;
    }

    pub fn render_frame(&mut self, _vram: &[u8]) {}

    pub fn debug_render_registers(&self) -> &PpuRegisters {
        &self.render_registers
    }

    pub(crate) fn reset_transient_state(&mut self) {
        self.current_bg2x = self.registers.bg2x;
        self.current_bg2y = self.registers.bg2y;
        self.current_bg3x = self.registers.bg3x;
        self.current_bg3y = self.registers.bg3y;
        self.bg2x_written_this_line = false;
        self.bg2y_written_this_line = false;
        self.bg3x_written_this_line = false;
        self.bg3y_written_this_line = false;
        self.snapshot_render_registers();
        self.scanline_layers = [[None; SCREEN_WIDTH]; 4];
        self.scanline_obj = [None; SCREEN_WIDTH];
        self.scanline_backdrop = [0; SCREEN_WIDTH];
        self.scanline_windows = [0; SCREEN_WIDTH];
    }
}

fn trace_scanline_state(ppu: &Ppu, y: usize) {
    static TRACE: OnceLock<bool> = OnceLock::new();
    if !*TRACE.get_or_init(|| std::env::var_os("VIBE_TRACE_SCANLINE").is_some()) {
        return;
    }

    if y >= SCREEN_HEIGHT {
        return;
    }

    let dispcnt = ppu.render_registers.dispcnt;
    eprintln!(
        "[scanline] y={} mode={} dispcnt={:04X} bg0cnt={:04X} bg1cnt={:04X} bg2cnt={:04X} bg3cnt={:04X} bg2pa={:04X} bg2pb={:04X} bg2pc={:04X} bg2pd={:04X} bg2x={:08X} bg2y={:08X} bg0hofs={:04X} bg0vofs={:04X} bg1hofs={:04X} bg1vofs={:04X} bg2hofs={:04X} bg2vofs={:04X}",
        y,
        dispcnt & DISPCNT_MODE_MASK,
        dispcnt,
        ppu.render_registers.bg0cnt,
        ppu.render_registers.bg1cnt,
        ppu.render_registers.bg2cnt,
        ppu.render_registers.bg3cnt,
        ppu.render_registers.bg2pa as u16,
        ppu.render_registers.bg2pb as u16,
        ppu.render_registers.bg2pc as u16,
        ppu.render_registers.bg2pd as u16,
        ppu.render_registers.bg2x as u32,
        ppu.render_registers.bg2y as u32,
        ppu.render_registers.bg0hofs,
        ppu.render_registers.bg0vofs,
        ppu.render_registers.bg1hofs,
        ppu.render_registers.bg1vofs,
        ppu.render_registers.bg2hofs,
        ppu.render_registers.bg2vofs,
    );
}

fn trace_affine_sample(
    bg_idx: usize,
    y: usize,
    x: usize,
    curr_x: i32,
    curr_y: i32,
    tx: i32,
    ty: i32,
    map_addr: Option<usize>,
    tile_idx: Option<usize>,
    tile_addr: Option<usize>,
    color_idx: Option<usize>,
) {
    static TRACE: OnceLock<bool> = OnceLock::new();
    if !*TRACE.get_or_init(|| std::env::var_os("VIBE_TRACE_AFFINE_SAMPLE").is_some()) {
        return;
    }
    if bg_idx != 2 {
        return;
    }
    if y != 89 && y != 90 {
        return;
    }
    if x != 0 && x != 40 && x != 80 && x != 120 && x != 160 && x != 200 {
        return;
    }

    eprintln!(
        "[affine-sample] bg={} y={} x={} curr_x={:08X} curr_y={:08X} tx={} ty={} map={:?} tile={:?} tile_addr={:?} color={:?}",
        bg_idx,
        y,
        x,
        curr_x as u32,
        curr_y as u32,
        tx,
        ty,
        map_addr,
        tile_idx,
        tile_addr,
        color_idx,
    );
}

fn debug_visible_layer_mask() -> u8 {
    static MASK: OnceLock<u8> = OnceLock::new();
    *MASK.get_or_init(|| {
        std::env::var("VIBE_LAYER_MASK")
            .ok()
            .and_then(|value| u8::from_str_radix(value.trim_start_matches("0x"), 16).ok())
            .unwrap_or(0x3F)
    })
}

fn window_contains_scanline(y: usize, start: usize, end: usize) -> bool {
    const FRAME_SCANLINES: usize = 228;

    let start_valid = start < FRAME_SCANLINES;
    let end_valid = end < FRAME_SCANLINES;
    match (start_valid, end_valid) {
        (true, true) => {
            if start <= end {
                y >= start && y < end
            } else {
                y >= start || y < end
            }
        }
        // If the end event is offscreen and never occurs during the frame, the
        // window remains active through the frame wrap once it has been enabled.
        (true, false) => true,
        (false, _) => false,
    }
}

fn text_bg_dimensions(screen_size: u16) -> (usize, usize) {
    match screen_size {
        0 => (256, 256),
        1 => (512, 256),
        2 => (256, 512),
        3 => (512, 512),
        _ => (256, 256),
    }
}
