use super::Cpu;
use crate::memory::Bus;

#[derive(Clone, Copy)]
enum ArmInstructionClass {
    DataProcessing,
    LoadStore,
    BlockTransfer,
    Branch,
    BranchExchange,
    SoftwareInterrupt,
    Coprocessor,
    Unimplemented,
}

impl Cpu {
    fn decode_and_execute_data_processing(&mut self, instruction: u32, current_pc: u32) {
        let opcode = (instruction >> 21) & 0xF;
        let s_bit = (instruction & (1 << 20)) != 0;
        let rn = ((instruction >> 16) & 0xF) as usize;
        let rd = ((instruction >> 12) & 0xF) as usize;

        let op1 = if rn == 15 {
            current_pc.wrapping_add(8)
        } else {
            self.regs[rn]
        };
        let op2 = self.arm_data_processing_operand2(instruction, current_pc);

        match opcode {
            0b1010 => {
                if !s_bit {
                    let r_bit = (instruction & (1 << 22)) != 0;
                    let psr = if r_bit { 0 } else { self.cpsr };
                    self.regs[rd] = psr;
                } else {
                    let result = op1.wrapping_sub(op2);
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                    self.set_flag_c(op1 >= op2);
                    let overflow = ((op1 ^ op2) & (op1 ^ result) & 0x80000000) != 0;
                    self.set_flag_v(overflow);
                }
            }
            0b1000 => {
                if !s_bit {
                    let r_bit = (instruction & (1 << 22)) != 0;
                    let psr = if r_bit { 0 } else { self.cpsr };
                    self.regs[rd] = psr;
                } else {
                    let result = op1 & op2;
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                }
            }
            0b1001 => {
                if !s_bit {
                    let r_bit = (instruction & (1 << 22)) != 0;
                    if !r_bit {
                        self.set_cpsr(op2);
                    }
                } else {
                    let result = op1 ^ op2;
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                }
            }
            0b1011 => {
                if !s_bit {
                    let r_bit = (instruction & (1 << 22)) != 0;
                    if !r_bit {
                        self.set_cpsr(op2);
                    }
                } else {
                    let result = op1.wrapping_add(op2);
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                    self.set_flag_c(((!op1) & op2) | ((!result) & (op1 | op2)) != 0);
                    let overflow = (!(op1 ^ op2) & (op1 ^ result) & 0x80000000) != 0;
                    self.set_flag_v(overflow);
                }
            }
            0b1100 => {
                let result = op1 | op2;
                self.regs[rd] = result;
                if s_bit && rd != 15 {
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                }
            }
            0b0100 => {
                let result = op1.wrapping_add(op2);
                self.regs[rd] = result;
                if s_bit && rd != 15 {
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                    let carry = (op1 as u64 + op2 as u64) > 0xFFFFFFFF;
                    self.set_flag_c(carry);
                    let overflow = (!(op1 ^ op2) & (op1 ^ result) & 0x80000000) != 0;
                    self.set_flag_v(overflow);
                }
            }
            0b0010 => {
                let result = op1.wrapping_sub(op2);
                self.regs[rd] = result;
                if s_bit && rd != 15 {
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                    self.set_flag_c(op1 >= op2);
                    let overflow = ((op1 ^ op2) & (op1 ^ result) & 0x80000000) != 0;
                    self.set_flag_v(overflow);
                }
            }
            0b0000 => {
                let result = op1 & op2;
                self.regs[rd] = result;
                if s_bit && rd != 15 {
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                }
            }
            0b1101 => {
                self.regs[rd] = op2;
                if s_bit && rd != 15 {
                    self.set_flag_n((op2 & (1 << 31)) != 0);
                    self.set_flag_z(op2 == 0);
                }
            }
            0b0001 => {
                let result = op1 ^ op2;
                self.regs[rd] = result;
                if s_bit && rd != 15 {
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                }
            }
            0b0011 => {
                let result = op2.wrapping_sub(op1);
                self.regs[rd] = result;
                if s_bit && rd != 15 {
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                    self.set_flag_c(op2 >= op1);
                }
            }
            0b0101 => {
                let carry = if self.get_flag_c() { 1 } else { 0 };
                let result = op1.wrapping_add(op2).wrapping_add(carry);
                self.regs[rd] = result;
                if s_bit && rd != 15 {
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                    self.set_flag_c((op1 as u64 + op2 as u64 + carry as u64) > 0xFFFFFFFF);
                }
            }
            0b0110 => {
                let carry_not = if self.get_flag_c() { 0 } else { 1 };
                let result = op1.wrapping_sub(op2).wrapping_sub(carry_not);
                self.regs[rd] = result;
                if s_bit && rd != 15 {
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                    self.set_flag_c((op1 as u64) >= (op2 as u64 + carry_not as u64));
                }
            }
            0b0111 => {
                let carry_not = if self.get_flag_c() { 0 } else { 1 };
                let result = op2.wrapping_sub(op1).wrapping_sub(carry_not);
                self.regs[rd] = result;
                if s_bit && rd != 15 {
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                    self.set_flag_c((op2 as u64) >= (op1 as u64 + carry_not as u64));
                }
            }
            0b1110 => {
                let result = op1 & (!op2);
                self.regs[rd] = result;
                if s_bit && rd != 15 {
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                }
            }
            0b1111 => {
                // MVN
                let result = !op2;
                self.regs[rd] = result;
                if s_bit && rd != 15 {
                    self.set_flag_n((result & (1 << 31)) != 0);
                    self.set_flag_z(result == 0);
                }
            }
            _ => {}
        }

        if s_bit && rd == 15 {
            let irq_idx = self.get_mode_index(self.cpsr);
            let spsr = self.banked_spsr[irq_idx];
            self.set_cpsr(spsr);
            self.thumb_mode = (spsr & 0x20) != 0;
        }
    }

