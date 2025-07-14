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
    column::Column,
    hold_to_confirm::HoldToConfirm,
    hold_to_confirm_button::HoldToConfirmButton,
    row::Row,
    sized_box::SizedBox,
    text::Text,
    Widget,
    Instant,
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
    Drag { x1: i32, y1: i32, x2: i32, y2: i32 },
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
                        return Some(Command::Drag { x1, y1, x2, y2 });
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
    let output_settings = OutputSettingsBuilder::new()
        .scale(2) // Scale to 2x for a reasonable size (480x560 pixels)
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

                // Handle stdin commands
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
                        Command::Drag { x1: _, y1, x2, y2 } => {
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
                            std::thread::sleep(std::time::Duration::from_millis(ms));
                        }
                        Command::Quit => break 'running,
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
            let child = SizedBox::<BinaryColor>::new(Size::new(200, 100));
            let mut hold_to_confirm = HoldToConfirm::new(child, 3000.0);
            hold_to_confirm.enable();
            
            // Wrap with ColorMap to convert BinaryColor to Rgb565
            let hold_to_confirm_rgb = ColorMap::new(hold_to_confirm, |color| {
                match color {
                    BinaryColor::On => PALETTE.primary,
                    BinaryColor::Off => PALETTE.surface_variant,
                }
            });
            
            run_widget!(hold_to_confirm_rgb)
        }
        "hold_button" => {
            let button_size = Size::new(200, 60);
            let text_widget = Text::new("SUBMIT");
            let mut button = HoldToConfirmButton::new(button_size, text_widget, 2000.0);
            button.enable();
            
            // Wrap button with ColorMap to convert BinaryColor to Rgb565
            let button_rgb = ColorMap::new(button, |color| {
                match color {
                    BinaryColor::On => PALETTE.primary,
                    BinaryColor::Off => PALETTE.surface_variant,
                }
            });
            
            // Center the button
            let centered = Center::new(button_rgb);
            
            run_widget!(centered)
        }
        "checkmark" => {
            // Animated checkmark
            let mut checkmark = Checkmark::new(Size::new(50, 50));
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
        _ => {
            eprintln!("Unknown demo: {}. Available demos: bip39_entry, bip39_t9, bip39_display, hold_confirm, hold_button, checkmark", demo);
            Ok(())
        }
    }
}

