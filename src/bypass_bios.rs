use crate::cpu::Cpu;
use crate::memory::Bus;

const SYSTEM_MODE: u32 = 0x1F;
const IRQ_MODE: u32 = 0x12;
const SUPERVISOR_MODE: u32 = 0x13;

const SYS_SP: u32 = 0x0300_7F00;
const IRQ_SP: u32 = 0x0300_7FA0;
const SVC_SP: u32 = 0x0300_7FE0;

const POSTFLG: usize = 0x300;
const KEYINPUT_LO: usize = 0x130;
const KEYINPUT_HI: usize = 0x131;
const HEADER_LOGO_START: usize = 0x04;
const HEADER_LOGO_END: usize = 0xA0;

/// 跳過 BIOS，直接將 CPU 及記憶體狀態設定為從 ROM 開機完成的狀態
pub fn bypass_bios(cpu: &mut Cpu, bus: &mut Bus) {
    cpu.clear_state();
    cpu.set_mode_stack_pointer(SYSTEM_MODE, SYS_SP);
    cpu.set_mode_stack_pointer(IRQ_MODE, IRQ_SP);
    cpu.set_mode_stack_pointer(SUPERVISOR_MODE, SVC_SP);

    cpu.set_cpsr(SYSTEM_MODE);

    for reg in 0..13 {
        cpu.set_reg(reg, 0);
    }

    cpu.set_reg(13, SYS_SP);
    cpu.set_pc(0x0800_0000);

    initialize_boot_memory(bus);
    initialize_io_registers(bus);
    seed_logo_residue(bus);
}

fn initialize_boot_memory(bus: &mut Bus) {
    // 1. 清空主記憶體
    bus.ewram.fill(0);
    bus.iwram.fill(0);

    // 2. 清空顯示相關記憶體 (這是 PPU 渲染的基礎)
    bus.vram.fill(0);
    bus.palram.fill(0);
    bus.oam.fill(0);
    // 關鍵修改：不要讓 PPU 自己去持有一份 VRAM 
    // 如果你的 Ppu 結構體裡還有 vram 欄位，請在這裡刪除對它的 fill 呼叫
    // bus.ppu.vram.fill(0); // <--- 如果有這行，刪掉它，統一用 bus.vram
    bus.ppu.frame_buffer.fill(0);
    bus.ppu.frame_ready = false;
    bus.ppu.frame_count = 0;

    bus.timer_reload = [0; 4];
    bus.timer_counter = [0; 4];
    bus.timer_control = [0; 4];
    bus.timer_prescaler_accum = [0; 4];
    bus.cycles = 0;

    if bus.rom.len() >= 0xA0 {
        for i in 0x04..0xA0 {
            let logo_byte = bus.rom[i];
            // 有些遊戲會檢查 IWRAM 裡的 Logo 備份
            bus.iwram[0x3F00 + (i - 0x04)] = logo_byte; 
        }
    }
}

fn initialize_io_registers(bus: &mut Bus) {
    bus.io.fill(0);



    // 保留目前模擬器依賴的非零 open-bus 預設值，避免未實作寄存器導致除以零。
    for i in 0x10..=0x3F {
        bus.io[i] = 0x01;
    }

    bus.io[KEYINPUT_LO] = 0xFF;
    bus.io[KEYINPUT_HI] = 0x03;

    // BIOS 執行完成後，POSTFLG 會被設為 1。
    bus.io[POSTFLG] = 0x01;

    // 略過 BIOS 時提供穩定的顯示與音訊暫存器預設值。
    bus.write_u16(0x0400_0000, 0x0080); // 先設定 Forced Blank (bit 7)
    bus.write_u16(0x0400_0088, 0x0200);

    // 中斷控制器保留為乾淨狀態，由遊戲自行開啟；強制 IME=1 容易引入過早 IRQ。
    bus.write_u16(0x0400_0200, 0x0000);
    bus.write_u16(0x0400_0202, 0x0000);
    bus.write_u16(0x0400_0208, 0x0000);

    // 設定 VCOUNT (0x4000006)
    bus.io[0x06] = 0;
}

fn seed_logo_residue(bus: &mut Bus) {
    // 這不是完整 BIOS logo 解壓流程，但能提供比全零更接近實機的 VRAM/Palette 殘留。
    bus.palram[0] = 0xFF;
    bus.palram[1] = 0x7F;
    bus.palram[2] = 0x00;
    bus.palram[3] = 0x00;

    if bus.rom.len() > HEADER_LOGO_START {
        let logo_end = HEADER_LOGO_END.min(bus.rom.len());
        let logo = &bus.rom[HEADER_LOGO_START..logo_end];
        let copy_len = logo.len().min(bus.vram.len());
        bus.vram[..copy_len].copy_from_slice(&logo[..copy_len]);
    }
}
