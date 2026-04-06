use super::Bus;

const REG_DISPSTAT: usize = 0x004;
const REG_VCOUNT_LO: usize = 0x006;
const REG_VCOUNT_HI: usize = 0x007;

const BG_SCROLL_START: usize = 0x010;
const BG_SCROLL_END: usize = 0x01F;

const IRQ_VBLANK: u16 = 0x0001;
const IRQ_HBLANK: u16 = 0x0002;
const IRQ_VCOUNT: u16 = 0x0004;

impl Bus {
    pub(super) fn write_io_u8(&mut self, offset: usize, value: u8) {
        if (0x100..=0x10F).contains(&offset) {
            self.write_timer_register(offset, value);
            return;
        }

        if (BG_SCROLL_START..=BG_SCROLL_END).contains(&offset) {
            self.write_bg_scroll_byte(offset, value);
        } else if offset == 0x202 || offset == 0x203 {
            self.io[offset] &= !value; // IF 暫存器是寫 1 清零
        } else if offset == REG_DISPSTAT {
            // 核心修正：DISPSTAT 唯讀位元保護
            // 0xF8 (11111000) 代表只有第 3-7 位是可寫的
            let mask = 0xF8;
            self.io[offset] = (self.io[offset] & !mask) | (value & mask);
        } else if offset != 0x130 && offset != 0x131 {
            self.io[offset] = value;
        }

        if offset == REG_DISPSTAT {
            // DISPSTAT (0x04000004) 的低 3 位 (0, 1, 2) 是硬體狀態位元，不可寫
            // 只有位元 3, 4, 5 (中斷啟動) 與 7 (VCount 設定位) 是可寫的
            let write_mask = 0xF8; // 11111000
            let old_val = self.io[offset];
            self.io[offset] = (old_val & !write_mask) | (value & write_mask);
        }

        if (0x0B0..=0x0DF).contains(&offset) {
            self.write_dma_register_byte(offset, value);
        }
    }

    fn write_bg_scroll_byte(&mut self, offset: usize, value: u8) {
        let reg_base = offset & !1;
        let current = self.io[reg_base] as u16 | ((self.io[reg_base + 1] as u16) << 8);
        let new_value = if (offset & 1) == 0 {
            (current & 0x0100) | value as u16
        } else {
            (current & 0x00FF) | (((value as u16) & 0x0001) << 8)
        } & 0x01FF;

        self.io[reg_base] = (new_value & 0x00FF) as u8;
        self.io[reg_base + 1] = ((new_value >> 8) & 0x0001) as u8;
    }

    pub fn tick(&mut self, cycles: usize) {
        let previous_cycles = self.cycles;
        self.cycles += cycles as u64;
        self.tick_timers(cycles as u32);

        let cycles_per_scanline = 1232;
        let total_scanlines = 228;

        let previous_cycle_in_frame = previous_cycles % (cycles_per_scanline * total_scanlines);
        let previous_scanline = (previous_cycle_in_frame / cycles_per_scanline) as u16;
        let previous_cycle_in_scanline = previous_cycle_in_frame % cycles_per_scanline;

        let cycle_in_frame = self.cycles % (cycles_per_scanline * total_scanlines);
        let scanline = (cycle_in_frame / cycles_per_scanline) as u16;
        let cycle_in_scanline = cycle_in_frame % cycles_per_scanline;

        self.io[REG_VCOUNT_LO] = (scanline & 0xFF) as u8;
        self.io[REG_VCOUNT_HI] = (scanline >> 8) as u8;

        let is_vblank = scanline >= 160;
        let is_hblank = cycle_in_scanline >= 960;

        let mut dispstat = self.io[REG_DISPSTAT];

        if is_vblank {
            dispstat |= 0x01;
        } else {
            dispstat &= !0x01;
        }

        if is_hblank {
            dispstat |= 0x02;
        } else {
            dispstat &= !0x02;
        }

        let vcount_setting = self.io[REG_DISPSTAT + 1];
        let vcount_match = scanline == (vcount_setting as u16);
        if vcount_match {
            dispstat |= 0x04;
        } else {
            dispstat &= !0x04;
        }

        self.io[REG_DISPSTAT] = dispstat;

        if cycles > 0 {
            let trigger_vblank = previous_scanline < 160 && scanline >= 160;
            let trigger_hblank = previous_scanline == scanline
                && previous_cycle_in_scanline < 960
                && cycle_in_scanline >= 960;
            let previous_vcount_match = previous_scanline == (vcount_setting as u16);
            let trigger_vcount = !previous_vcount_match && vcount_match;

            let irq_enable = self.io[REG_DISPSTAT];

            if trigger_vblank {
                self.process_dma_trigger(1);
                if (irq_enable & 0x08) != 0 {
                    self.request_interrupt(IRQ_VBLANK);
                }
            }
            if trigger_hblank {
                self.process_dma_trigger(2);
                if (irq_enable & 0x10) != 0 {
                    self.request_interrupt(IRQ_HBLANK);
                }
            }
            if trigger_vcount && (irq_enable & 0x20) != 0 {
                self.request_interrupt(IRQ_VCOUNT);
            }
        }
    }
}
