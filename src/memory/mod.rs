use std::cell::RefCell;
use std::collections::VecDeque;

mod dma;
mod io;
mod timer;
mod utils;

pub use utils::{lz77_decompress_vram, lz77_decompress_wram};

pub struct Bus {
    pub bios: Box<[u8; 16384]>,
    pub rom: Vec<u8>,
    pub ewram: Box<[u8; 256 * 1024]>,
    pub iwram: Box<[u8; 32 * 1024]>,
    pub io: Box<[u8; 1024]>,
    pub palram: Box<[u8; 1024]>,
    pub vram: Box<[u8; 96 * 1024]>,
    pub oam: Box<[u8; 1024]>,
    pub ppu: crate::ppu::Ppu,
    pub dma: [crate::dma::DmaChannel; 4],
    pub timer_reload: [u16; 4],
    pub timer_counter: [u16; 4],
    pub timer_control: [u16; 4],
    pub timer_prescaler_accum: [u32; 4],
    pub cycles: u64,
    pub io_trace: RefCell<VecDeque<String>>,
}

impl Bus {

    pub fn process_dma_trigger(&mut self, timing: u16) {
        for i in 0..4 {
            let start_timing = (self.dma[i].ctrl >> 12) & 0x03;
            if self.dma[i].enabled && start_timing == timing {
                self.perform_dma(i);
            }
        }
    }

    fn masked_offset(addr: u32, mask: u32, len: usize) -> usize {
        ((addr & mask) as usize) % len
    }

    fn vram_offset(&self, addr: u32) -> usize {
        ((addr & 0x0001_FFFF) as usize) % self.vram.len()
    }

    fn vram_halfword_offset(&self, addr: u32) -> usize {
        self.vram_offset(addr & 0x0001_FFFE)
    }

    fn write_vram_u8_internal(&mut self, addr: u32, value: u8) {
        let offset = self.vram_halfword_offset(addr);
        let next_offset = self.vram_offset((addr & 0x0001_FFFE).wrapping_add(1));

        self.vram[offset] = value;
        self.vram[next_offset] = value;
    }

    fn write_vram_u16_internal(&mut self, addr: u32, value: u16) {
        if addr >= 0x06000000 && addr < 0x06018000 && value != 0 {
            println!(
                "\x1b[33m[VRAM WRITE] addr={:08X} value={:04X}\x1b[0m",
                addr, value
            );
        }
        let offset = self.vram_halfword_offset(addr);
        let next_offset = self.vram_offset((addr & 0x0001_FFFE).wrapping_add(1));

        self.vram[offset] = (value & 0x00FF) as u8;
        self.vram[next_offset] = (value >> 8) as u8;
    }

    fn write_vram_u32_internal(&mut self, addr: u32, value: u32) {
        let aligned_addr = addr & !3;
        self.write_vram_u16_internal(aligned_addr, value as u16);
        self.write_vram_u16_internal(aligned_addr + 2, (value >> 16) as u16);
    }

    fn write_region_u8(region: &mut [u8], addr: u32, mask: u32, value: u8) {
        let offset = Self::masked_offset(addr, mask, region.len());
        region[offset] = value;
    }

    fn write_region_u16(region: &mut [u8], addr: u32, mask: u32, value: u16) {
        if addr >= 0x05000000 && addr < 0x05000400 && value != 0 {
            println!(
                "\x1b[36m[PAL WRITE] addr={:08X} value={:04X}\x1b[0m",
                addr, value
            );
        }
        let aligned_addr = addr & !1;
        let offset = Self::masked_offset(aligned_addr, mask, region.len());
        let next_offset = Self::masked_offset(aligned_addr.wrapping_add(1), mask, region.len());
        region[offset] = (value & 0x00FF) as u8;
        region[next_offset] = (value >> 8) as u8;
    }

    fn write_region_u32(region: &mut [u8], addr: u32, mask: u32, value: u32) {
        let aligned_addr = addr & !3;
        Self::write_region_u16(region, aligned_addr, mask, value as u16);
        Self::write_region_u16(region, aligned_addr + 2, mask, (value >> 16) as u16);
    }

