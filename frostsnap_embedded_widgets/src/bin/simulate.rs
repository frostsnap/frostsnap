#![cfg(not(target_arch = "riscv32"))]

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
};
use embedded_graphics_simulator::{
    OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
};
use frostsnap_embedded_widgets::{
    palette::PALETTE,
    widgets::{
        bip39::{DisplaySeedWords, EnterBip39ShareScreen},
        hold_to_confirm::HoldToConfirmWidget,
        sized_box::SizedBox,
        Widget,
    },
    Instant,
};
use std::time::SystemTime;

const SCREEN_WIDTH: u32 = 240;
const SCREEN_HEIGHT: u32 = 280;

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

    // Select which widget to demo
    let demo = std::env::args().nth(1).unwrap_or_else(|| "bip39_entry".to_string());

    match demo.as_str() {
        "bip39_entry" => run_bip39_entry_demo(&mut display, &mut window),
        "bip39_display" => run_bip39_display_demo(&mut display, &mut window),
        "hold_confirm" => run_hold_confirm_demo(&mut display, &mut window),
        _ => {
            eprintln!("Unknown demo: {}. Available demos: bip39_entry, bip39_display, hold_confirm", demo);
            Ok(())
        }
    }
}

fn run_bip39_entry_demo(
    display: &mut SimulatorDisplay<Rgb565>,
    window: &mut Window,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut widget = EnterBip39ShareScreen::new(Size::new(SCREEN_WIDTH, SCREEN_HEIGHT));
    let mut last_touch: Option<Point> = None;
    let start_time = SystemTime::now();

    // Initial draw and update to initialize the window
    let initial_time = Instant::from_ticks(0);
    let _ = widget.draw(display, initial_time);
    window.update(display);

    'running: loop {
        let current_time = Instant::from_ticks(
            SystemTime::now()
                .duration_since(start_time)
                .unwrap()
                .as_millis() as u64,
        );

        // Handle simulator events
        for event in window.events() {
            match event {
                SimulatorEvent::Quit => break 'running,
                SimulatorEvent::MouseButtonDown { point, .. } => {
                    last_touch = Some(point);
                    widget.handle_touch(point, current_time, false);
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
        let _ = widget.draw(display, current_time);

        // Update window
        window.update(display);

        // Check if widget is finished
        if widget.is_finished() {
            println!("Mnemonic entered: {}", widget.get_mnemonic());
            break 'running;
        }
    }

    Ok(())
}

fn run_bip39_display_demo(
    display: &mut SimulatorDisplay<Rgb565>,
    window: &mut Window,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_touch: Option<Point> = None;
    
    // Generate random test words
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    let bip39_words = frostsnap_backup::bip39_words::BIP39_WORDS;
    let mut test_words: [&'static str; 25] = [""; 25];
    for i in 0..25 {
        test_words[i] = bip39_words.choose(&mut rng).unwrap();
    }

    let mut widget = DisplaySeedWords::new(
        Size::new(SCREEN_WIDTH, SCREEN_HEIGHT),
        test_words,
        42, // Share index
    );
    
    let start_time = SystemTime::now();

    // Initial draw and update to initialize the window
    let initial_time = Instant::from_ticks(0);
    let _ = widget.draw(display, initial_time);
    window.update(display);

    'running: loop {
        let current_time = Instant::from_ticks(
            SystemTime::now()
                .duration_since(start_time)
                .unwrap()
                .as_millis() as u64,
        );

        // Handle simulator events
        for event in window.events() {
            match event {
                SimulatorEvent::Quit => break 'running,
                SimulatorEvent::MouseButtonDown { point, .. } => {
                    last_touch = Some(point);
                    widget.handle_touch(point, current_time, false);
                }
                SimulatorEvent::MouseButtonUp { point, .. } => {
                    widget.handle_touch(point, current_time, true);
                    last_touch = None;
                }
                SimulatorEvent::MouseMove { point } => {
                    if let Some(prev_point) = last_touch {
                        // Check if this is a vertical drag
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
        let _ = widget.draw(display, current_time);

        // Update window
        window.update(display);
    }

    Ok(())
}

fn run_hold_confirm_demo(
    display: &mut SimulatorDisplay<Rgb565>,
    window: &mut Window,
) -> Result<(), Box<dyn std::error::Error>> {
    let child = SizedBox::new(Size::new(200, 100));
    let mut widget = HoldToConfirmWidget::new(child, 3000.0); // 3 second hold
    widget.enable(); // Enable the widget
    
    let start_time = SystemTime::now();

    // Initial draw and update to initialize the window
    let initial_time = Instant::from_ticks(0);
    let _ = widget.draw(display, initial_time);
    window.update(display);

    'running: loop {
        let current_time = Instant::from_ticks(
            SystemTime::now()
                .duration_since(start_time)
                .unwrap()
                .as_millis() as u64,
        );

        // Handle simulator events
        for event in window.events() {
            match event {
                SimulatorEvent::Quit => break 'running,
                SimulatorEvent::MouseButtonDown { point, .. } => {
                    widget.handle_touch(point, current_time, false);
                }
                SimulatorEvent::MouseButtonUp { point, .. } => {
                    widget.handle_touch(point, current_time, true);
                }
                _ => {}
            }
        }

        // Draw widget
        let _ = widget.draw(display, current_time);

        // Update window
        window.update(display);

        // Check if confirmed
        if widget.is_completed() {
            println!("Hold to confirm completed!");
            break 'running;
        }
    }

    Ok(())
}