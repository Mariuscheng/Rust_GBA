use crate::memory::Bus;

#[derive(Clone, Copy, Default)]
pub struct DmaChannel {
    // 來源與目的位址暫存器 (由遊戲設定)
    pub sad: u32,
    pub dad: u32,
    pub count: u16,
    pub ctrl: u16,

    // DMA 執行時使用的內部計數與指標暫存器
    pub internal_sad: u32,
    pub internal_dad: u32,
    pub internal_count: u32, // DMA3 可以高達 0x10000
    pub enabled: bool,
}

impl DmaChannel {
    pub fn new() -> Self {
        Self::default()
    }

    /// 當 DMA 控制暫存器 (CNT_H) 的最高位 (Enable bit 15) 被設為 1 時觸發
    pub fn enable(&mut self, is_dma3: bool) {
        self.enabled = true;
        self.internal_sad = self.sad;
        self.internal_dad = self.dad;

        // Count 為 0 時，代表最大傳輸量
        let max_count = if is_dma3 { 0x10000 } else { 0x4000 };
        self.internal_count = if self.count == 0 {
            max_count
        } else {
            self.count as u32
        };
    }

    /// 執行一次 DMA 傳輸步進 (每次傳輸 16-bit 或 32-bit)
    pub fn step(&mut self, bus: &mut Bus) -> bool {
        if !self.enabled {
            return false;
        }

        let is_32bit = (self.ctrl & (1 << 10)) != 0;
        let addr_step = if is_32bit { 4 } else { 2 };

        // 執行實際的讀寫
        if is_32bit {
            let val = bus.read_u32(self.internal_sad);
            bus.write_u32(self.internal_dad, val);
        } else {
            let val = bus.read_u16(self.internal_sad);
            bus.write_u16(self.internal_dad, val);
        }

        // 更新內部地址（根據 DST/SRC 控制位元增加、減少或固定）
        self.update_addresses(addr_step);

        self.internal_count -= 1;
        if self.internal_count == 0 {
            self.enabled = false;
            // 如果沒開啟 Repeat，則關閉 Enable bit
            if (self.ctrl & (1 << 9)) == 0 {
                self.ctrl &= !(1 << 15);
            }
            return true; // 傳輸完成
        }
        false
    }

    fn update_addresses(&mut self, addr_step: u32) {
        let src_adj = (self.ctrl >> 7) & 0x3;
        let dst_adj = (self.ctrl >> 5) & 0x3;

        let src_mask: u32 = 0x0FFF_FFFF;
        let dst_mask: u32 = 0x0FFF_FFFF;

        match src_adj {
            0 => {
                self.internal_sad = self.internal_sad.wrapping_add(addr_step) & src_mask;
            }
            1 => {
                self.internal_sad = self.internal_sad.wrapping_sub(addr_step) & src_mask;
            }
            2 => {}
            _ => {}
        }

        match dst_adj {
            0 | 3 => {
                self.internal_dad = self.internal_dad.wrapping_add(addr_step) & dst_mask;
            }
            1 => {
                self.internal_dad = self.internal_dad.wrapping_sub(addr_step) & dst_mask;
            }
            2 => {}
            _ => {}
        }
    }
}
