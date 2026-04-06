use crate::memory::Bus;

mod arm;
mod instructions;
mod thumb;

pub struct Cpu {
    pub regs: [u32; 16],
    pub cpsr: u32,
    pub thumb_mode: bool,

    //// 銀行寄存器，用於不同模式下的切換
    pub banked_r8_12: [u32; 5],
    pub banked_r13: [u32; 6],
    pub banked_r14: [u32; 6],
    pub banked_spsr: [u32; 6],

    pub halted: bool,
    pub trace_log: Vec<String>,
}

impl Cpu {
    const DEFAULT_STEP_CYCLES: usize = 4;

    pub fn new() -> Self {
        let mut cpu = Self {
            regs: [0; 16],
            cpsr: 0x13, // 初始通常為 SVC 模式
            thumb_mode: false,
            banked_r8_12: [0; 5],
            banked_r13: [0; 6],
            banked_r14: [0; 6],
            banked_spsr: [0; 6],
            halted: false,
            trace_log: Vec::with_capacity(1000),
        };
        cpu.reset();
        cpu
    }

    pub fn clear_state(&mut self) {
        self.regs = [0; 16];
        self.cpsr = 0x13; // 初始通常為 SVC 模式
        self.thumb_mode = false;
        self.banked_r8_12 = [0; 5];
        self.banked_r13 = [0; 6];
        self.banked_r14 = [0; 6];
        self.banked_spsr = [0; 6];
        self.halted = false;
        self.trace_log.clear();
    }

    pub fn reset(&mut self) {
        self.regs[15] = 0x08000000; // 從 ROM 入口點開始
        self.cpsr = 0x13;
        self.thumb_mode = false;
        self.halted = false;
    }

    pub fn pc(&self) -> u32 {
        self.regs[15]
    }

    pub fn get_mode_index(&self, cpsr: u32) -> usize {
        match cpsr & 0x1F {
            0b10000 | 0b11111 => 0,
            0b10001 => 1,
            0b10010 => 2,
            0b10011 => 3,
            0b10111 => 4,
            0b11011 => 5,
            _ => 0,
        }
    }

    pub fn current_mode(&self) -> u32 {
        self.cpsr & 0x1F
    }

    pub fn set_reg(&mut self, index: usize, value: u32) {
        self.regs[index] = value;
    }

    pub fn set_pc(&mut self, value: u32) {
        self.regs[15] = if self.thumb_mode {
            value & !1
        } else {
            value & !3
        };
    }

    pub fn set_mode_stack_pointer(&mut self, mode: u32, value: u32) {
        let mode_idx = self.get_mode_index(mode);
        self.banked_r13[mode_idx] = value;

        if self.get_mode_index(self.cpsr) == mode_idx {
            self.regs[13] = value;
        }
    }

    fn swap_fiq_register_bank(&mut self) {
        for i in 0..5 {
            std::mem::swap(&mut self.regs[8 + i], &mut self.banked_r8_12[i]);
        }
    }

    pub fn set_cpsr(&mut self, new_cpsr: u32) {
        self.change_mode(self.cpsr, new_cpsr);
        self.cpsr = new_cpsr;
    }

    pub fn change_mode(&mut self, old_cpsr: u32, new_cpsr: u32) {
        let old_idx = self.get_mode_index(old_cpsr);
        let new_idx = self.get_mode_index(new_cpsr);

        if old_idx != new_idx {
            if old_idx == 1 || new_idx == 1 {
                self.swap_fiq_register_bank();
            }

            self.banked_r13[old_idx] = self.regs[13];
            self.banked_r14[old_idx] = self.regs[14];

            self.regs[13] = self.banked_r13[new_idx];
            self.regs[14] = self.banked_r14[new_idx];
        }
    }

    // 旗標在 CPSR 中的位置：N(31), Z(30), C(29), V(28)
    pub fn get_flag_n(&self) -> bool {
        (self.cpsr >> 31) != 0
    }
    pub fn get_flag_z(&self) -> bool {
        (self.cpsr >> 30 & 1) != 0
    }
    pub fn get_flag_c(&self) -> bool {
        (self.cpsr >> 29 & 1) != 0
    }
    pub fn get_flag_v(&self) -> bool {
        (self.cpsr >> 28 & 1) != 0
    }

    pub fn set_flag_n(&mut self, val: bool) {
        if val {
            self.cpsr |= 1 << 31
        } else {
            self.cpsr &= !(1 << 31)
        }
    }
    pub fn set_flag_z(&mut self, val: bool) {
        if val {
            self.cpsr |= 1 << 30
        } else {
            self.cpsr &= !(1 << 30)
        }
    }
    pub fn set_flag_c(&mut self, val: bool) {
        if val {
            self.cpsr |= 1 << 29
        } else {
            self.cpsr &= !(1 << 29)
        }
    }
    pub fn set_flag_v(&mut self, val: bool) {
        if val {
            self.cpsr |= 1 << 28
        } else {
            self.cpsr &= !(1 << 28)
        }
    }

    pub fn check_cond(&self, cond: u32) -> bool {
        match cond {
            0x0 => self.get_flag_z(),
            0x1 => !self.get_flag_z(),
            0x2 => self.get_flag_c(),
            0x3 => !self.get_flag_c(),
            0x4 => self.get_flag_n(),
            0x5 => !self.get_flag_n(),
            0x6 => self.get_flag_v(),
            0x7 => !self.get_flag_v(),
            0x8 => self.get_flag_c() && !self.get_flag_z(),
            0x9 => !self.get_flag_c() || self.get_flag_z(),
            0xA => self.get_flag_n() == self.get_flag_v(),
            0xB => self.get_flag_n() != self.get_flag_v(),
            0xC => !self.get_flag_z() && (self.get_flag_n() == self.get_flag_v()),
            0xD => self.get_flag_z() || (self.get_flag_n() != self.get_flag_v()),
            0xE => true,
            _ => true,
        }
    }

