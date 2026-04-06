use super::Bus;

fn lz77_decode_bytes(bus: &mut Bus, mut src: u32) -> Vec<u8> {
    let header = bus.read_u32(src);
    let uncompressed_size = header >> 8;
    src += 4;

    if uncompressed_size == 0 {
        return Vec::new();
    }

    let mut output = Vec::with_capacity(uncompressed_size as usize);
    while (output.len() as u32) < uncompressed_size {
        let flags = bus.read_u8(src);
        src += 1;

        for i in (0..8).rev() {
            if (output.len() as u32) >= uncompressed_size {
                break;
            }

            if (flags & (1 << i)) != 0 {
                let b1 = bus.read_u8(src);
                let b2 = bus.read_u8(src + 1);
                src += 2;

                let count = ((b1 >> 4) & 0xF) as u32 + 3;
                let disp = (((b1 & 0xF) as u32) << 8) | (b2 as u32);

                let mut copy_index = output.len().saturating_sub(disp as usize + 1);
                for _ in 0..count {
                    if (output.len() as u32) >= uncompressed_size {
                        break;
                    }
                    let byte = output[copy_index];
                    output.push(byte);
                    copy_index += 1;
                }
            } else {
                let byte = bus.read_u8(src);
                src += 1;
                output.push(byte);
            }
        }
    }
    output
}

pub fn lz77_decompress_wram(bus: &mut Bus, src: u32, mut dst: u32) {
    let output = lz77_decode_bytes(bus, src);
    for byte in output {
        bus.write_u8(dst, byte);
        dst = dst.wrapping_add(1);
    }
}

pub fn lz77_decompress_vram(bus: &mut Bus, src: u32, mut dst: u32) {
    let output = lz77_decode_bytes(bus, src);

    for chunk in output.chunks(2) {
        let value = match chunk {
            [lo, hi] => *lo as u16 | ((*hi as u16) << 8),
            [lo] => *lo as u16,
            _ => 0,
        };

        bus.write_u16(dst & !1, value);
        dst = dst.wrapping_add(2);
    }
}
