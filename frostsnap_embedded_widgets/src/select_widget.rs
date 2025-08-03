/// Macro for selecting and running demo widgets
#[macro_export]
macro_rules! select_widget {
    ($demo:expr, $screen_size:expr, $run_macro:ident) => {
        match $demo.as_ref() {
            "bip39_entry" => {
                let widget = $crate::bip39::EnterBip39ShareScreen::new($screen_size);
                $run_macro!(widget);
            }
            "bip39_t9" => {
                let widget = $crate::bip39::EnterBip39T9Screen::new($screen_size);
                $run_macro!(widget);
            }
            "confirm_touch" | "hold_confirm" | "hold_checkmark" => {
                use $crate::{text::Text, HoldToConfirm, palette::PALETTE};
                use embedded_graphics::pixelcolor::BinaryColor;
                
                let prompt_text = Text::new("Confirm\ntransaction", u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, BinaryColor::On));
                let prompt_widget = prompt_text.color_map(|c| match c {
                    BinaryColor::On => PALETTE.on_surface,
                    BinaryColor::Off => PALETTE.background,
                });
                
                let widget = HoldToConfirm::new($screen_size, 2000, prompt_widget);
                $run_macro!(widget);
            }
            "welcome" => {
                use $crate::welcome::Welcome;
                let widget = Welcome::new();
                $run_macro!(widget);
            }
            "vertical_slide" => {
                use $crate::{PageDemo, VerticalPaginator, palette::PALETTE};
                use embedded_graphics::{prelude::*, framebuffer::buffer_size};
                
                let page_demo = PageDemo::new($screen_size);
                const SCREEN_WIDTH: usize = 240;
                const SCREEN_HEIGHT: usize = 280;
                const BUFFER_SIZE: usize = buffer_size::<<PageDemo as Widget>::Color>(SCREEN_WIDTH, SCREEN_HEIGHT);
                let paginator = VerticalPaginator::<_, SCREEN_WIDTH, SCREEN_HEIGHT, BUFFER_SIZE>::new(page_demo);
                
                let widget = paginator.color_map(|c| match c.luma() {
                    0b00 => PALETTE.background,
                    0b01 => PALETTE.outline,
                    0b10 => PALETTE.primary,
                    0b11|_ => PALETTE.on_background
                });
                
                $run_macro!(widget);
            }
            "bip39_backup" => {
                use $crate::{bip39::Bip39BackupDisplay, VerticalPaginator, PaginatorWithScrollBar, palette::PALETTE};
                use embedded_graphics::{prelude::*, framebuffer::buffer_size};
                use embedded_text::alignment::HorizontalAlignment;
                
                // Generate test word indices - same words as original display
                const TEST_WORD_INDICES: [u16; 25] = [
                    1337, // owner
                    432,  // deny
                    1789, // survey
                    923,  // journey
                    567,  // embark
                    1456, // recall
                    234,  // churn
                    1678, // spawn
                    890,  // invest
                    345,  // crater
                    1234, // neutral
                    678,  // fiscal
                    1890, // thumb
                    456,  // diamond
                    1567, // robot
                    789,  // guitar
                    1345, // oyster
                    123,  // badge
                    1789, // survey
                    567,  // embark
                    1012, // lizard
                    1456, // recall
                    789,  // guitar
                    1678, // spawn
                    234,  // churn
                ];
                let share_index = 42;
                
                let backup_display = Bip39BackupDisplay::new($screen_size, TEST_WORD_INDICES, share_index);
                const SCREEN_WIDTH: usize = 240;
                const SCREEN_HEIGHT: usize = 280; // Full screen height
                const BUFFER_SIZE: usize = buffer_size::<<Bip39BackupDisplay as Widget>::Color>(SCREEN_WIDTH, SCREEN_HEIGHT);
                let paginator = VerticalPaginator::<_, SCREEN_WIDTH, SCREEN_HEIGHT, BUFFER_SIZE>::new(backup_display);
                
                let paginator_mapped = paginator.color_map(|c| match c.luma() {
                    0b00 => PALETTE.background,           // Black background
                    0b01 => PALETTE.on_surface_variant,   // Gray for secondary text
                    0b10 => PALETTE.outline,              // Not used currently
                    0b11 => PALETTE.primary,              // Cyan/blue for primary text
                    _ => PALETTE.on_background
                });
                
                // Create HoldToConfirm widget for final page
                use $crate::{HoldToConfirm, text::Text};
                use embedded_graphics::pixelcolor::BinaryColor;
                
                let confirm_prompt = Text::new("I have written down:\n\n- the key index\n- all 25 words", u8g2_fonts::U8g2TextStyle::new($crate::FONT_SMALL, BinaryColor::On));
                let confirm_prompt_rgb = confirm_prompt.color_map(|c| match c {
                    BinaryColor::On => PALETTE.on_surface,
                    BinaryColor::Off => PALETTE.background,
                });
                
                let hold_to_confirm = HoldToConfirm::new($screen_size, 2000, confirm_prompt_rgb);
                
                let widget = PaginatorWithScrollBar::new(paginator_mapped, hold_to_confirm, $screen_size);
                
                $run_macro!(widget);
            }
            "fade_in_fade_out" => {
                use $crate::{fader::Fader, text::Text, palette::PALETTE};
                use embedded_graphics::pixelcolor::BinaryColor;
                
                // Simple text widget that will fade in/out
                let text = Text::new("Fade Demo", u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, BinaryColor::On));
                let text_colored = text.color_map(|c| match c {
                    BinaryColor::On => PALETTE.on_background,
                    BinaryColor::Off => PALETTE.background,
                });
                
                // Create a fader starting faded out
                let mut fader = Fader::new_faded_out(text_colored);
                // Start the fade-in immediately
                fader.start_fade_in(1000, 50, PALETTE.background);
                
                $run_macro!(fader);
            }
            "device_name" => {
                use $crate::DeviceNameScreen;
                
                // Create device name screen with a long name to test
                let mut device_name_screen = DeviceNameScreen::new("Frank Zappppper".into());
                device_name_screen.set_edit_mode(true);
                
                $run_macro!(device_name_screen);
            }
            "bobbing_icon" => {
                use $crate::{container::Container, sized_box::SizedBox, center::Center, translate::Translate, palette::PALETTE, Widget};
                use embedded_graphics::{prelude::*, primitives::PrimitiveStyle};
                
                // Simple sized box as child
                let sized_box = SizedBox::new(Size::new(50, 50));
                
                // Put it in a container with a border
                let container = Container::new(sized_box)
                    .with_border(PrimitiveStyle::with_stroke(PALETTE.on_background, 10));
                

                // Wrap in Translate with repeat mode
                let mut translate = Translate::new(container, PALETTE.background);
                translate.set_repeat(true);
                // Bob right and left 30 pixels over 10 seconds each way
                translate.translate(Point::new(100, 100), 1000);
                
                $run_macro!(translate);
            }
            "swipe_up_chevron" => {
                use $crate::{SwipeUpChevron, palette::PALETTE, center::Center};
                
                // Create swipe up chevron with bobbing animation
                let swipe_hint = SwipeUpChevron::new(PALETTE.on_surface, PALETTE.background);
                
                // Center it on screen
                let centered = Center::new(swipe_hint);
                
                $run_macro!(centered);
            }
            "keygen_check" => {
                use $crate::keygen_check::KeygenCheck;
                
                // Create keygen check widget with the specified bytes and t_of_n
                let widget = KeygenCheck::new([0x40, 0x86, 0xc8, 0xbd], (2, 3));
                
                $run_macro!(widget);
            }
            _ => {
                panic!("Unknown demo: '{}'. Valid demos: bip39_entry, bip39_t9, hold_confirm, checkmark, welcome, vertical_slide, bip39_backup, fade_in_fade_out, device_name, bobbing_icon, swipe_up_chevron, keygen_check", $demo);
            }
        }
    };
}