    pub fn step(&mut self, bus: &mut Bus) -> usize {
        self.check_interrupts(bus);

        if self.halted {
            return 1;
        }

        let current_pc = self.regs[15];

        if self.thumb_mode {
            // Thumb 模式：讀取 16-bit
            let instruction = bus.read_u16(current_pc);
            // GBA 流水線行為：PC 在執行時通常指向 PC+4 (但在模擬器中我們習慣先加 2)
            self.regs[15] = current_pc.wrapping_add(2);
            self.execute_thumb(bus, instruction, current_pc);
        } else {
            // ARM 模式：讀取 32-bit
            let instruction = bus.read_u32(current_pc);
            // ARM 流水線 PC 通常指向 PC+8
            self.regs[15] = current_pc.wrapping_add(4);
            self.execute_arm(bus, instruction, current_pc);
        }

        Self::DEFAULT_STEP_CYCLES
    }

    pub fn check_interrupts(&mut self, bus: &mut Bus) {
        let pending_interrupts = bus.pending_interrupts();

        if pending_interrupts != 0 {
            self.halted = false;
        }

        if bus.interrupt_master_enable() && pending_interrupts != 0 && (self.cpsr & 0x80) == 0 {
            let old_cpsr = self.cpsr;
            let new_cpsr = (old_cpsr & !0x1F) | 0x12 | 0x80;
            self.set_cpsr(new_cpsr);

            let irq_idx = self.get_mode_index(0x12);
            let mut spsr_val = old_cpsr;
            if self.thumb_mode {
                spsr_val |= 0x20;
            } else {
                spsr_val &= !0x20;
            }
            self.banked_spsr[irq_idx] = spsr_val;

            self.regs[14] = self.regs[15].wrapping_add(4);

            self.thumb_mode = false;
            self.regs[15] = 0x00000018;
        }
    }

    pub fn handle_swi(&mut self, bus: &mut Bus, swi_number: u32, current_pc: u32) {
        let raw_swi_number = swi_number;
        let swi_number = if self.thumb_mode {
            raw_swi_number & 0xFF
        } else if raw_swi_number > 0xFF {
            (raw_swi_number >> 16) & 0xFF
        } else {
            raw_swi_number & 0xFF
        };

        if swi_number != 0x05 {
            println!(
                "[HLE SWI] called SWI {:#04X} (raw {:#08X}) at PC {:#010X} (R0: {:#X}, R1: {:#X}, R2: {:#X})",
                swi_number,
                raw_swi_number,
                self.pc(),
                self.regs[0],
                self.regs[1],
                self.regs[2]
            );
        }
        match swi_number {
            0x01 => {
                let _flags = self.regs[0];
            }
            0x02 => {
                self.halted = true;
            }
            0x04 | 0x05 => {
                let wait_flags = if swi_number == 0x05 {
                    0x0001
                } else {
                    self.regs[1] as u16
                };

                let intr_flags = bus.read_u16(0x03007FF8);
                if (intr_flags & wait_flags) != 0 {
                    bus.write_u16(0x03007FF8, intr_flags & !wait_flags);
                    self.halted = false;
                } else {
                    self.halted = true;
                    self.regs[15] = current_pc;
                }
            }
            0x08 => {
                self.regs[0] = (self.regs[0] as f64).sqrt() as u32;
            }
            0x0B => {
                let mut src = self.regs[0];
                let mut dst = self.regs[1];
                let ctrl = self.regs[2];
                let count = ctrl & 0x1FFFFF;
                let fixed_src = (ctrl & (1 << 24)) != 0;
                let is_32bit = (ctrl & (1 << 26)) != 0;

                let step = if is_32bit { 4 } else { 2 };
                for _ in 0..count {
                    if is_32bit {
                        let val = bus.read_u32(src & !3);
                        bus.write_u32(dst & !3, val);
                    } else {
                        let val = bus.read_u16(src & !1);
                        bus.write_u16(dst & !1, val);
                    }
                    if !fixed_src {
                        src = src.wrapping_add(step);
                    }
                    dst = dst.wrapping_add(step);
                }
            }
            0x0C => {
                let src = self.regs[0] & !3;
                let mut dst = self.regs[1] & !3;
                let ctrl = self.regs[2];
                let blocks = ctrl & 0x1FFFFF;
                let fixed_src = (ctrl & (1 << 24)) != 0;
                let total_words = blocks * 8;

                if total_words == 0 {
                    return;
                }

                if fixed_src {
                    let fill = bus.read_u32(src);
                    for _ in 0..total_words {
                        bus.write_u32(dst, fill);
                        dst = dst.wrapping_add(4);
                    }
                } else {
                    let mut current_src = src;
                    for _ in 0..total_words {
                        let val = bus.read_u32(current_src);
                        bus.write_u32(dst, val);
                        current_src = current_src.wrapping_add(4);
                        dst = dst.wrapping_add(4);
                    }
                }
            }
            0x11 | 0x12 => {
                let src = self.regs[0];
                let dst = self.regs[1];

                if swi_number == 0x12 {
                    crate::memory::lz77_decompress_vram(bus, src, dst);
                } else {
                    crate::memory::lz77_decompress_wram(bus, src, dst);
                }
            }
            _ => {
                println!("[HLE SWI] Unhandled SWI: 0x{:02X}", swi_number);
            }
        }
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}
