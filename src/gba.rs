use crate::bypass_bios::bypass_bios;
use crate::cpu::Cpu;
use crate::memory::Bus;

/// `Gba` 是整台遊戲機的實體，擁有硬體的一切
pub struct Gba {
    pub cpu: Cpu,
    pub bus: Bus,
    // 預留位置未來加入：
    // pub ppu: Ppu,       // 圖形處理
    // pub apu: Apu,       // 音效處理
    // pub keypad: Keypad, // 取樣手把控制器
}

impl Gba {
    pub fn new(rom_data: Vec<u8>) -> Self {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new(rom_data);

        // 我們實作了 HLE 所以一律跳過 BIOS 並設定寄存器
        bypass_bios(&mut cpu, &mut bus);

        Self { cpu, bus }
    }

    /// 執行一個模擬主迴圈 (通常會是 一個 Frame 或 一次 CPU step)
    pub fn step(&mut self) {
        self.bus.process_dma();

        // 記錄時脈推進前的 Scanline
        let old_scanline = self.bus.read_u16(0x04000006);

        // 先讓 CPU 跑一個週期指令
        self.cpu.trace_log.push(format!("{:08X}", self.cpu.pc()));
        if self.cpu.trace_log.len() > 1000 {
            self.cpu.trace_log.remove(0);
        }
        let cpu_cycles = self.cpu.step(&mut self.bus);

        // 模擬時脈流逝與更新硬體狀態
        self.bus.tick(cpu_cycles);

        // 記錄推進後的 Scanline
        let new_scanline = self.bus.read_u16(0x04000006);

        // 如果進入新的 Scanline，呼叫 Ppu 渲染這條線
        if old_scanline != new_scanline {
            let mut ppu = std::mem::take(&mut self.bus.ppu);
            ppu.render_scanline(&self.bus, new_scanline);
            if new_scanline == 160 && ppu.frame_count == 8 {
                let dispcnt = self.bus.read_u16(0x0400_0000);
                if dispcnt == 0x0080 {
                    let trace_len = self.cpu.trace_log.len();
                    let start = trace_len.saturating_sub(32);
                    println!(
                        "[DEBUG] DISPCNT still 0x0080 at frame {}. Recent PCs:",
                        ppu.frame_count
                    );
                    for pc in &self.cpu.trace_log[start..] {
                        println!("[DEBUG]   {}", pc);
                    }
                }
            }
            if new_scanline == 160 && ppu.frame_count % 30 == 0 {
                println!(
                    "[GBA] Frame {} ended. PC: {:#010x}",
                    ppu.frame_count,
                    self.cpu.pc()
                );
            }
            self.bus.ppu = ppu;
        }
    }
}
