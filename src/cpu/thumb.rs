use super::Cpu;
use crate::memory::Bus;

impl Cpu {
    fn execute_thumb_pop(&mut self, bus: &mut Bus, instruction: u16) {
        let include_pc = (instruction & 0x0100) != 0;
        let rlist = instruction & 0x00FF;

        let mut addr = self.regs[13];
        for i in 0..8 {
            if (rlist & (1 << i)) != 0 {
                self.regs[i] = bus.read_u32(addr);
                addr += 4;
            }
        }

        if include_pc {
            let pc_val = bus.read_u32(addr);
            addr += 4;
            self.regs[15] = pc_val & !1;
            self.thumb_mode = (pc_val & 1) != 0;
        }

        self.regs[13] = addr;
    }

    fn execute_thumb_push(&mut self, bus: &mut Bus, instruction: u16) {
        let include_lr = (instruction & 0x0100) != 0;
        let rlist = instruction & 0x00FF;

        let mut count = 0;
        for i in 0..8 {
            if (rlist & (1 << i)) != 0 {
                count += 1;
            }
        }
        if include_lr {
            count += 1;
        }

        let start_addr = self.regs[13].wrapping_sub(count * 4);
        let mut offset = 0;
        for i in 0..8 {
            if (rlist & (1 << i)) != 0 {
                bus.write_u32(start_addr + offset, self.regs[i]);
                offset += 4;
            }
        }
        if include_lr {
            bus.write_u32(start_addr + offset, self.regs[14]);
        }

        self.regs[13] = start_addr;
    }

    fn execute_thumb_sp_relative_load_store(&mut self, bus: &mut Bus, instruction: u16) {
        let is_load = (instruction & 0x0800) != 0;
        let rd = ((instruction >> 8) & 0x07) as usize;
        let addr = self.regs[13].wrapping_add(((instruction & 0xFF) as u32) << 2);

        if is_load {
            self.regs[rd] = bus.read_u32(addr & !3);
        } else {
            bus.write_u32(addr & !3, self.regs[rd]);
        }
    }

    fn execute_thumb_add_sp_offset(&mut self, instruction: u16) {
        let is_sub = (instruction & 0x0080) != 0;
        let offset = ((instruction & 0x007F) as u32) << 2;

        if is_sub {
            self.regs[13] = self.regs[13].wrapping_sub(offset);
        } else {
            self.regs[13] = self.regs[13].wrapping_add(offset);
        }
    }

