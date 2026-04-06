use super::Bus;

impl Bus {
    pub(super) fn write_dma_register_byte(&mut self, offset: usize, value: u8) {
        let channel = (offset - 0x0B0) / 12;
        let reg_idx = (offset - 0x0B0) % 12;

        match reg_idx {
            0..=3 => {
                let shift = (reg_idx % 4) * 8;
                self.dma[channel].sad =
                    (self.dma[channel].sad & !(0xFF << shift)) | ((value as u32) << shift);
            }
            4..=7 => {
                let shift = (reg_idx % 4) * 8;
                self.dma[channel].dad =
                    (self.dma[channel].dad & !(0xFF << shift)) | ((value as u32) << shift);
            }
            8..=9 => {
                let shift = (reg_idx % 2) * 8;
                self.dma[channel].count =
                    (self.dma[channel].count & !(0xFF << shift)) | ((value as u16) << shift);
            }
            10..=11 => {
                let shift = (reg_idx % 2) * 8;
                let old_ctrl = self.dma[channel].ctrl;
                self.dma[channel].ctrl =
                    (self.dma[channel].ctrl & !(0xFF << shift)) | ((value as u16) << shift);

                if (self.dma[channel].ctrl & 0x8000) != 0 && (old_ctrl & 0x8000) == 0 {
                    self.dma[channel].enable(channel == 3);
                } else if (self.dma[channel].ctrl & 0x8000) == 0 {
                    self.dma[channel].enabled = false;
                }
            }
            _ => {}
        }
    }

    pub fn process_dma(&mut self) {
        for channel in 0..4 {
            let ctrl = self.dma[channel].ctrl;
            let enabled = self.dma[channel].enabled;
            let timing = (ctrl >> 12) & 0x3;

            if enabled && timing == 0 {
                self.do_dma_transfer(channel);
            }
        }
    }

    fn do_dma_transfer(&mut self, channel: usize) {
        let is_32bit = (self.dma[channel].ctrl & (1 << 10)) != 0;
        let count = self.dma[channel].internal_count;
        let src_adj = (self.dma[channel].ctrl >> 7) & 0x3;
        let dst_adj = (self.dma[channel].ctrl >> 5) & 0x3;

        let src_mask: u32 = 0x0FFFFFFF;
        let dst_mask: u32 = if channel == 3 { 0x0FFFFFFF } else { 0x07FFFFFF };
        let step = if is_32bit { 4 } else { 2 };

        // **新增追蹤記錄**
        if true {
            println!(
                "[DMA] Channel {} transfer {} bytes: src={:08X}, dst={:08X}, ctrl={:04X}, src_adj={}",
                channel,
                count * step,
                self.dma[channel].internal_sad,
                self.dma[channel].internal_dad,
                self.dma[channel].ctrl,
                src_adj
            );
        }

        for _ in 0..count {
            let current_sad = self.dma[channel].internal_sad & src_mask;
            let current_dad = self.dma[channel].internal_dad & dst_mask;

            if is_32bit {
                let data = self.read_u32(current_sad);
                self.write_u32(current_dad, data);
            } else {
                let data = self.read_u16(current_sad);
                self.write_u16(current_dad, data);
            }

            match src_adj {
                0 => {
                    self.dma[channel].internal_sad =
                        self.dma[channel].internal_sad.wrapping_add(step) & src_mask;
                }
                1 => {
                    self.dma[channel].internal_sad =
                        self.dma[channel].internal_sad.wrapping_sub(step) & src_mask;
                }
                2 => {}
                _ => {}
            }

            match dst_adj {
                0 | 3 => {
                    self.dma[channel].internal_dad =
                        self.dma[channel].internal_dad.wrapping_add(step) & dst_mask;
                }
                1 => {
                    self.dma[channel].internal_dad =
                        self.dma[channel].internal_dad.wrapping_sub(step) & dst_mask;
                }
                2 => {}
                _ => {}
            }
        }

        self.dma[channel].enabled = false;

        let ctrl = self.dma[channel].ctrl;
        self.dma[channel].ctrl &= !(1 << 15);

        let io_addr = 0x0B0 + channel * 12;
        self.io[io_addr + 11] &= 0x7F;

        if (ctrl & (1 << 14)) != 0 {
            self.request_interrupt(1 << (8 + channel));
        }
    }
}
