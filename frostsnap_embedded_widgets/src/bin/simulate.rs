#![cfg(not(target_arch = "riscv32"))]

use embedded_graphics::{
    pixelcolor::{BinaryColor, Rgb565},
    prelude::*,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use frostsnap_embedded_widgets::{
    palette::PALETTE,
    bip39::{DisplaySeedWords, EnterBip39ShareScreen, EnterBip39T9Screen},
    center::Center,
    checkmark::Checkmark,
    color_map::ColorMap,
    fader::Fader,
    hold_to_confirm::HoldToConfirm,
    text::Text,
    welcome::Welcome,
    Widget,
    Instant,
    KeyTouch,
};
use std::time::SystemTime;
use std::io::{self, BufRead};
use std::sync::mpsc;
use std::thread;

const SCREEN_WIDTH: u32 = 240;
const SCREEN_HEIGHT: u32 = 280;

#[derive(Debug)]
enum Command {
    Touch { x: i32, y: i32 },
    Release { x: i32, y: i32 },
    Drag { _x1: i32, y1: i32, x2: i32, y2: i32 },
    Screenshot { filename: String },
    Wait { ms: u64 },
    Quit,
}

fn parse_command(line: &str) -> Option<Command> {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    
    match parts[0] {
        "touch" => {
            if parts.len() >= 2 {
                let coords: Vec<&str> = parts[1].split(',').collect();
                if coords.len() == 2 {
                    if let (Ok(x), Ok(y)) = (coords[0].parse(), coords[1].parse()) {
                        return Some(Command::Touch { x, y });
                    }
                }
            }
        }
        "release" => {
            if parts.len() >= 2 {
                let coords: Vec<&str> = parts[1].split(',').collect();
                if coords.len() == 2 {
                    if let (Ok(x), Ok(y)) = (coords[0].parse(), coords[1].parse()) {
                        return Some(Command::Release { x, y });
                    }
                }
            }
        }
        "drag" => {
            if parts.len() >= 3 {
                let start_coords: Vec<&str> = parts[1].split(',').collect();
                let end_coords: Vec<&str> = parts[2].split(',').collect();
                if start_coords.len() == 2 && end_coords.len() == 2 {
                    if let (Ok(x1), Ok(y1), Ok(x2), Ok(y2)) = (
                        start_coords[0].parse(),
                        start_coords[1].parse(),
                        end_coords[0].parse(),
                        end_coords[1].parse(),
                    ) {
                        return Some(Command::Drag { _x1: x1, y1, x2, y2 });
                    }
                }
            }
        }
        "screenshot" => {
            if parts.len() >= 2 {
                return Some(Command::Screenshot {
                    filename: parts[1].to_string(),
                });
            }
        }
        "wait" => {
            if parts.len() >= 2 {
                if let Ok(ms) = parts[1].parse() {
                    return Some(Command::Wait { ms });
                }
            }
        }
        "quit" => return Some(Command::Quit),
        _ => {}
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create display
    let mut display = SimulatorDisplay::<Rgb565>::new(Size::new(SCREEN_WIDTH, SCREEN_HEIGHT));
    
    // Clear display with background color
    display.clear(PALETTE.background)?;

    // Create output settings with proper RGB color and scaling
    // Device is 3.75cm (37.5mm) tall with 280 pixels = 0.134mm/pixel
    // For a typical 96 DPI monitor (3.78 pixels/mm), life size would be scale ~0.5
    // But that's too small, so we use scale 1 for a more practical size
    // Scale 1 = 240x280 pixels on screen (about 6.3cm x 7.4cm on a 96 DPI monitor)
    let output_settings = OutputSettingsBuilder::new()
        .scale(1) // Life-size would be ~0.5, but 1 is more practical
        .pixel_spacing(0) // No spacing between pixels
        .build();

    // Create window
    let mut window = Window::new("Frostsnap Widget Simulator", &output_settings);
    
    // Create channel for stdin commands
    let (tx, rx) = mpsc::channel();
    
    // Spawn thread to read stdin
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            if let Ok(line) = line {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match parse_command(&line) {
                    Some(cmd) => {
                        if tx.send(cmd).is_err() {
                            break;
                        }
                    }
                    None => {
                        eprintln!("Failed to parse command: '{}'", line);
                        std::process::exit(1);
                    }
                }
            }
        }
    });

    // Macro to run a widget with all the boilerplate handling
    macro_rules! run_widget {
        ($widget:expr) => {{
            let mut widget = $widget;
            let mut last_touch: Option<Point> = None;
            let start_time = SystemTime::now();
            let mut touch_feedback: Vec<(Point, u8)> = Vec::new(); // Point and frames remaining
            let mut wait_until: Option<u64> = None; // Time to wait until before processing next command

            // Initial draw and update to initialize the window
            let initial_time = Instant::from_millis(0);
            let _ = widget.draw(&mut display, initial_time);
            window.update(&display);

            'running: loop {
                let current_time = Instant::from_millis(
                    SystemTime::now()
                        .duration_since(start_time)
                        .unwrap()
                        .as_millis() as u64,
                );

                // Handle stdin commands (only if not waiting)
                if wait_until.map_or(true, |until| current_time.as_millis() >= until) {
                    wait_until = None; // Clear wait
                    if let Ok(cmd) = rx.try_recv() {
                        dbg!(&cmd);
                        match cmd {
                        Command::Touch { x, y } => {
                            let point = Point::new(x, y);
                            last_touch = Some(point);
                            widget.handle_touch(point, current_time, false);
                            // Add touch feedback
                            touch_feedback.push((point, 30));
                        }
                        Command::Release { x, y } => {
                            let point = Point::new(x, y);
                            widget.handle_touch(point, current_time, true);
                            last_touch = None;
                        }
                        Command::Drag { _x1: _, y1, x2, y2 } => {
                            widget.handle_vertical_drag(Some(y1 as u32), y2 as u32);
                            last_touch = Some(Point::new(x2, y2));
                        }
                        Command::Screenshot { filename } => {
                            // First draw any pending touch feedback circles
                            use embedded_graphics::{
                                primitives::{Circle, PrimitiveStyle},
                                pixelcolor::RgbColor,
                            };
                            
                            for (point, _) in &touch_feedback {
                                // Draw green circle at touch point
                                let _ = Circle::new(*point - Point::new(10, 10), 20)
                                    .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 3))
                                    .draw(&mut display);
                                // Also draw a filled circle in the center
                                let _ = Circle::new(*point - Point::new(2, 2), 4)
                                    .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
                                    .draw(&mut display);
                            }
                            
                            // Update window to show the circles
                            window.update(&display);
                            
                            // Save screenshot using the display's output image
                            let output_image = display.to_rgb_output_image(&output_settings);
                            if let Err(e) = output_image.save_png(&filename) {
                                eprintln!("Failed to save screenshot: {}", e);
                            } else {
                                println!("Screenshot saved to {}", filename);
                            }
                        }
                        Command::Wait { ms } => {
                            wait_until = Some(current_time.as_millis() + ms);
                        }
                        Command::Quit => break 'running,
                        }
                    }
                }

                // Handle simulator events
                for event in window.events() {
                    match event {
                        SimulatorEvent::Quit => break 'running,
                        SimulatorEvent::MouseButtonDown { point, .. } => {
                            last_touch = Some(point);
                            widget.handle_touch(point, current_time, false);
                            // Don't add touch feedback for manual mouse clicks
                        }
                        SimulatorEvent::MouseButtonUp { point, .. } => {
                            widget.handle_touch(point, current_time, true);
                            last_touch = None;
                        }
                        SimulatorEvent::MouseMove { point } => {
                            if let Some(prev_point) = last_touch {
                                // Check if this is a vertical drag (more vertical than horizontal movement)
                                if (point.y - prev_point.y).abs() > 2 {
                                    widget.handle_vertical_drag(Some(prev_point.y as u32), point.y as u32);
                                }
                                last_touch = Some(point);
                            }
                        }
                        _ => {}
                    }
                }

                // Draw widget
                let _ = widget.draw(&mut display, current_time);

                // Draw touch feedback circles
                use embedded_graphics::{
                    primitives::{Circle, PrimitiveStyle},
                    pixelcolor::RgbColor,
                };
                
                // Update and draw touch feedback
                touch_feedback.retain_mut(|(point, frames)| {
                    if *frames > 0 {
                        // Draw green circle at touch point
                        let _ = Circle::new(*point - Point::new(10, 10), 20)
                            .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 3))
                            .draw(&mut display);
                        // Also draw a filled circle in the center
                        let _ = Circle::new(*point - Point::new(2, 2), 4)
                            .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
                            .draw(&mut display);
                        *frames -= 1;
                        true
                    } else {
                        false
                    }
                });

                // Update window
                window.update(&display);
                
            }

            Ok(())
        }};
    }

    // Select which widget to demo
    let demo = std::env::args().nth(1).unwrap_or_else(|| "bip39_entry".to_string());

    match demo.as_str() {
        "bip39_entry" => {
            run_widget!(EnterBip39ShareScreen::new(Size::new(SCREEN_WIDTH, SCREEN_HEIGHT)))
        }
        "bip39_t9" => {
            run_widget!(EnterBip39T9Screen::new(Size::new(SCREEN_WIDTH, SCREEN_HEIGHT)))
        }
        "bip39_display" => {
            // Generate random test words
            use rand::seq::SliceRandom;
            let mut rng = rand::thread_rng();
            let bip39_words = frostsnap_backup::bip39_words::BIP39_WORDS;
            let mut test_words: [&'static str; 25] = [""; 25];
            for i in 0..25 {
                test_words[i] = bip39_words.choose(&mut rng).unwrap();
            }
            let share_index = 42;
            
            run_widget!(DisplaySeedWords::new(
                Size::new(SCREEN_WIDTH, SCREEN_HEIGHT),
                test_words,
                share_index
            ))
        },
        "hold_confirm" => {
            // Create text widgets for prompt and success messages
            let prompt_text = Text::new("Confirm\ntransaction");
            let prompt_widget = prompt_text.color_map(|c| match c {
                BinaryColor::On => PALETTE.on_surface,
                BinaryColor::Off => PALETTE.background,
            });
            
            let success_text = Text::new("Transaction\nsigned");
            let success_widget = success_text.color_map(|c| match c {
                BinaryColor::On => PALETTE.on_surface,
                BinaryColor::Off => PALETTE.background,
            });
            
            let widget = HoldToConfirm::new(
                Size::new(SCREEN_WIDTH, SCREEN_HEIGHT), 
                2000.0,
                prompt_widget,
                success_widget
            );
            
            run_widget!(widget)
        }
        "checkmark" => {
            // Animated checkmark
            let mut checkmark = Checkmark::new(96);
            checkmark.start_animation();
            
            // Wrap with ColorMap to convert BinaryColor to Rgb565
            let checkmark_rgb = ColorMap::new(checkmark, |color| {
                match color {
                    BinaryColor::On => PALETTE.primary,
                    BinaryColor::Off => PALETTE.background,
                }
            });
            
            // Center the checkmark
            let centered = Center::new(checkmark_rgb);
            
            run_widget!(centered)
        }
        "welcome" => {
            // Welcome screen widget
            let widget = Welcome::new();
            
            run_widget!(widget)
        }
        "fade_in_fade_out" => {
            // Create a widget that manages the fade in/out timing
            struct FadeController<W> {
                fader: Fader<W>,
                started: bool,
            }
            
            impl<W: Widget<Color = Rgb565>> Widget for FadeController<W> {
                type Color = Rgb565;
                
                fn draw<D: DrawTarget<Color = Self::Color>>(
                    &mut self,
                    target: &mut D,
                    current_time: Instant,
                ) -> Result<(), D::Error> {
                    // Start the first fade-in
                    if !self.started {
                        self.fader.start_fade_in(1000, 50, PALETTE.background);
                        self.started = true;
                    }
                    
                    // When fade completes, immediately start the next one
                    if self.fader.is_fade_complete() {
                        if self.fader.is_faded_out() {
                            // Fade in
                            self.fader.start_fade_in(1000, 50, PALETTE.background);
                        } else {
                            // Fade out
                            self.fader.start_fade(1000, 50, PALETTE.background);
                        }
                    }
                    
                    self.fader.draw(target, current_time)
                }
                
                fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) -> Option<crate::KeyTouch> {
                    self.fader.handle_touch(point, current_time, is_release)
                }
                
                fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
                    self.fader.handle_vertical_drag(prev_y, new_y);
                }
                
                fn size_hint(&self) -> Option<Size> {
                    self.fader.size_hint()
                }
                
                fn force_full_redraw(&mut self) {
                    self.fader.force_full_redraw();
                }
            }
            
            // Create a text widget to fade in and out
            let text = Text::new("Hello, Fading World!");
            let text_colored = text.color_map(|c| match c {
                BinaryColor::On => PALETTE.primary,
                BinaryColor::Off => PALETTE.background,
            });
            
            // Create a fader starting faded out
            let fader = Fader::new_faded_out(text_colored);
            
            let controller = FadeController {
                fader,
                started: false,
            };
            
            run_widget!(controller)
        }
        _ => {
            eprintln!("Unknown demo: {}. Available demos: bip39_entry, bip39_t9, bip39_display, hold_confirm, checkmark, welcome, hold_checkmark, fade_in_fade_out", demo);
            Ok(())
        }
    }
}