    fn decode_and_execute_load_store(&mut self, bus: &mut Bus, instruction: u32, current_pc: u32) {
        if (instruction & 0x0E000090) == 0x00000090 && ((instruction >> 5) & 0b11) != 0 {
            let p = (instruction & (1 << 24)) != 0;
            let u = (instruction & (1 << 23)) != 0;
            let i = (instruction & (1 << 22)) != 0;
            let w = (instruction & (1 << 21)) != 0;
            let l = (instruction & (1 << 20)) != 0;

            let rn = ((instruction >> 16) & 0xF) as usize;
            let rd = ((instruction >> 12) & 0xF) as usize;

            let s = (instruction & (1 << 6)) != 0;
            let h = (instruction & (1 << 5)) != 0;

            let offset = if i {
                let imm_high = ((instruction >> 8) & 0xF) << 4;
                let imm_low = instruction & 0xF;
                imm_high | imm_low
            } else {
                let rm = (instruction & 0xF) as usize;
                self.regs[rm]
            };

            let base_addr = if rn == 15 {
                current_pc.wrapping_add(8)
            } else {
                self.regs[rn]
            };
            let target_addr = if u {
                base_addr.wrapping_add(offset)
            } else {
                base_addr.wrapping_sub(offset)
            };
            let active_addr = if p { target_addr } else { base_addr };

            if l {
                let data = match (s, h) {
                    (false, true) => bus.read_u16(active_addr) as u32,
                    (true, false) => (bus.read_u8(active_addr) as i8) as u32,
                    (true, true) => (bus.read_u16(active_addr) as i16) as u32,
                    _ => 0,
                };
                self.regs[rd] = data;
            } else {
                let data = if rd == 15 {
                    current_pc.wrapping_add(12)
                } else {
                    self.regs[rd]
                };
                if h {
                    bus.write_u16(active_addr, data as u16);
                }
            }

            if (!p || w) && rn != 15 && !(l && rn == rd) {
                self.regs[rn] = target_addr;
            }
            return;
        }

        let op_type = (instruction >> 25) & 0b111;
        let is_imm_offset = op_type == 0b010;
        let p = (instruction & (1 << 24)) != 0;
        let u = (instruction & (1 << 23)) != 0;
        let b = (instruction & (1 << 22)) != 0;
        let w = (instruction & (1 << 21)) != 0;
        let l = (instruction & (1 << 20)) != 0;
        let rn = ((instruction >> 16) & 0xF) as usize;
        let rd = ((instruction >> 12) & 0xF) as usize;

        let offset = if is_imm_offset {
            instruction & 0xFFF
        } else {
            let rm = (instruction & 0xF) as usize;
            if rm == 15 {
                current_pc.wrapping_add(8)
            } else {
                self.regs[rm]
            }
        };

        let base_addr = if rn == 15 {
            current_pc.wrapping_add(8)
        } else {
            self.regs[rn]
        };
        let target_addr = if u {
            base_addr.wrapping_add(offset)
        } else {
            base_addr.wrapping_sub(offset)
        };
        let active_addr = if p { target_addr } else { base_addr };

        if l {
            if b {
                self.regs[rd] = bus.read_u8(active_addr) as u32;
            } else {
                let raw_data = bus.read_u32(active_addr & !3);
                let alignment = (active_addr & 3) * 8;
                self.regs[rd] = raw_data.rotate_right(alignment);
            }

            if rd == 15 {
                self.regs[15] &= !3;
            }
        } else {
            let data = if rd == 15 {
                current_pc.wrapping_add(12)
            } else {
                self.regs[rd]
            };

            if b {
                bus.write_u8(active_addr, data as u8);
            } else {
                bus.write_u32(active_addr, data);
            }
        }

        if (!p || w) && rn != 15 && !(l && rn == rd) {
            self.regs[rn] = target_addr;
        }
    }

