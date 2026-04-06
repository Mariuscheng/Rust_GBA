use rust_gba::display::{DisplayDevice, Sdl3Display};
use rust_gba::gba::Gba;
use std::env;
use std::fs;

fn main() {
    println!("=== Rust GBA Emulator 初始化 ===");

    let args: Vec<String> = env::args().collect();
    let default_rom = "rom.gba".to_string();
    let rom_path = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with("--"))
        .map(|arg| arg.as_str())
        .unwrap_or(default_rom.as_str());

    let rom_data = match fs::read(rom_path) {
        Ok(data) => {
            println!("成功載入 ROM: {} (大小: {} bytes)", rom_path, data.len());
            data
        }
        Err(e) => {
            eprintln!("錯誤: 無法載入 ROM 檔案 {}: {}", rom_path, e);
            std::process::exit(1);
        }
    };

    let mut gba = Gba::new(rom_data);

    println!(">>> GBA 系統初始化完成，準備進入主迴圈 (Main Loop) <<<");

    let sdl_context = sdl3::init().unwrap();
    let mut display = Sdl3Display::new(&sdl_context);
    let mut event_pump = sdl_context.event_pump().unwrap();

    println!(">>> 啟動系統... <<<");

    'running: loop {
        // 先跑硬體時脈直到產生一個新畫面 (Frame)
        while !gba.bus.ppu.frame_ready {
            gba.step();
        }
        gba.bus.ppu.frame_ready = false;

        display.update_frame(&gba.bus.ppu.frame_buffer[..]);

        for event in event_pump.poll_iter() {
            match event {
                sdl3::event::Event::KeyDown {
                    keycode: Some(sdl3::keyboard::Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }
    }

    println!("=== 模擬器結束執行 ===");
}
