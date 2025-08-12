# Frostsnap Embedded Widgets

A collection of embedded graphics widgets for the Frostsnap device, designed for use on resource-constrained embedded systems with touchscreen displays.

## Features

- **No-std compatible**: All widgets are designed to work without the standard library
- **Touch-enabled**: Built-in touch event handling for interactive UIs
- **Material Design**: Uses a dark theme color palette based on Material Design
- **Flexible sizing**: Widgets can adapt to different screen sizes
- **Animation support**: Smooth transitions and visual feedback

## Widgets

### BIP39 Widgets
- `EnterBip39ShareScreen`: Full-screen widget for entering BIP39 mnemonic words
- `AlphabeticKeyboard`: Touch keyboard optimized for BIP39 word entry
- `WordSelector`: Word selection interface with fuzzy matching

### General Widgets
- `HoldToConfirm`: Wrapper widget that requires a long press to confirm actions
- `SizedBox`: Simple widget with fixed dimensions for layout purposes

### Components
- Navigation buttons with page counter
- Progress bars for multi-step processes
- Scroll bars for long content
- Touch feedback animations

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
frostsnap_embedded_widgets = { path = "../frostsnap_embedded_widgets" }
```

Basic example:

```rust
use frostsnap_embedded_widgets::{
    widgets::{EnterBip39ShareScreen, Widget},
    Instant,
};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};

// Create a BIP39 entry screen
let mut screen = EnterBip39ShareScreen::new(Size::new(240, 280));

// Handle touch events
screen.handle_touch(point, current_time, lift_up);

// Draw to display
screen.draw(&mut display, current_time)?;
```

## Simulator

The crate includes a desktop simulator for testing widgets without hardware:

```bash
# Requires SDL2 to be installed on your system
cargo run --bin simulate bip39_entry
cargo run --bin simulate hold_confirm
```

## Color Palette

The widgets use a consistent Material Design dark theme palette defined in `src/palette.rs`. Colors include:
- Background and surface colors
- Primary, secondary, and tertiary accent colors
- Text and icon colors with proper contrast
- Error and success states

## License

Same as the parent Frostsnap project.