    fn decode_and_execute_block_transfer(
        &mut self,
        bus: &mut Bus,
        instruction: u32,
        current_pc: u32,
    ) {
        let p = (instruction & (1 << 24)) != 0;
        let u = (instruction & (1 << 23)) != 0;
        let s = (instruction & (1 << 22)) != 0;
        let w = (instruction & (1 << 21)) != 0;
        let l = (instruction & (1 << 20)) != 0;
        let rn = ((instruction >> 16) & 0xF) as usize;
        let register_list = instruction & 0xFFFF;

        let base_addr = self.regs[rn];

        let mut num_regs = 0;
        for i in 0..16 {
            if (register_list & (1 << i)) != 0 {
                num_regs += 1;
            }
        }

        let start_addr = if u {
            base_addr
        } else {
            base_addr.wrapping_sub(num_regs * 4)
        };

        let mut current_addr = start_addr;
        let mut affected_regs = Vec::new();

        for i in 0..16 {
            if (register_list & (1 << i)) != 0 {
                let _addr = if u {
                    let temp = current_addr;
                    if p {
                        current_addr = current_addr.wrapping_add(4);
                        current_addr
                    } else {
                        current_addr = current_addr.wrapping_add(4);
                        temp
                    }
                } else {
                    let temp = current_addr;
                    if p { temp.wrapping_add(4) } else { temp }
                };

                let active_addr = start_addr + (affected_regs.len() as u32 * 4);
                if l {
                    self.regs[i] = bus.read_u32(active_addr);
                    if i == 15 && s {
                        let irq_idx = self.get_mode_index(self.cpsr);
                        let spsr = self.banked_spsr[irq_idx];
                        self.set_cpsr(spsr);
                        self.thumb_mode = (spsr & 0x20) != 0;
                    }
                } else {
                    let data = if i == 15 {
                        current_pc.wrapping_add(12)
                    } else {
                        self.regs[i]
                    };
                    bus.write_u32(active_addr, data);
                }
                affected_regs.push(format!("R{}", i));
            }
        }

        if w {
            if u {
                self.regs[rn] = base_addr.wrapping_add(num_regs * 4);
            } else {
                self.regs[rn] = base_addr.wrapping_sub(num_regs * 4);
            }
        }
    }

