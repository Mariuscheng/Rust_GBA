use rust_gba::display::{DisplayDevice, Sdl3Display};
use rust_gba::gba::Gba;
use std::env;
use std::fs;

fn main() {
    println!("=== Rust GBA Emulator 初始化 ===");

    // 1. 解析命令列參數 (如果有傳入 ROM 路徑)
    let args: Vec<String> = env::args().collect();
    let debug_tiles = args.iter().any(|arg| arg == "--debug-tiles");
    let default_rom = "rom.gba".to_string();
    let rom_path = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with("--"))
        .map(|arg| arg.as_str())
        .unwrap_or(default_rom.as_str());

    // 2. 讀取 GBA ROM
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

    // 3. 實例化整台主機 - 把 GBA 所有硬體都建立出來
    let mut gba = Gba::new(rom_data);

    // 確保 VRAM 存取沒有問題 (測試物件組合 / 匯流排讀寫邏輯)
    gba.bus.write_u16(0x06000000, 0x1234);
    assert_eq!(
        gba.bus.read_u16(0x06000000),
        0x1234,
        "VRAM 無法正確讀寫！檢查位址對應或組合邏輯"
    );
    println!(
        ">>> VRAM 記憶體映射連線測試成功 (0x06000000 = {:#X})",
        gba.bus.read_u16(0x06000000)
    );

    // 清除測試寫入的值
    gba.bus.write_u16(0x06000000, 0x0000);

    // 測試：讀取 ROM 進入點的 32-bit ARM 指令
    let entry_point_inst = gba.bus.read_u32(0x08000000);
    println!(
        "成功! ROM (0x08000000) 的第一條 ARM 指令為 0x{:08X}",
        entry_point_inst
    );

    println!(">>> GBA 系統初始化完成，準備進入主迴圈 (Main Loop) <<<");
    if debug_tiles {
        println!(">>> 啟用 Debug Tile Renderer: 顯示目前啟用 BG 的 char base tiles <<<");
    }

    // 5. 進入程式主迴圈與 SDL3 渲染
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

        if debug_tiles {
            let mut ppu = std::mem::take(&mut gba.bus.ppu);
            ppu.render_debug_tiles(&gba.bus);
            gba.bus.ppu = ppu;
        }

        display.update_frame(&gba.bus.ppu.frame_buffer[..]);

        if gba.bus.ppu.frame_count > 150 {
            println!("Debug 150 frames. VRAM used? Let's see.");
            break 'running;
        }

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
