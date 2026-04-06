use capstone::prelude::*;
use std::fs;

fn main() {
    let cs = Capstone::new().arm().mode(arch::arm::ArchMode::Thumb).build().unwrap();
    let rom = fs::read("../rom.gba").unwrap();
    let addr = 0x08000300 - 0x08000000;
    let data = &rom[addr..addr+0x20];
    let insns = cs.disasm_all(data, 0x08000300).unwrap();
    for i in insns.iter() {
        println!("0x{:X}:\t{}\t{}", i.address(), i.mnemonic().unwrap_or(""), i.op_str().unwrap_or(""));
    }
}