    pub fn execute_thumb(&mut self, bus: &mut Bus, instruction: u16, current_pc: u32) {
        let valid_bios = current_pc < 0x00004000;
        let valid_mem = current_pc >= 0x02000000 && current_pc < 0x04000000;
        let valid_rom = current_pc >= 0x08000000 && current_pc <= 0x0E000000;

        if !(valid_bios || valid_mem || valid_rom) {
            println!(
                "CRASH WARNING: Invalid THUMB PC: {:08X}, Instruction: {:04X}",
                current_pc, instruction
            );
            self.halted = true;
            return;
        }

        let pc_plus_4 = current_pc.wrapping_add(4);

        if (instruction & 0xFE00) == 0xBC00 {
            self.execute_thumb_pop(bus, instruction);
            return;
        }

        if (instruction & 0xFE00) == 0xB400 {
            let is_pop = (instruction & 0x0800) != 0;

            if !is_pop {
                self.execute_thumb_push(bus, instruction);
                return;
            }
        }

        if (instruction & 0xFC00) == 0x4000 {
            let op = (instruction >> 6) & 0x0F;
            let rs = ((instruction >> 3) & 0x07) as usize;
            let rd = (instruction & 0x07) as usize;

            let val_s = self.regs[rs];
            let val_d = self.regs[rd];

            match op {
                0 => {
                    self.regs[rd] &= val_s;
                    self.set_flag_n((self.regs[rd] & 0x8000_0000) != 0);
                    self.set_flag_z(self.regs[rd] == 0);
                }
                1 => {
                    self.regs[rd] ^= val_s;
                    self.set_flag_n((self.regs[rd] & 0x8000_0000) != 0);
                    self.set_flag_z(self.regs[rd] == 0);
                }
                2 => {
                    let shift = val_s & 0xFF;
                    if shift > 0 {
                        if shift >= 32 {
                            self.regs[rd] = 0;
                            self.set_flag_c(if shift == 32 { (val_d & 1) != 0 } else { false });
                        } else {
                            self.set_flag_c(((val_d >> (32 - shift)) & 1) != 0);
                            self.regs[rd] = val_d << shift;
                        }
                    }
                    self.set_flag_n((self.regs[rd] & 0x8000_0000) != 0);
                    self.set_flag_z(self.regs[rd] == 0);
                }
                3 => {
                    let shift = val_s & 0xFF;
                    if shift > 0 {
                        if shift >= 32 {
                            self.set_flag_c(if shift == 32 {
                                (val_d & 0x8000_0000) != 0
                            } else {
                                false
                            });
                            self.regs[rd] = 0;
                        } else {
                            self.set_flag_c(((val_d >> (shift - 1)) & 1) != 0);
                            self.regs[rd] = val_d >> shift;
                        }
                    }
                    self.set_flag_n((self.regs[rd] & 0x8000_0000) != 0);
                    self.set_flag_z(self.regs[rd] == 0);
                }
                4 => {
                    let shift = val_s & 0xFF;
                    if shift > 0 {
                        if shift >= 32 {
                            let bit31 = (val_d & 0x8000_0000) != 0;
                            self.regs[rd] = if bit31 { 0xFFFFFFFF } else { 0 };
                            self.set_flag_c(bit31);
                        } else {
                            self.set_flag_c(((val_d >> (shift - 1)) & 1) != 0);
                            self.regs[rd] = ((val_d as i32) >> shift) as u32;
                        }
                    }
                    self.set_flag_n((self.regs[rd] & 0x8000_0000) != 0);
                    self.set_flag_z(self.regs[rd] == 0);
                }
                5 => {
                    let c = if self.get_flag_c() { 1 } else { 0 };
                    let res = (val_d as u64) + (val_s as u64) + c;
                    self.regs[rd] = res as u32;
                    self.set_flag_n((self.regs[rd] & 0x8000_0000) != 0);
                    self.set_flag_z(self.regs[rd] == 0);
                    self.set_flag_c(res > 0xFFFFFFFF);
                    self.set_flag_v(
                        (!(val_d ^ val_s) & (val_d ^ self.regs[rd]) & 0x8000_0000) != 0,
                    );
                }
                6 => {
                    let c = if self.get_flag_c() { 1 } else { 0 };
                    let res = (val_d as u64)
                        .wrapping_sub(val_s as u64)
                        .wrapping_sub(1 - c);
                    self.regs[rd] = res as u32;
                    self.set_flag_n((self.regs[rd] & 0x8000_0000) != 0);
                    self.set_flag_z(self.regs[rd] == 0);
                    self.set_flag_c(res < 0x1_0000_0000);
                    self.set_flag_v(((val_d ^ val_s) & (val_d ^ self.regs[rd]) & 0x8000_0000) != 0);
                }
                7 => {
                    let shift = val_s & 0xFF;
                    if shift > 0 {
                        let shift = shift % 32;
                        if shift == 0 {
                            self.set_flag_c((val_d & 0x8000_0000) != 0);
                        } else {
                            self.set_flag_c(((val_d >> (shift - 1)) & 1) != 0);
                            self.regs[rd] = val_d.rotate_right(shift);
                        }
                    }
                    self.set_flag_n((self.regs[rd] & 0x8000_0000) != 0);
                    self.set_flag_z(self.regs[rd] == 0);
                }
                8 => {
                    let res = val_d & val_s;
                    self.set_flag_n((res & 0x8000_0000) != 0);
                    self.set_flag_z(res == 0);
                }
                9 => {
                    let res = 0u32.wrapping_sub(val_s);
                    self.regs[rd] = res;
                    self.set_flag_n((res & 0x8000_0000) != 0);
                    self.set_flag_z(res == 0);
                    self.set_flag_c(0 >= val_s);
                    self.set_flag_v(((0 ^ val_s) & (0 ^ res) & 0x8000_0000) != 0);
                }
                10 => {
                    let result = val_d.wrapping_sub(val_s);
                    self.set_flag_n((result & 0x8000_0000) != 0);
                    self.set_flag_z(result == 0);
                    self.set_flag_c(val_d >= val_s);
                    let overflow = ((val_d ^ val_s) & (val_d ^ result) & 0x8000_0000) != 0;
                    self.set_flag_v(overflow);
                }
                11 => {
                    let res = (val_d as u64) + (val_s as u64);
                    self.set_flag_n(((res as u32) & 0x8000_0000) != 0);
                    self.set_flag_z((res as u32) == 0);
                    self.set_flag_c(res > 0xFFFFFFFF);
                    self.set_flag_v((!(val_d ^ val_s) & (val_d ^ (res as u32)) & 0x8000_0000) != 0);
                }
                12 => {
                    self.regs[rd] |= val_s;
                    self.set_flag_n((self.regs[rd] & 0x8000_0000) != 0);
                    self.set_flag_z(self.regs[rd] == 0);
                }
                13 => {
                    let res = val_d.wrapping_mul(val_s);
                    self.regs[rd] = res;
                    self.set_flag_n((res & 0x8000_0000) != 0);
                    self.set_flag_z(res == 0);
                }
                14 => {
                    self.regs[rd] &= !val_s;
                    self.set_flag_n((self.regs[rd] & 0x8000_0000) != 0);
                    self.set_flag_z(self.regs[rd] == 0);
                }
                15 => {
                    self.regs[rd] = !val_s;
                    self.set_flag_n((self.regs[rd] & 0x8000_0000) != 0);
                    self.set_flag_z(self.regs[rd] == 0);
                }
                _ => {}
            }
            return;
        }

        if (instruction & 0xF000) == 0x9000 {
            self.execute_thumb_sp_relative_load_store(bus, instruction);
            return;
        }

        if (instruction & 0xFF00) == 0xB000 {
            self.execute_thumb_add_sp_offset(instruction);
            return;
        }

        if (instruction & 0xF800) == 0x1800 {
            let rd = (instruction & 0x07) as usize;
            let rs = ((instruction >> 3) & 0x07) as usize;
            let is_sub = (instruction & (1 << 9)) != 0;
            let is_imm = (instruction & (1 << 10)) != 0;
            let rn_num = (instruction >> 6) & 0x07;

            let op1 = self.regs[rs];
            let op2 = if is_imm {
                rn_num as u32
            } else {
                self.regs[rn_num as usize]
            };

            if is_sub {
                let result = op1.wrapping_sub(op2);
                self.regs[rd] = result;
                self.set_flag_n((result & (1 << 31)) != 0);
                self.set_flag_z(result == 0);
                self.set_flag_c(op1 >= op2);
                self.set_flag_v(((op1 ^ op2) & (op1 ^ result) & 0x80000000) != 0);
            } else {
                let result = op1.wrapping_add(op2);
                self.regs[rd] = result;
                self.set_flag_n((result & (1 << 31)) != 0);
                self.set_flag_z(result == 0);
                self.set_flag_c((op1 as u64 + op2 as u64) > 0xFFFFFFFF);
                self.set_flag_v((!(op1 ^ op2) & (op1 ^ result) & 0x80000000) != 0);
            }
            return;
        }

        if (instruction & 0xE000) == 0x2000 {
            let op = (instruction >> 11) & 0x03;
            let rd = ((instruction >> 8) & 0x07) as usize;
            let imm8 = (instruction & 0xFF) as u32;

            let op1 = self.regs[rd];

            match op {
                0b00 => {
                    self.regs[rd] = imm8;
                    self.set_flag_n(false);
                    self.set_flag_z(imm8 == 0);
                }
                0b01 => {
                    let result = op1.wrapping_sub(imm8);
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                    self.set_flag_c(op1 >= imm8);
                    self.set_flag_v(((op1 ^ imm8) & (op1 ^ result) & 0x80000000) != 0);
                }
                0b10 => {
                    let result = op1.wrapping_add(imm8);
                    self.regs[rd] = result;
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                    self.set_flag_c((op1 as u64 + imm8 as u64) > 0xFFFFFFFF);
                    self.set_flag_v((!(op1 ^ imm8) & (op1 ^ result) & 0x80000000) != 0);
                }
                0b11 => {
                    let result = op1.wrapping_sub(imm8);
                    self.regs[rd] = result;
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                    self.set_flag_c(op1 >= imm8);
                    self.set_flag_v(((op1 ^ imm8) & (op1 ^ result) & 0x80000000) != 0);
                }
                _ => unreachable!(),
            }
            return;
        }

        // 檔案中約第 112 行
        if (instruction & 0xF800) == 0x4800 {
            let rd = ((instruction >> 8) & 0x07) as usize;
            let word8 = (instruction & 0xFF) as u32;
            let addr = (pc_plus_4 & !3).wrapping_add(word8 << 2);
            self.regs[rd] = bus.read_u32(addr);
            return;
        }

        if (instruction & 0xF000) == 0xF000 {
            let offset_11 = (instruction & 0x07FF) as u32;
            if (instruction & 0x0800) == 0 {
                let sign_extend = if (offset_11 & 0x0400) != 0 {
                    (0xFFFFF800 | offset_11) << 12
                } else {
                    offset_11 << 12
                };
                let pc_plus_4 = self.regs[15].wrapping_add(2);
                self.regs[14] = pc_plus_4.wrapping_add(sign_extend);
            } else {
                let offset = offset_11 << 1;
                let target = self.regs[14].wrapping_add(offset);
                self.regs[14] = self.regs[15] | 1;
                self.regs[15] = target & !1;
            }
            return;
        }

        if (instruction & 0xFF00) == 0xDF00 {
            let swi_number = (instruction & 0xFF) as u32;
            self.handle_swi(bus, swi_number, current_pc);
            return;
        }

        if (instruction & 0xF000) == 0xD000 {
            let cond = (instruction >> 8) & 0x0F;
            if cond == 0x0F {
                return;
            }

            let mut offset = (instruction & 0xFF) as i32;
            if (offset & 0x80) != 0 {
                offset |= !0xFF;
            }
            let branch_offset = offset << 1;

            if self.check_cond(cond as u32) {
                let target_addr = (pc_plus_4 as i32).wrapping_add(branch_offset) as u32;
                self.regs[15] = target_addr;
            }
            return;
        }

        if (instruction & 0xFF80) == 0x4700 {
            let rm = ((instruction >> 3) & 0xF) as usize;
            let target = self.regs[rm];
            self.thumb_mode = (target & 1) != 0;
            self.regs[15] = target & !1;
            return;
        }

        if (instruction & 0xFF87) == 0x4700 {
            let rm = ((instruction >> 3) & 0x0F) as usize;
            let target_addr = if rm == 15 { pc_plus_4 } else { self.regs[rm] };
            if (target_addr & 1) != 0 {
                self.thumb_mode = true;
                self.regs[15] = target_addr & !1;
            } else {
                self.thumb_mode = false;
                self.regs[15] = target_addr & !3;
            }
            return;
        }

        if (instruction & 0xE000) == 0x0000 && (instruction & 0x1800) != 0x1800 {
            let op = (instruction >> 11) & 0x03;
            let offset = ((instruction >> 6) & 0x1F) as u32;
            let rs = ((instruction >> 3) & 0x07) as usize;
            let rd = (instruction & 0x07) as usize;
            let val = self.regs[rs];
            let result = match op {
                0b00 => {
                    let shift = offset;
                    if shift == 0 {
                        val
                    } else {
                        let res = val << shift;
                        self.set_flag_c((val & (1 << (32 - shift))) != 0);
                        res
                    }
                }
                0b01 => {
                    let shift = if offset == 0 { 32 } else { offset };
                    if shift == 32 {
                        self.set_flag_c((val & 0x80000000) != 0);
                        0
                    } else {
                        let res = val >> shift;
                        self.set_flag_c((val & (1 << (shift - 1))) != 0);
                        res
                    }
                }
                0b10 => {
                    let shift = if offset == 0 { 32 } else { offset };
                    if shift == 32 {
                        let bit31 = (val & 0x80000000) != 0;
                        self.set_flag_c(bit31);
                        if bit31 { 0xFFFFFFFF } else { 0 }
                    } else {
                        let res = ((val as i32) >> shift) as u32;
                        self.set_flag_c((val & (1 << (shift - 1))) != 0);
                        res
                    }
                }
                _ => val,
            };
            self.regs[rd] = result;
            self.set_flag_n((result & 0x80000000) != 0);
            self.set_flag_z(result == 0);
            return;
        }

        if (instruction & 0xF800) == 0x1800 {
            let op = (instruction >> 9) & 0x01;
            let is_imm = (instruction >> 10) & 0x01;
            let rn = ((instruction >> 3) & 0x07) as usize;
            let rd = (instruction & 0x07) as usize;
            let op1 = self.regs[rn];
            let op2 = if is_imm != 0 {
                ((instruction >> 6) & 0x07) as u32
            } else {
                self.regs[((instruction >> 6) & 0x07) as usize]
            };

            let result = if op == 0 {
                let res = op1.wrapping_add(op2);
                self.set_flag_c((op1 as u64 + op2 as u64) > 0xFFFFFFFF);
                self.set_flag_v((!(op1 ^ op2) & (op1 ^ res) & 0x80000000) != 0);
                res
            } else {
                let res = op1.wrapping_sub(op2);
                self.set_flag_c(op1 >= op2);
                self.set_flag_v(((op1 ^ op2) & (op1 ^ res) & 0x80000000) != 0);
                res
            };
            self.regs[rd] = result;
            self.set_flag_n((result & 0x80000000) != 0);
            self.set_flag_z(result == 0);
            return;
        }

        if (instruction & 0xFC00) == 0x4400 {
            let op = (instruction >> 8) & 0x03;
            let h1 = (instruction >> 7) & 0x01;
            let h2 = (instruction >> 6) & 0x01;
            let rs = (((instruction >> 3) & 0x07) | (h2 << 3)) as usize;
            let rd = ((instruction & 0x07) | (h1 << 3)) as usize;

            match op {
                0b00 => {
                    let op1 = if rd == 15 { pc_plus_4 } else { self.regs[rd] };
                    let op2 = if rs == 15 { pc_plus_4 } else { self.regs[rs] };
                    let res = op1.wrapping_add(op2);
                    if rd == 15 {
                        self.regs[15] = res & !1;
                    } else {
                        self.regs[rd] = res;
                    }
                }
                0b01 => {
                    let op1 = if rd == 15 { pc_plus_4 } else { self.regs[rd] };
                    let op2 = if rs == 15 { pc_plus_4 } else { self.regs[rs] };
                    let res = op1.wrapping_sub(op2);
                    self.set_flag_n((res & 0x80000000) != 0);
                    self.set_flag_z(res == 0);
                    self.set_flag_c(op1 >= op2);
                    self.set_flag_v(((op1 ^ op2) & (op1 ^ res) & 0x80000000) != 0);
                }
                0b10 => {
                    let op2 = if rs == 15 { pc_plus_4 } else { self.regs[rs] };
                    if rd == 15 {
                        self.regs[15] = op2 & !1;
                    } else {
                        self.regs[rd] = op2;
                    }
                }
                0b11 => {
                    let target_addr = if rs == 15 { pc_plus_4 } else { self.regs[rs] };
                    if (target_addr & 1) != 0 {
                        self.thumb_mode = true;
                        self.regs[15] = target_addr & !1;
                    } else {
                        self.thumb_mode = false;
                        self.regs[15] = target_addr & !3;
                    }
                }
                _ => {}
            }
            return;
        }

        if (instruction & 0xF200) == 0x5000 {
            let l = (instruction >> 11) & 0x01;
            let b = (instruction >> 10) & 0x01;
            let ro = ((instruction >> 6) & 0x07) as usize;
            let rb = ((instruction >> 3) & 0x07) as usize;
            let rd = (instruction & 0x07) as usize;
            let addr = self.regs[rb].wrapping_add(self.regs[ro]);

            if l == 0 {
                if b == 0 {
                    bus.write_u32(addr & !3, self.regs[rd]);
                } else {
                    bus.write_u8(addr, (self.regs[rd] & 0xFF) as u8);
                }
            } else if b == 0 {
                let mut val = bus.read_u32(addr & !3);
                let align = addr & 3;
                if align != 0 {
                    val = val.rotate_right((align * 8) as u32);
                }
                self.regs[rd] = val;
            } else {
                self.regs[rd] = bus.read_u8(addr) as u32;
            }
            return;
        }

        if (instruction & 0xE000) == 0x6000 {
            let b = (instruction >> 12) & 0x01;
            let l = (instruction >> 11) & 0x01;
            let offset = ((instruction >> 6) & 0x1F) as u32;
            let rb = ((instruction >> 3) & 0x07) as usize;
            let rd = (instruction & 0x07) as usize;

            let addr = if b == 0 {
                self.regs[rb].wrapping_add(offset << 2)
            } else {
                self.regs[rb].wrapping_add(offset)
            };

            if l == 0 {
                if b == 0 {
                    bus.write_u32(addr & !3, self.regs[rd]);
                } else {
                    bus.write_u8(addr, (self.regs[rd] & 0xFF) as u8);
                }
            } else if b == 0 {
                let mut val = bus.read_u32(addr & !3);
                let align = addr & 3;
                if align != 0 {
                    val = val.rotate_right((align * 8) as u32);
                }
                self.regs[rd] = val;
            } else {
                self.regs[rd] = bus.read_u8(addr) as u32;
            }
            return;
        }

        if (instruction & 0xF800) == 0xE000 {
            let offset_11 = (instruction & 0x07FF) as u32;
            let sign_extend = if (offset_11 & 0x0400) != 0 {
                (0xFFFFF800 | offset_11) << 1
            } else {
                offset_11 << 1
            };
            self.regs[15] = pc_plus_4.wrapping_add(sign_extend);
            return;
        }

        if (instruction & 0xF000) == 0x8000 {
            let l = (instruction >> 11) & 0x01;
            let offset = ((instruction >> 6) & 0x1F) as u32;
            let rb = ((instruction >> 3) & 0x07) as usize;
            let rd = (instruction & 0x07) as usize;
            let addr = self.regs[rb].wrapping_add(offset << 1);

            if l == 0 {
                bus.write_u16(addr & !1, (self.regs[rd] & 0xFFFF) as u16);
            } else {
                let val = bus.read_u16(addr & !1) as u32;
                self.regs[rd] = val;
            }
            return;
        }

        if (instruction & 0xF200) == 0x5200 {
            let h = (instruction >> 11) & 0x01;
            let s = (instruction >> 10) & 0x01;
            let ro = ((instruction >> 6) & 0x07) as usize;
            let rb = ((instruction >> 3) & 0x07) as usize;
            let rd = (instruction & 0x07) as usize;

            let addr = self.regs[rb].wrapping_add(self.regs[ro]);

            match (s, h) {
                (0, 0) => {
                    bus.write_u16(addr & !1, (self.regs[rd] & 0xFFFF) as u16);
                }
                (0, 1) => {
                    self.regs[rd] = bus.read_u16(addr & !1) as u32;
                }
                (1, 0) => {
                    let val = bus.read_u8(addr) as i8;
                    self.regs[rd] = val as i32 as u32;
                }
                (1, 1) => {
                    let val = bus.read_u16(addr & !1) as i16;
                    if (addr & 1) != 0 {
                        let b = bus.read_u8(addr) as i8;
                        self.regs[rd] = b as i32 as u32;
                    } else {
                        self.regs[rd] = val as i32 as u32;
                    }
                }
                _ => {}
            }
            return;
        }

        // 檔案末尾處
        if (instruction & 0xF000) == 0xA000 {
            let rd = ((instruction >> 8) & 0x07) as usize;
            let sp_bit = (instruction & 0x0800) != 0;
            let word8 = (instruction & 0xFF) as u32;

            if sp_bit {
                self.regs[rd] = self.regs[13].wrapping_add(word8 << 2);
            } else {
                // 錯誤：pc_plus_4 & !2
                // 正確：必須 & !3
                let pc = pc_plus_4 & !3;
                self.regs[rd] = pc.wrapping_add(word8 << 2);
            }
            return;
        }

        if (instruction & 0xF000) == 0xC000 {
            let is_load = (instruction & (1 << 11)) != 0;
            let rn = ((instruction >> 8) & 0x07) as usize;
            let rlist = instruction & 0xFF;
            let mut addr = self.regs[rn];

            if is_load {
                for i in 0..8 {
                    if (rlist & (1 << i)) != 0 {
                        self.regs[i] = bus.read_u32(addr & !3);
                        addr += 4;
                    }
                }
                if (rlist & (1 << rn)) == 0 {
                    self.regs[rn] = addr;
                }
            } else {
                let mut is_first = true;
                for i in 0..8 {
                    if (rlist & (1 << i)) != 0 {
                        let value_to_store = if i == rn && !is_first {
                            addr
                        } else {
                            self.regs[i]
                        };
                        bus.write_u32(addr & !3, value_to_store);
                        addr += 4;
                        is_first = false;
                    }
                }
                self.regs[rn] = addr;
            }
        }
    }
}
