use super::Cpu;

impl Cpu {
    pub fn arm_data_processing_operand2(&self, instruction: u32, current_pc: u32) -> u32 {
        if ((instruction >> 25) & 0b111) == 0b001 {
            let imm8 = instruction & 0xFF;
            let rot = ((instruction >> 8) & 0xF) * 2;
            return imm8.rotate_right(rot);
        }

        let rm = (instruction & 0xF) as usize;
        let rm_val = if rm == 15 {
            current_pc.wrapping_add(8)
        } else {
            self.regs[rm]
        };

        let shift_amount = if (instruction & (1 << 4)) != 0 {
            let rs = ((instruction >> 8) & 0xF) as usize;
            self.regs[rs] & 0xFF
        } else {
            (instruction >> 7) & 0x1F
        };

        let shift_type = (instruction >> 5) & 0x3;

        match shift_type {
            0 => {
                if shift_amount == 0 {
                    rm_val
                } else if shift_amount < 32 {
                    rm_val << shift_amount
                } else {
                    0
                }
            }
            1 => {
                let amt = if (instruction & (1 << 4)) == 0 && shift_amount == 0 {
                    32
                } else {
                    shift_amount
                };
                if amt == 0 {
                    rm_val
                } else if amt >= 32 {
                    0
                } else {
                    rm_val >> amt
                }
            }
            2 => {
                let amt = if (instruction & (1 << 4)) == 0 && shift_amount == 0 {
                    32
                } else {
                    shift_amount
                };
                if amt == 0 {
                    rm_val
                } else if amt >= 32 {
                    ((rm_val as i32) >> 31) as u32
                } else {
                    ((rm_val as i32) >> amt) as u32
                }
            }
            3 => {
                if (instruction & (1 << 4)) == 0 && shift_amount == 0 {
                    let c = self.get_flag_c() as u32;
                    (rm_val >> 1) | (c << 31)
                } else {
                    rm_val.rotate_right(shift_amount & 0x1F)
                }
            }
            _ => rm_val,
        }
    }
}