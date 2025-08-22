#![cfg(not(target_arch = "riscv32"))]

use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use std::cell::RefCell;
use std::io::{self, BufRead};
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::SystemTime;

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
    let parts: Vec<&str> = line.split_whitespace().collect();
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
                        return Some(Command::Drag {
                            _x1: x1,
                            y1,
                            x2,
                            y2,
                        });
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
    let display = Rc::new(RefCell::new(SimulatorDisplay::<Rgb565>::new(Size::new(
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
    ))));

    // Clear display with background color
    display.borrow_mut().clear(PALETTE.background)?;

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
            let line: Result<String, _> = line;
            let line: String = match line {
                Ok(line) => line.trim().to_owned(),
                Err(e) => {
                    eprintln!("failed to read command: {e}");
                    return;
                }
            };
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
    });

    // Macro to run a widget with all the boilerplate handling
    macro_rules! run_widget {
        ($widget:expr) => {{
            let mut widget = $widget;
            // Set constraints on root widget
            widget.set_constraints(Size::new(240, 280));
            let mut last_touch: Option<Point> = None;
            let mut drag_start: Option<Point> = None; // Initial position when mouse down
            let mut is_dragging = false;
            const DRAG_THRESHOLD: i32 = 5; // Minimum pixels to consider it a drag
            let start_time = SystemTime::now();
            let mut touch_feedback: Vec<(Point, u8)> = Vec::new(); // Point and frames remaining
            let mut wait_until: Option<u64> = None; // Time to wait until before processing next command

            // Initial draw and update to initialize the window
            let _initial_time = Instant::from_millis(0);
            window.update(&display.borrow());

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
                                if let Some(prev_point) = last_touch {
                                    widget.handle_vertical_drag(Some(prev_point.y as u32), y as u32, true);
                                }
                                widget.handle_touch(point, current_time, true);
                                last_touch = None;
                            }
                            Command::Drag { _x1: _, y1, x2, y2 } => {
                                widget.handle_vertical_drag(Some(y1 as u32), y2 as u32, false);
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
                                        .draw(&mut *display.borrow_mut());
                                    // Also draw a filled circle in the center
                                    let _ = Circle::new(*point - Point::new(2, 2), 4)
                                        .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
                                        .draw(&mut *display.borrow_mut());
                                }

                                // Update window to show the circles
                                window.update(&display.borrow());

                                // Save screenshot using the display's output image
                                let output_image = display.borrow().to_rgb_output_image(&output_settings);
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
                            drag_start = Some(point);
                            last_touch = Some(point);
                            widget.handle_touch(point, current_time, false);
                            is_dragging = false;
                            // Don't send touch event yet - wait to see if it's a drag
                        }
                        SimulatorEvent::MouseButtonUp { point, .. } => {
                            if is_dragging {
                                // This was a drag - send final drag event with previous position
                                let prev_y = last_touch.map(|p| p.y as u32);
                                widget.handle_vertical_drag(prev_y, point.y as u32, true);
                            } else {
                                // This was a click - send touch down and up
                                widget.handle_touch(point, current_time, true);
                            }
                            drag_start = None;
                            last_touch = None;
                            is_dragging = false;
                        }
                        SimulatorEvent::MouseMove { point } => {
                            if let Some(start) = drag_start {
                                // Check if we've moved enough to consider it a drag
                                let distance = ((point.x - start.x).pow(2) + (point.y - start.y).pow(2)) as f32;
                                let distance = distance.sqrt() as i32;

                                if distance > DRAG_THRESHOLD {
                                    // Start dragging if we haven't already
                                    if !is_dragging {
                                        is_dragging = true;
                                    }

                                    // Send drag with previous position
                                    let prev_y = last_touch.map(|p| p.y as u32);
                                    widget.handle_vertical_drag(prev_y, point.y as u32, false);
                                }

                                last_touch = Some(point);
                            }
                        }
                        _ => {}
                    }
                }

                let mut target = SuperDrawTarget::from_shared(display.clone(), PALETTE.background);
                let _ = widget.draw(&mut target, current_time);

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
                            .draw(&mut *display.borrow_mut());
                        // Also draw a filled circle in the center
                        let _ = Circle::new(*point - Point::new(2, 2), 4)
                            .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
                            .draw(&mut *display.borrow_mut());
                        *frames -= 1;
                        true
                    } else {
                        false
                    }
                });

                // Update window
                window.update(&display.borrow());

            }

        }};
    }

    // Select which widget to demo
    let demo = std::env::args().nth(1).unwrap_or("help".to_string());
    let screen_size = Size::new(SCREEN_WIDTH, SCREEN_HEIGHT);

    // Use the demo_widget! macro for all demos (including help)
    frostsnap_embedded_widgets::demo_widget!(demo, screen_size, run_widget);
    Ok(())
}