    pub fn new(rom: Vec<u8>) -> Self {
        let mut bios = Box::new([0u8; 16384]);
        bios[0x18..0x1C].copy_from_slice(&[0x38, 0x00, 0x00, 0xEA]);
        bios[0x100..0x104].copy_from_slice(&[0x0F, 0x50, 0x2D, 0xE9]);
        bios[0x104..0x108].copy_from_slice(&[0x18, 0x00, 0x9F, 0xE5]);
        bios[0x108..0x10C].copy_from_slice(&[0x00, 0x00, 0x90, 0xE5]);
        bios[0x10C..0x110].copy_from_slice(&[0x00, 0x00, 0x50, 0xE3]);
        bios[0x110..0x114].copy_from_slice(&[0x01, 0x00, 0x00, 0x0A]);
        bios[0x114..0x118].copy_from_slice(&[0x00, 0xE0, 0x8F, 0xE2]);
        bios[0x118..0x11C].copy_from_slice(&[0x10, 0xFF, 0x2F, 0xE1]);
        bios[0x11C..0x120].copy_from_slice(&[0x0F, 0x50, 0xBD, 0xE8]);
        bios[0x120..0x124].copy_from_slice(&[0x04, 0xF0, 0x5E, 0xE2]);
        bios[0x124..0x128].copy_from_slice(&[0xFC, 0x7F, 0x00, 0x03]);

        let mut io = Box::new([0u8; 1024]);
        io[0x130] = 0xFF;
        io[0x131] = 0x03;

        for i in 0x20..=0x3F {
            io[i] = 0x01;
        }

        Self {
            bios,
            rom,
            ewram: Box::new([0; 256 * 1024]),
            iwram: Box::new([0; 32 * 1024]),
            io,
            palram: Box::new([0; 1024]),
            vram: Box::new([0; 96 * 1024]),
            oam: Box::new([0; 1024]),
            ppu: crate::ppu::Ppu::new(),
            dma: Default::default(),
            timer_reload: [0; 4],
            timer_counter: [0; 4],
            timer_control: [0; 4],
            timer_prescaler_accum: [0; 4],
            cycles: 0,
            io_trace: RefCell::new(VecDeque::with_capacity(100)),
        }
    }

    pub fn interrupt_enable(&self) -> u16 {
        self.io[0x200] as u16 | ((self.io[0x201] as u16) << 8)
    }

    pub fn interrupt_flags(&self) -> u16 {
        self.io[0x202] as u16 | ((self.io[0x203] as u16) << 8)
    }

    pub fn interrupt_master_enable(&self) -> bool {
        (self.io[0x208] & 1) != 0
    }

    pub fn pending_interrupts(&self) -> u16 {
        self.interrupt_enable() & self.interrupt_flags()
    }

    pub fn request_interrupt(&mut self, flags: u16) {
        let new_flags = self.interrupt_flags() | flags;
        self.io[0x202] = (new_flags & 0x00FF) as u8;
        self.io[0x203] = (new_flags >> 8) as u8;
    }

