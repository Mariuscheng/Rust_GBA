pub trait DisplayDevice {
    fn update_frame(&mut self, pixels: &[u8]);
}

pub struct Sdl3Display {
    canvas: sdl3::render::Canvas<sdl3::video::Window>,
    width: u32,
    height: u32,
    pitch: usize,
}

impl Sdl3Display {
    pub fn new(sdl_context: &sdl3::Sdl) -> Self {
        let video_subsystem = sdl_context.video().unwrap();
        let window = video_subsystem
            .window("Rust GBA Emulator", 240 * 3, 160 * 3)
            .position_centered()
            .build()
            .unwrap();

        Self {
            canvas: window.into_canvas(),
            width: 240,
            height: 160,
            pitch: 240 * 4,
        }
    }
}

impl DisplayDevice for Sdl3Display {
    fn update_frame(&mut self, pixels: &[u8]) {
        let texture_creator = self.canvas.texture_creator();
        let mut texture = texture_creator
            .create_texture_streaming(sdl3::pixels::PixelFormat::RGBA8888, self.width, self.height)
            .unwrap();

        texture.update(None, pixels, self.pitch).unwrap();
        self.canvas.clear();
        self.canvas.copy(&texture, None, None).unwrap();
        self.canvas.present();
    }
}
