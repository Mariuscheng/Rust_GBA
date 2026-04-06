use super::Bus;

impl Bus {
    pub(super) fn write_timer_register(&mut self, offset: usize, value: u8) {
        let timer = (offset - 0x100) / 4;
        let reg = (offset - 0x100) % 4;

        self.io[offset] = value;

        match reg {
            0 => {
                self.timer_reload[timer] = (self.timer_reload[timer] & 0xFF00) | value as u16;
            }
            1 => {
                self.timer_reload[timer] =
                    (self.timer_reload[timer] & 0x00FF) | ((value as u16) << 8);
            }
            2 => {
                let old_control = self.timer_control[timer];
                self.timer_control[timer] = (self.timer_control[timer] & 0xFF00) | value as u16;

                let was_enabled = (old_control & 0x0080) != 0;
                let is_enabled = (self.timer_control[timer] & 0x0080) != 0;
                if !was_enabled && is_enabled {
                    self.timer_counter[timer] = self.timer_reload[timer];
                    self.timer_prescaler_accum[timer] = 0;
                    self.sync_timer_io(timer);
                }
            }
            3 => {
                self.timer_control[timer] =
                    (self.timer_control[timer] & 0x00FF) | ((value as u16) << 8);
            }
            _ => {}
        }
    }

    pub(super) fn tick_timers(&mut self, cycles: u32) {
        let mut overflows = [0u32; 4];

        for timer in 0..4 {
            let control = self.timer_control[timer];
            if (control & 0x0080) == 0 {
                continue;
            }

            let increments = if (control & 0x0004) != 0 {
                if timer == 0 { 0 } else { overflows[timer - 1] }
            } else {
                let prescaler = match control & 0x0003 {
                    0 => 1,
                    1 => 64,
                    2 => 256,
                    _ => 1024,
                };

                self.timer_prescaler_accum[timer] += cycles;
                let ticks = self.timer_prescaler_accum[timer] / prescaler;
                self.timer_prescaler_accum[timer] %= prescaler;
                ticks
            };

            if increments == 0 {
                continue;
            }

            overflows[timer] = self.advance_timer(timer, increments);
        }
    }

    fn advance_timer(&mut self, timer: usize, increments: u32) -> u32 {
        let start = self.timer_counter[timer] as u32;
        let reload = self.timer_reload[timer] as u32;
        let end = start + increments;

        if end < 0x1_0000 {
            self.timer_counter[timer] = end as u16;
            self.sync_timer_io(timer);
            return 0;
        }

        let period = 0x1_0000 - reload;
        let remaining = end - 0x1_0000;
        let overflow_count = 1 + (remaining / period);
        self.timer_counter[timer] = (reload + (remaining % period)) as u16;
        self.sync_timer_io(timer);

        if (self.timer_control[timer] & 0x0040) != 0 {
            self.request_interrupt(1 << (3 + timer));
        }

        overflow_count
    }

    fn sync_timer_io(&mut self, timer: usize) {
        let base = 0x100 + timer * 4;
        self.io[base] = (self.timer_counter[timer] & 0x00FF) as u8;
        self.io[base + 1] = (self.timer_counter[timer] >> 8) as u8;
        self.io[base + 2] = (self.timer_control[timer] & 0x00FF) as u8;
        self.io[base + 3] = (self.timer_control[timer] >> 8) as u8;
    }
}