    pub fn read_u8(&self, addr: u32) -> u8 {
        match addr >> 24 {
            0x00 => {
                if (addr as usize) < self.bios.len() {
                    self.bios[addr as usize]
                } else {
                    0
                }
            }
            0x02 => self.ewram[(addr as usize) % (256 * 1024)],
            0x03 => self.iwram[(addr as usize) % (32 * 1024)],
            0x04 => {
                let offset = (addr & 0x0000_03FF) as usize;
                self.io[offset]
            }
            0x05 => {
                let offset = (addr & 0x0000_03FF) as usize;
                self.palram[offset % self.palram.len()]
            }
            0x06 => {
                let offset = (addr & 0x0001_FFFF) as usize;
                self.vram[offset % self.vram.len()]
            }
            0x07 => {
                let offset = (addr & 0x0000_03FF) as usize;
                self.oam[offset % self.oam.len()]
            }
            0x08 | 0x09 | 0x0A => {
                let offset = (addr & 0x01FFFFFF) as usize;
                if offset < self.rom.len() {
                    self.rom[offset]
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    pub fn read_u16(&self, addr: u32) -> u16 {
        let aligned_addr = addr & !1;

        let val = match aligned_addr >> 24 {
            0x00 => {
                if aligned_addr < 0x4000 {
                    u16::from_le_bytes([
                        self.bios[aligned_addr as usize],
                        self.bios[aligned_addr as usize + 1],
                    ])
                } else {
                    0
                }
            }
            0x02 => {
                let offset = (aligned_addr & 0x3_FFFF) as usize;
                u16::from_le_bytes([self.ewram[offset], self.ewram[offset + 1]])
            }
            0x03 => {
                let offset = (aligned_addr & 0x7FFF) as usize;
                u16::from_le_bytes([self.iwram[offset], self.iwram[offset + 1]])
            }
            0x04 => {
                let offset = (aligned_addr & 0x3FF) as usize;
                u16::from_le_bytes([self.io[offset], self.io[offset + 1]])
            }
            0x05 => {
                let offset = (aligned_addr & 0x3FF) as usize;
                u16::from_le_bytes([self.palram[offset], self.palram[offset + 1]])
            }
            0x06 => {
                let mut offset = (aligned_addr & 0x1FFFF) as usize;
                if offset >= 96 * 1024 {
                    offset -= 0x8000;
                }
                u16::from_le_bytes([self.vram[offset], self.vram[offset + 1]])
            }
            0x08..=0x0D => {
                let offset = (aligned_addr & 0x01FF_FFFF) as usize;
                if offset + 1 < self.rom.len() {
                    u16::from_le_bytes([self.rom[offset], self.rom[offset + 1]])
                } else {
                    0xFFFF
                }
            }
            _ => {
                let b0 = self.read_u8(aligned_addr);
                let b1 = self.read_u8(aligned_addr + 1);
                u16::from_le_bytes([b0, b1])
            }
        };

        if (addr >> 24) == 0x04 {
            let offset = addr & 0x0000_03FF;
            if offset != 0x200
                && offset != 0x202
                && offset != 0x208
                && offset != 0x004
                && offset != 0x006
            {
                let mut trace = self.io_trace.borrow_mut();
                if trace.len() >= 100 {
                    trace.pop_front();
                }
                trace.push_back(format!("read_u16(0x{:08X}) = 0x{:04X}", addr, val));
            }
        }

        val
    }

    pub fn read_u32(&self, addr: u32) -> u32 {
        let aligned_addr = addr & !3; // 取得 4-byte 對齊位址

        // 先取得對齊後的 32-bit 原始數值 (原本的 match 邏輯)
        let val = match aligned_addr >> 24 {
            0x02 => {
                let offset = (aligned_addr & 0x3_FFFF) as usize;
                u32::from_le_bytes(self.ewram[offset..offset + 4].try_into().unwrap())
            }
            0x03 => {
                let offset = (aligned_addr & 0x7FFF) as usize;
                u32::from_le_bytes(self.iwram[offset..offset + 4].try_into().unwrap())
            }
            0x06 => {
                let mut offset = (aligned_addr & 0x1FFFF) as usize;
                if offset >= 96 * 1024 {
                    offset -= 0x8000;
                }
                u32::from_le_bytes([
                    self.vram[offset],
                    self.vram[offset + 1],
                    self.vram[offset + 2],
                    self.vram[offset + 3],
                ])
            }
            0x08..=0x0D => {
                let offset = (aligned_addr & 0x01FF_FFFF) as usize;
                if offset + 3 < self.rom.len() {
                    u32::from_le_bytes(self.rom[offset..offset + 4].try_into().unwrap())
                } else {
                    0
                }
            }
            _ => {
                let h0 = self.read_u16(aligned_addr) as u32;
                let h1 = self.read_u16(aligned_addr + 2) as u32;
                h0 | (h1 << 16)
            }
        };

        // 關鍵修正：補上循環右移 (Rotate Right)
        // 根據 ARM7TDMI 規範：LDR 讀取非對齊位址時，會將讀到的 Word 依照 (addr & 3) * 8 bit 進行右移
        val.rotate_right((addr & 3) * 8)
    }

    pub fn write_u8(&mut self, addr: u32, value: u8) {
        match addr >> 24 {
            0x02 => {
                Self::write_region_u8(&mut self.ewram[..], addr, 0x0003_FFFF, value);
            }
            0x03 => {
                Self::write_region_u8(&mut self.iwram[..], addr, 0x0000_7FFF, value);
            }
            0x04 => {
                let offset = (addr & 0x0000_03FF) as usize;
                self.write_io_u8(offset, value);
            }
            0x05 => {
                Self::write_region_u8(&mut self.palram[..], addr, 0x0000_03FF, value);
            }
            0x06 => {
                self.write_vram_u8_internal(addr, value);
            }
            0x07 => {
                Self::write_region_u8(&mut self.oam[..], addr, 0x0000_03FF, value);
            }
            _ => {}
        }
    }

    pub fn write_u16(&mut self, addr: u32, value: u16) {
        let aligned_addr = addr & !1;
        match aligned_addr >> 24 {
            0x02 => {
                let offset = (aligned_addr & 0x3_FFFF) as usize;
                self.ewram[offset] = (value & 0xFF) as u8;
                self.ewram[offset + 1] = (value >> 8) as u8;
            }
            0x03 => {
                let offset = (aligned_addr & 0x7FFF) as usize;
                self.iwram[offset] = (value & 0xFF) as u8;
                self.iwram[offset + 1] = (value >> 8) as u8;
            }
            0x04 => {
                let offset = (aligned_addr & 0x3FF) as usize;
                self.write_io_u8(offset, (value & 0xFF) as u8);
                self.write_io_u8(offset + 1, (value >> 8) as u8);
            }
            0x05 => {
                Self::write_region_u16(&mut self.palram[..], aligned_addr, 0x0000_03FF, value);
            }
            0x06 => {
                self.write_vram_u16_internal(aligned_addr, value);
            }
            0x07 => {
                Self::write_region_u16(&mut self.oam[..], aligned_addr, 0x0000_03FF, value);
            }
            _ => {
                self.write_u8(aligned_addr, (value & 0xFF) as u8);
                self.write_u8(aligned_addr + 1, (value >> 8) as u8);
            }
        }
    }

    pub fn write_u32(&mut self, addr: u32, value: u32) {
        let aligned_addr = addr & !3;
        match aligned_addr >> 24 {
            0x05 => {
                Self::write_region_u32(&mut self.palram[..], aligned_addr, 0x0000_03FF, value);
            }
            0x06 => {
                self.write_vram_u32_internal(aligned_addr, value);
            }
            0x07 => {
                Self::write_region_u32(&mut self.oam[..], aligned_addr, 0x0000_03FF, value);
            }
            _ => {
                self.write_u16(aligned_addr, (value & 0xFFFF) as u16);
                self.write_u16(aligned_addr + 2, (value >> 16) as u16);
            }
        }
    }

    pub fn perform_dma(&mut self, ch: usize) {
        // 1. 先把參數抓出來，避免在循環中重複訪問 self.dma
        let (mut src, mut dst, count, is_32bit, s_step, d_step) = {
            let channel = &mut self.dma[ch];
            if !channel.enabled { return; }
    
            // 如果是 Repeat 模式且 Dest Ctrl == 3，重置目標地址
            if ((channel.ctrl >> 5) & 0x03) == 3 {
                channel.internal_dad = channel.dad;
            }
    
            let cnt = if channel.count == 0 {
                if ch == 3 { 0x10000 } else { 0x4000 }
            } else {
                channel.count as u32
            };
    
            (channel.internal_sad, channel.internal_dad, cnt, channel.is_32bit(), channel.src_step(), channel.dest_step())
        };
    
        // 2. 執行搬運
        for _ in 0..count {
            if is_32bit {
                let val = self.read_u32(src);
                self.write_u32(dst, val);
            } else {
                // 重要：GBA 很多 Tile 搬運是 16-bit
                let val = self.read_u16(src);
                self.write_u16(dst, val);
            }
            src = (src as i32 + s_step) as u32;
            dst = (dst as i32 + d_step) as u32;
        }
    
        // 3. 寫回更新後的地址到內部暫存器 (為了 Repeat 模式)
        {
            let channel = &mut self.dma[ch];
            channel.internal_sad = src;
            channel.internal_dad = dst;
    
            // 如果不是 Repeat 模式 (bit 9 == 0)，關閉 DMA
            if (channel.ctrl & 0x0200) == 0 {
                channel.enabled = false;
                let io_addr = 0x0BA + (ch * 12);
                self.io[io_addr + 1] &= 0x7F; // 清除 Enable 位元
            }
        }
    }
}
