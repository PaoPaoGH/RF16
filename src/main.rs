mod vm;

use minifb::{Key, ScaleMode, Window, WindowOptions};
use rodio::{buffer::SamplesBuffer, DeviceSinkBuilder, Player};
use std::env;
use std::f64::consts::PI;
use std::fs;
use std::io;
use std::num::NonZero;

const WINDOW_SIZE: usize = 512;
const FB_SIZE: usize = 16;
const PIXEL_SCALE: usize = WINDOW_SIZE / FB_SIZE;

const SAMPLE_RATE: i32 = 48000;
const AMPLITUDE: f32 = 0.85;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Palette {
    Rgb332,
    Grayscale,
}

fn pixel_color(byte: u8, palette: Palette) -> u32 {
    match palette {
        Palette::Rgb332 => {
            let r = ((byte & 0xE0) >> 5) as u32;
            let g = ((byte & 0x1C) >> 2) as u32;
            let b = (byte & 0x03) as u32;

            let r = (r * 255) / 7;
            let g = (g * 255) / 7;
            let b = (b * 255) / 3;

            (r << 16) | (g << 8) | b
        }
        Palette::Grayscale => {
            let v = byte as u32;
            (v << 16) | (v << 8) | v
        }
    }
}

fn envelope(i: usize, attack: usize, samples: usize) -> f64 {
    if i < attack {
        i as f64 / attack as f64
    } else if i > samples - attack {
        (samples - i) as f64 / attack as f64
    } else {
        1.0
    }
}

fn make_note_buffer(pitch: u8) -> Vec<f32> {
    let freq = 440.0 * 2f64.powf((pitch as f64 - 69.0) / 12.0);
    let samples = SAMPLE_RATE as usize / 6;
    let attack = SAMPLE_RATE as usize / 50;

    (0..samples)
        .map(|i| {
            let t = i as f64 / SAMPLE_RATE as f64;
            (AMPLITUDE as f64 * envelope(i, attack, samples) * (2.0 * PI * freq * t).sin()) as f32
        })
        .collect()
}

fn play_note(player_audio_stream: &Player, pitch: u8) {
    let samples = make_note_buffer(pitch);

    let source = SamplesBuffer::new(
        NonZero::new(1u16).unwrap(),
        NonZero::new(SAMPLE_RATE as u32).unwrap(),
        samples,
    );

    player_audio_stream.append(source);
}

fn read_input(window: &Window) -> u8 {
    let mut key = 0;

    if window.is_key_down(Key::Z) {
        key |= 0x80;
    }

    if window.is_key_down(Key::X) {
        key |= 0x40;
    }

    if window.is_key_down(Key::Enter) {
        key |= 0x20;
    }

    if window.is_key_down(Key::Space) {
        key |= 0x10;
    }

    if window.is_key_down(Key::Up) {
        key |= 0x08;
    }

    if window.is_key_down(Key::Down) {
        key |= 0x04;
    }

    if window.is_key_down(Key::Left) {
        key |= 0x02;
    }

    if window.is_key_down(Key::Right) {
        key |= 0x01;
    }

    key
}

fn load_program(path: &str) -> io::Result<vm::VM> {
    let raw = fs::read(path)?;
    let source: String = raw.iter().map(|&b| b as char).collect();

    Ok(vm::VM::new(vm::compile_bf(&source)))
}

fn write_scaled_pixel(framebuffer: &mut [u32], x: usize, y: usize, color: u32) {
    for sy in 0..PIXEL_SCALE {
        for sx in 0..PIXEL_SCALE {
            framebuffer[(y * PIXEL_SCALE + sy) * WINDOW_SIZE + (x * PIXEL_SCALE + sx)] = color;
        }
    }
}

fn render_framebuffer(interp: &vm::VM, framebuffer: &mut [u32], palette: Palette) {
    for y in 0..FB_SIZE {
        for x in 0..FB_SIZE {
            let color = pixel_color(interp.ram[y * FB_SIZE + x], palette);
            write_scaled_pixel(framebuffer, x, y, color);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 || args.len() > 3 {
        eprintln!("Usage: {} <filename> [--grayscale]", args[0]);
        std::process::exit(1);
    }

    let palette = if args.iter().any(|a| a == "--grayscale") {
        Palette::Grayscale
    } else {
        Palette::Rgb332
    };

    let mut interp = load_program(&args[1])?;

    let mut window = Window::new(
        "RUST BF16",
        WINDOW_SIZE,
        WINDOW_SIZE,
        WindowOptions {
            scale_mode: ScaleMode::AspectRatioStretch,
            resize: true,
            ..WindowOptions::default()
        },
    )?;

    window.set_target_fps(60);

    let mut framebuffer: Vec<u32> = vec![0u32; WINDOW_SIZE * WINDOW_SIZE];
    let audio_stream = DeviceSinkBuilder::open_default_sink()?;
    let player_audio_stream = Player::connect_new(audio_stream.mixer());

    let mut current_note: u8 = 0;

    while window.is_open() {
        render_framebuffer(&interp, &mut framebuffer, palette);

        if !interp.run_until_output_with_input(|| read_input(&window)) {
            break;
        }

        let note = interp.ram[interp.ptr];

        if note == current_note { /* skip */ } else {
            current_note = note;
            if current_note != 0 {
                play_note(&player_audio_stream, current_note);
             }
        }

        window.update_with_buffer(&framebuffer, WINDOW_SIZE, WINDOW_SIZE)?;
    }

    Ok(())
}
