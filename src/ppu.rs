#[derive(Clone, Copy)]
struct BgControl {
    priority: u16,
    char_base_block: u16,
    color_256: bool,
    screen_base_block: u16,
    screen_size: u16,
}

#[derive(Clone, Copy)]
struct TextMapEntry {
    tile_idx: u16,
    hflip: bool,
    vflip: bool,
    palette_bank: u16,
}

#[derive(Clone, Copy)]
struct TextBgPixelAddress {
    tile_x: u32,
    tile_y: u32,
    pixel_x: u32,
    pixel_y: u32,
}

pub struct Ppu {
    // 240x160 像素的 RGBA8888 緩衝區
    pub frame_buffer: Box<[u8; 240 * 160 * 4]>,
    pub frame_ready: bool,
    pub frame_count: u64,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ppu {
    const SCREEN_WIDTH: usize = 240;
    const SCREEN_HEIGHT: usize = 160;
    const TILE_SIZE: u32 = 8;
    const TEXT_SCREENBLOCK_SIZE: u32 = 0x0800;
    const DISPCNT_FORCED_BLANK: u16 = 0x0080;
    const DISPCNT_BG2_ENABLE: u16 = 0x0400;
    const BYTES_PER_PIXEL: usize = 4;

    pub fn new() -> Self {
        Self {
            frame_buffer: Box::new(
                [0; Self::SCREEN_WIDTH * Self::SCREEN_HEIGHT * Self::BYTES_PER_PIXEL],
            ),
            frame_ready: false,
            frame_count: 0,
        }
    }

    fn vram_offset(vram: &[u8], addr: u32) -> usize {
        ((addr & 0x1FFFF) as usize) % vram.len()
    }

    fn read_io_u16(io: &[u8; 1024], addr: u32) -> u16 {
        let offset = (addr & 0x03FF) as usize;
        io[offset] as u16 | ((io[offset + 1] as u16) << 8)
    }
    fn read_pal_u16(palram: &[u8], addr: u32) -> u16 {
        let offset = (addr & 0x03FF) as usize;
        palram[offset] as u16 | ((palram[offset + 1] as u16) << 8)
    }
    fn read_vram_u8(vram: &[u8], addr: u32) -> u8 {
        let offset = Self::vram_offset(vram, addr);
        vram[offset]
    }
    fn read_vram_u16(vram: &[u8], addr: u32) -> u16 {
        let offset = Self::vram_offset(vram, addr & !1);
        let next_offset = Self::vram_offset(vram, (addr & !1).wrapping_add(1));
        vram[offset] as u16 | ((vram[next_offset] as u16) << 8)
    }

    fn bg_enable_mask(bg: u32) -> u16 {
        1 << (8 + bg)
    }

    fn read_bg_control(io: &[u8; 1024], bg: u32) -> BgControl {
        let bgcnt = Self::read_io_u16(io, 0x0400_0008 + bg * 2);
        BgControl {
            priority: bgcnt & 0x0003,
            char_base_block: (bgcnt & 0x000C) >> 2,
            color_256: (bgcnt & 0x0080) != 0,
            screen_base_block: (bgcnt & 0x1F00) >> 8,
            screen_size: (bgcnt >> 14) & 0x0003,
        }
    }

    fn read_text_map_entry(vram: &[u8], screen_addr: u32, map_idx: u32) -> TextMapEntry {
        let map_data = Self::read_vram_u16(vram, screen_addr + map_idx * 2);
        TextMapEntry {
            tile_idx: map_data & 0x03FF,
            hflip: (map_data & 0x0400) != 0,
            vflip: (map_data & 0x0800) != 0,
            palette_bank: (map_data >> 12) & 0x000F,
        }
    }

    fn text_bg_pixel_address(
        x: usize,
        y: usize,
        hofs: u32,
        vofs: u32,
        bg_width: u32,
        bg_height: u32,
    ) -> TextBgPixelAddress {
        let scroll_x = (x as u32 + hofs) % bg_width;
        let scroll_y = (y as u32 + vofs) % bg_height;

        TextBgPixelAddress {
            tile_x: scroll_x / Self::TILE_SIZE,
            tile_y: scroll_y / Self::TILE_SIZE,
            pixel_x: scroll_x % Self::TILE_SIZE,
            pixel_y: scroll_y % Self::TILE_SIZE,
        }
    }

    fn text_bg_map_entry_at(
        vram: &[u8],
        screen_addr: u32,
        screen_size: u16,
        tile_x: u32,
        tile_y: u32,
    ) -> TextMapEntry {
        let screenblock_offset = Self::text_bg_screenblock_offset(screen_size, tile_x, tile_y);
        let screen_index = (tile_y % 32) * 32 + (tile_x % 32);
        Self::read_text_map_entry(vram, screen_addr + screenblock_offset, screen_index)
    }

    fn tile_byte_size(color_256: bool) -> u32 {
        if color_256 { 64 } else { 32 }
    }

    fn format_vram_bytes(vram: &[u8], start_addr: u32, byte_count: usize) -> String {
        (0..byte_count)
            .map(|index| {
                format!(
                    "{:02X}",
                    Self::read_vram_u8(vram, start_addr + index as u32)
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn format_screen_entries(vram: &[u8], screen_addr: u32, entry_count: usize) -> String {
        (0..entry_count)
            .map(|index| {
                format!(
                    "{:04X}",
                    Self::read_vram_u16(vram, screen_addr + (index as u32) * 2)
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn count_non_zero_tile_bytes(
        vram: &[u8],
        char_addr: u32,
        tile_idx: u16,
        color_256: bool,
    ) -> usize {
        let tile_addr = char_addr + tile_idx as u32 * Self::tile_byte_size(color_256);
        let byte_count = Self::tile_byte_size(color_256) as usize;
        (0..byte_count)
            .filter(|index| Self::read_vram_u8(vram, tile_addr + *index as u32) != 0)
            .count()
    }

    fn log_mode_0_state(&self, io: &[u8; 1024], palram: &[u8], vram: &[u8], dispcnt: u16) {
        for bg in 0..4 {
            if (dispcnt & Self::bg_enable_mask(bg)) == 0 {
                continue;
            }

            let control = Self::read_bg_control(io, bg);
            let char_addr = 0x0600_0000 + control.char_base_block as u32 * 0x4000;
            let screen_addr =
                0x0600_0000 + control.screen_base_block as u32 * Self::TEXT_SCREENBLOCK_SIZE;
            let hofs = Self::read_io_u16(io, 0x0400_0010 + bg * 4);
            let vofs = Self::read_io_u16(io, 0x0400_0012 + bg * 4);
            let sample_entry = Self::read_text_map_entry(vram, screen_addr, 0);
            let sample_tile_addr =
                char_addr + sample_entry.tile_idx as u32 * Self::tile_byte_size(control.color_256);
            let screen_bytes = Self::format_vram_bytes(vram, screen_addr, 16);
            let screen_entries = Self::format_screen_entries(vram, screen_addr, 8);
            let sample_tile_bytes = Self::format_vram_bytes(
                vram,
                sample_tile_addr,
                Self::tile_byte_size(control.color_256).min(16) as usize,
            );
            let sample_tile_non_zero = Self::count_non_zero_tile_bytes(
                vram,
                char_addr,
                sample_entry.tile_idx,
                control.color_256,
            );
            let sample_color_idx = Self::read_text_bg_pixel(
                vram,
                char_addr,
                sample_entry.tile_idx,
                control.color_256,
                0,
                0,
            );
            let sample_palette = if sample_color_idx == 0 {
                0
            } else if control.color_256 {
                Self::read_pal_u16(palram, 0x0500_0000 + sample_color_idx as u32 * 2)
            } else {
                Self::read_pal_u16(
                    palram,
                    0x0500_0000
                        + (sample_entry.palette_bank as u32 * 16 + sample_color_idx as u32) * 2,
                )
            };

            println!(
                "[PPU][BG{}] prio={} char_base={:08X} screen_base={:08X} 8bpp={} size={} hofs={} vofs={} tile={} hflip={} vflip={} pal_bank={} sample_idx={} sample_rgb555={:04X} tile_addr={:08X} tile_non_zero={} tile_bytes=[{}] screen_bytes=[{}] screen_entries=[{}]",
                bg,
                control.priority,
                char_addr,
                screen_addr,
                control.color_256,
                control.screen_size,
                hofs,
                vofs,
                sample_entry.tile_idx,
                sample_entry.hflip,
                sample_entry.vflip,
                sample_entry.palette_bank,
                sample_color_idx,
                sample_palette,
                sample_tile_addr,
                sample_tile_non_zero,
                sample_tile_bytes,
                screen_bytes,
                screen_entries,
            );
        }
    }

    fn pixel_offset(x: usize, y: usize) -> usize {
        (y * Self::SCREEN_WIDTH + x) * Self::BYTES_PER_PIXEL
    }

    fn write_pixel_rgba(&mut self, x: usize, y: usize, color: [u8; 4]) {
        let offset = Self::pixel_offset(x, y);
        self.frame_buffer[offset] = color[0];
        self.frame_buffer[offset + 1] = color[1];
        self.frame_buffer[offset + 2] = color[2];
        self.frame_buffer[offset + 3] = color[3];
    }

    fn clear_scanline(&mut self, y: usize, color: [u8; 4]) {
        for x in 0..Self::SCREEN_WIDTH {
            self.write_pixel_rgba(x, y, color);
        }
    }

    fn render_mode_3_scanline(&mut self, vram: &[u8], y: usize) {
        for x in 0..Self::SCREEN_WIDTH {
            let addr = 0x0600_0000 + (y * Self::SCREEN_WIDTH + x) as u32 * 2;
            let color_15 = Self::read_vram_u16(vram, addr);
            self.write_pixel_rgba(x, y, Self::rgb15_to_rgba(color_15));
        }
    }

    fn render_mode_4_scanline(&mut self, palram: &[u8], vram: &[u8], dispcnt: u16, y: usize) {
        let page = if (dispcnt & 0x0010) != 0 { 1 } else { 0 };
        let page_offset = 0xA000 * page;

        for x in 0..Self::SCREEN_WIDTH {
            let addr = 0x0600_0000 + page_offset + (y * Self::SCREEN_WIDTH + x) as u32;
            let index = Self::read_vram_u8(vram, addr);
            if index != 0 {
                let color_15 = Self::read_pal_u16(palram, 0x0500_0000 + (index as u32) * 2);
                self.write_pixel_rgba(x, y, Self::rgb15_to_rgba(color_15));
            }
        }
    }

    fn render_mode_5_scanline(&mut self, vram: &[u8], dispcnt: u16, y: usize) {
        if y >= 128 {
            return;
        }

        let page = if (dispcnt & 0x0010) != 0 { 1 } else { 0 };
        let page_offset = 0xA000 * page;
        let bitmap_width = 160;

        for x in 0..bitmap_width {
            let addr = 0x0600_0000 + page_offset + (y * bitmap_width + x) as u32 * 2;
            let color_15 = Self::read_vram_u16(vram, addr);
            self.write_pixel_rgba(x, y, Self::rgb15_to_rgba(color_15));
        }
    }

    fn text_bg_dimensions(screen_size: u16) -> (u32, u32) {
        match screen_size & 0x3 {
            0 => (256, 256),
            1 => (512, 256),
            2 => (256, 512),
            3 => (512, 512),
            _ => (256, 256),
        }
    }

    fn text_bg_screenblock_offset(screen_size: u16, tile_x: u32, tile_y: u32) -> u32 {
        let block_x = tile_x / 32;
        let block_y = tile_y / 32;

        match screen_size & 0x3 {
            0 => 0,
            1 => block_x * Self::TEXT_SCREENBLOCK_SIZE,
            2 => block_y * Self::TEXT_SCREENBLOCK_SIZE,
            3 => (block_y * 2 + block_x) * Self::TEXT_SCREENBLOCK_SIZE,
            _ => 0,
        }
    }

    fn read_text_bg_pixel(
        vram: &[u8],
        char_addr: u32,
        tile_idx: u16,
        color_256: bool,
        pixel_x: u32,
        pixel_y: u32,
    ) -> u8 {
        if color_256 {
            let tile_addr = char_addr + tile_idx as u32 * 64;
            Self::read_vram_u8(vram, tile_addr + pixel_y * Self::TILE_SIZE + pixel_x)
        } else {
            let tile_addr = char_addr + tile_idx as u32 * 32;
            let byte = Self::read_vram_u8(vram, tile_addr + pixel_y * 4 + (pixel_x / 2));
            if (pixel_x & 1) == 0 {
                byte & 0x0F
            } else {
                (byte >> 4) & 0x0F
            }
        }
    }

    fn lookup_bg_palette_color(
        palram: &[u8],
        color_idx: u8,
        color_256: bool,
        palette_bank: u16,
    ) -> [u8; 4] {
        let pal_addr = if color_256 {
            0x0500_0000 + color_idx as u32 * 2
        } else {
            0x0500_0000 + (palette_bank as u32 * 16 + color_idx as u32) * 2
        };
        Self::rgb15_to_rgba(Self::read_pal_u16(palram, pal_addr))
    }

    fn render_mode_0_bg_scanline(
        &mut self,
        io: &[u8; 1024],
        palram: &[u8],
        vram: &[u8],
        bg: u32,
        control: BgControl,
        y: usize,
    ) {
        let (bg_width, bg_height) = Self::text_bg_dimensions(control.screen_size);
        let char_addr = 0x0600_0000 + control.char_base_block as u32 * 0x4000;
        let screen_addr =
            0x0600_0000 + control.screen_base_block as u32 * Self::TEXT_SCREENBLOCK_SIZE;

        let hofs = Self::read_io_u16(io, 0x0400_0010 + bg * 4) as u32 % bg_width;
        let vofs = Self::read_io_u16(io, 0x0400_0012 + bg * 4) as u32 % bg_height;

        for x in 0..Self::SCREEN_WIDTH {
            let address = Self::text_bg_pixel_address(x, y, hofs, vofs, bg_width, bg_height);
            let entry = Self::text_bg_map_entry_at(
                vram,
                screen_addr,
                control.screen_size,
                address.tile_x,
                address.tile_y,
            );

            let pixel_x = if entry.hflip {
                Self::TILE_SIZE - 1 - address.pixel_x
            } else {
                address.pixel_x
            };
            let pixel_y = if entry.vflip {
                Self::TILE_SIZE - 1 - address.pixel_y
            } else {
                address.pixel_y
            };

            let color_idx = Self::read_text_bg_pixel(
                vram,
                char_addr,
                entry.tile_idx,
                control.color_256,
                pixel_x,
                pixel_y,
            );
            if color_idx == 0 {
                continue;
            }

            let color = Self::lookup_bg_palette_color(
                palram,
                color_idx,
                control.color_256,
                entry.palette_bank,
            );
            self.write_pixel_rgba(x, y, color);
        }
    }

    fn render_mode_0_scanline(
        &mut self,
        io: &[u8; 1024],
        palram: &[u8],
        vram: &[u8],
        dispcnt: u16,
        y: usize,
    ) {
        let mut bg_order = [0u32, 1, 2, 3];
        bg_order.sort_by_key(|bg| {
            let control = Self::read_bg_control(io, *bg);
            (control.priority, *bg as u16)
        });

        for bg in bg_order.into_iter().rev() {
            if (dispcnt & Self::bg_enable_mask(bg)) == 0 {
                continue;
            }
            let control = Self::read_bg_control(io, bg);
            self.render_mode_0_bg_scanline(io, palram, vram, bg, control, y);
        }
    }

    /// 在掃描線剛改變時被呼叫
    pub fn render_scanline(&mut self, bus: &crate::memory::Bus, scanline: u16) {
        let io = &bus.io;
        let palram = &bus.palram[..];
        let vram = &bus.vram[..];
        if scanline == 160 {
            self.frame_ready = true;
            return;
        }
        if scanline >= 160 {
            // VBlank 區間不繪圖
            return;
        }

        let dispcnt = Self::read_io_u16(io, 0x0400_0000);
        let bg_mode = dispcnt & 0x0007;

        if scanline == 0 {
            self.frame_count += 1;
            println!(
                "[PPU] Frame {} Mode: {}, DISPCNT: {:04X}, forced_blank={}, bg0={}, bg1={}, bg2={}, bg3={}",
                self.frame_count,
                bg_mode,
                dispcnt,
                (dispcnt & Self::DISPCNT_FORCED_BLANK) != 0,
                (dispcnt & 0x0100) != 0,
                (dispcnt & 0x0200) != 0,
                (dispcnt & Self::DISPCNT_BG2_ENABLE) != 0,
                (dispcnt & 0x0800) != 0,
            );
            if bg_mode == 0 {
                self.log_mode_0_state(io, palram, vram, dispcnt);
            }
        }

        // 1. 畫背景底色 (Palette RAM 的第 0 色)
        // Palettes are at 0x0500_0000
        let backdrop_color = Self::rgb15_to_rgba(Self::read_pal_u16(palram, 0x0500_0000));
        let y = scanline as usize;
        self.clear_scanline(y, backdrop_color);

        match bg_mode {
            0 => self.render_mode_0_scanline(io, palram, vram, dispcnt, y),
            3 if (dispcnt & Self::DISPCNT_BG2_ENABLE) != 0 => self.render_mode_3_scanline(vram, y),
            4 if (dispcnt & Self::DISPCNT_BG2_ENABLE) != 0 => {
                self.render_mode_4_scanline(palram, vram, dispcnt, y)
            }
            5 if (dispcnt & Self::DISPCNT_BG2_ENABLE) != 0 => {
                self.render_mode_5_scanline(vram, dispcnt, y)
            }
            _ => {
                // Mode 1/2 affine BGs are not implemented yet; keep backdrop only.
            }
        }
    }

    /// 將 GBA 的 15-bit 色彩轉換為 RGBA8888
    fn rgb15_to_rgba(color: u16) -> [u8; 4] {
        let r = (color & 0x001F) as u32;
        let g = ((color >> 5) & 0x001F) as u32;
        let b = ((color >> 10) & 0x001F) as u32;

        let r_8 = (r << 3) | (r >> 2);
        let g_8 = (g << 3) | (g >> 2);
        let b_8 = (b << 3) | (b >> 2);

        [r_8 as u8, g_8 as u8, b_8 as u8, 0xFF]
    }
}