    fn decode_and_execute_branch(&mut self, instruction: u32, current_pc: u32) {
        if (instruction & 0x0FFFFFF0) == 0x012FFF10 {
            let rm = (instruction & 0x0F) as usize;
            let target_addr = if rm == 15 {
                current_pc.wrapping_add(8)
            } else {
                self.regs[rm]
            };
            if (target_addr & 1) != 0 {
                self.thumb_mode = true;
                self.regs[15] = target_addr & !1;
            } else {
                self.thumb_mode = false;
                self.regs[15] = target_addr & !3;
            }
            return;
        }

        let link_flag = (instruction & (1 << 24)) != 0;
        let mut offset = instruction & 0x00FFFFFF;

        if (offset & 0x00800000) != 0 {
            offset |= 0xFF000000;
        }

        let branch_offset = (offset << 2) as i32;
        let pipeline_pc = current_pc.wrapping_add(8);
        let target_addr = (pipeline_pc as i32).wrapping_add(branch_offset) as u32;

        if link_flag {
            self.regs[14] = current_pc.wrapping_add(4);
        }

        self.regs[15] = target_addr;
    }

    fn classify_arm_instruction(instruction: u32) -> ArmInstructionClass {
        if (instruction & 0x0FFFFFF0) == 0x012FFF10 {
            return ArmInstructionClass::BranchExchange;
        }

        let op_type = (instruction >> 25) & 0b111;

        match op_type {
            0b000 | 0b001 => ArmInstructionClass::DataProcessing,
            0b010 | 0b011 => ArmInstructionClass::LoadStore,
            0b100 => ArmInstructionClass::BlockTransfer,
            0b101 => ArmInstructionClass::Branch,
            0b110 | 0b111 => {
                if (instruction & 0x0F000000) == 0x0F000000 {
                    ArmInstructionClass::SoftwareInterrupt
                } else {
                    ArmInstructionClass::Coprocessor
                }
            }
            _ => ArmInstructionClass::Unimplemented,
        }
    }

    fn dispatch_arm_instruction(&mut self, bus: &mut Bus, instruction: u32, current_pc: u32) {
        match Self::classify_arm_instruction(instruction) {
            ArmInstructionClass::DataProcessing => {
                self.decode_and_execute_data_processing(instruction, current_pc);
            }
            ArmInstructionClass::LoadStore => {
                self.decode_and_execute_load_store(bus, instruction, current_pc);
            }
            ArmInstructionClass::BlockTransfer => {
                self.decode_and_execute_block_transfer(bus, instruction, current_pc);
            }
            ArmInstructionClass::Branch => {
                self.decode_and_execute_branch(instruction, current_pc);
            }
            ArmInstructionClass::BranchExchange => {
                let rn = (instruction & 0xF) as usize;
                let target = if rn == 15 {
                    current_pc.wrapping_add(8)
                } else {
                    self.regs[rn]
                };

                if (target & 1) != 0 {
                    self.thumb_mode = true;
                    // 位址對齊 Thumb 指令的 2-byte 邊界
                    self.regs[15] = target & !1;
                } else {
                    self.thumb_mode = false;
                    // 位址對齊 ARM 指令的 4-byte 邊界
                    self.regs[15] = target & !3;
                }
            }
            ArmInstructionClass::SoftwareInterrupt => {
                // SWI 指令的 0-23 位才是編號
                let swi_number = instruction & 0x00FF_FFFF;
                // 這裡直接呼叫，不要去跑任何 DataProcessing 的邏輯
                self.handle_swi(bus, swi_number, current_pc);
            }
            ArmInstructionClass::Coprocessor | ArmInstructionClass::Unimplemented => {}
        }
    }

    pub fn execute_arm(&mut self, bus: &mut Bus, instruction: u32, current_pc: u32) {
        let valid_bios = current_pc < 0x00004000;
        let valid_mem = current_pc >= 0x02000000 && current_pc < 0x04000000;
        let valid_rom = current_pc >= 0x08000000 && current_pc <= 0x0E000000;

        if !(valid_bios || valid_mem || valid_rom) {
            println!(
                "CRASH WARNING: Invalid ARM PC: {:08X}, Instruction: {:08X}",
                current_pc, instruction
            );
            self.halted = true;
            return;
        }

        let cond = instruction >> 28;

        if !self.check_cond(cond) {
            return;
        }

        self.dispatch_arm_instruction(bus, instruction, current_pc);
    }
}
