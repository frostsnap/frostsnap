/// Macro for selecting and running demo widgets
#[macro_export]
macro_rules! demo_widget {
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
            "confirm_touch" | "hold_confirm" | "hold_checkmark" | "hold_to_confirm" => {
                use $crate::{text::Text, HoldToConfirm, palette::PALETTE};
                use embedded_graphics::text::Alignment;
                use embedded_graphics::pixelcolor::BinaryColor;
                
                let prompt_text = Text::new("Confirm\ntransaction", u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.on_background)).with_alignment(Alignment::Center);
                let widget = HoldToConfirm::new($screen_size, 2000, prompt_text);
                $run_macro!(widget);
            }
            "welcome" => {
                use $crate::welcome::Welcome;
                let widget = Welcome::new();
                $run_macro!(widget);
            }
            "vertical_slide" => {
                // Commented out until VerticalPaginator is refactored
                panic!("vertical_slide demo is temporarily disabled while refactoring");
            }
            "bip39_backup" => {
                use $crate::bip39::Bip39BackupDisplay;
                use embedded_graphics::prelude::*;
                
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
                
                // Create the backup display - it now uses PageSlider internally and outputs Rgb565
                let widget = Bip39BackupDisplay::new($screen_size, TEST_WORD_INDICES, share_index);
                
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
                use $crate::{bobbing_carat::BobbingCarat, center::Center, palette::PALETTE};
                
                // Create the bobbing carat widget with colors
                let bobbing_carat = BobbingCarat::new(PALETTE.on_background, PALETTE.background);
                
                // Center it on screen
                let centered = Center::new(bobbing_carat);
                
                $run_macro!(centered);
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
                
                // Create mock data for demo purposes
                let t_of_n = (2, 3); // 2 of 3 threshold
                let security_check_code: [u8; 4] = [0xAB, 0xCD, 0xEF, 0x12];
                
                let widget = KeygenCheck::new(t_of_n, security_check_code);
                $run_macro!(widget);
            }
            "sign_prompt" => {
                use $crate::sign_prompt::SignPrompt;
                use frostsnap_core::bitcoin_transaction::PromptSignBitcoinTx;
                use core::str::FromStr;
                
                // Create dummy transaction data with different address types
                // Segwit v0 address (starts with bc1q)
                // let segwit_address = bitcoin::Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
                //     .unwrap()
                //     .assume_checked();
                
                // Taproot address (starts with bc1p)
                let taproot_address = bitcoin::Address::from_str("bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297")
                    .unwrap()
                    .assume_checked();
                
                // Legacy P2PKH address (starts with 1)
                // let legacy_address = bitcoin::Address::from_str("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa")
                //     .unwrap()
                //     .assume_checked();
                
                let prompt = PromptSignBitcoinTx {
                    foreign_recipients: $crate::alloc::vec![
                        (taproot_address, bitcoin::Amount::from_sat(500_001)), // 21.005 BTC
                        // (segwit_address, bitcoin::Amount::from_sat(150_000)), // 0.0015 BTC
                        // (legacy_address, bitcoin::Amount::from_sat(50_000)), // 0.0005 BTC
                    ],
                    fee: bitcoin::Amount::from_sat(125_000), // 0.00125 BTC (high fee for demo)
                };
                
                // Create the sign prompt widget
                let widget = SignPrompt::new($screen_size, prompt);
                
                $run_macro!(widget);
            }
            "bitcoin_amount" => {
                use $crate::{bitcoin_amount_display::BitcoinAmountDisplay, column::{Column, MainAxisAlignment}, palette::PALETTE};
                
                // Create a simple BitcoinAmountDisplay with 21 BTC
                let amount_display = BitcoinAmountDisplay::new(21_000_000); // 21 BTC
                
                // Map binary colors to Gray4 for display
                let amount_mapped = amount_display.color_map(|c| match c {
                    embedded_graphics::pixelcolor::BinaryColor::Off => PALETTE.text_disabled,
                    embedded_graphics::pixelcolor::BinaryColor::On => PALETTE.primary,
                });
                
                // Put it in a Column with MainAxisAlignment::Center like in sign_prompt
                let widget = Column::new((amount_mapped,))
                    .with_main_axis_alignment(MainAxisAlignment::Center);
                
                $run_macro!(widget);
            }
            "slide_in" => {
                use $crate::{PageSlider, WidgetList, Widget, center::Center, text::Text, container::Container, palette::PALETTE};
                use embedded_graphics::prelude::*;
                use embedded_graphics::primitives::PrimitiveStyle;
                
                // Create a WidgetList that generates text widgets on the fly
                struct InfiniteTextPages;
                
                impl WidgetList<Container<Text<u8g2_fonts::U8g2TextStyle<embedded_graphics::pixelcolor::Rgb565>>>> for InfiniteTextPages {
                    fn len(&self) -> usize {
                        usize::MAX // Infinite pages!
                    }
                    
                    fn get(&self, index: usize) -> Option<Container<Text<u8g2_fonts::U8g2TextStyle<embedded_graphics::pixelcolor::Rgb565>>>> {
                        let text = Text::new(
                            $crate::alloc::format!("Page {}\nLorem ipsum\nderp herp!", index + 1),
                            u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                        );
                        let container = Container::new(text)
                            .with_border(PrimitiveStyle::with_stroke(PALETTE.primary, 2));
                        Some(container)
                    }
                }
                
                // Create the PageSlider with infinite text pages
                let page_slider = PageSlider::new(InfiniteTextPages, $screen_size.height);
                let widget = Center::new(page_slider);
                
                $run_macro!(widget);
            }
            "slide_in_old" => {
                // Keep the old demo for reference
                use $crate::{slide_in_transition::SlideInTransition, text::Text, palette::PALETTE, Instant, Widget, container::Container, center::Center};
                use embedded_graphics::prelude::*;
                use embedded_graphics::primitives::PrimitiveStyle;
                use $crate::alloc::format;
                
                // Create initial text widget centered in a container with border
                let text = Text::new("Page 1\nLorem ipus\nderp herp!", u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let container = Container::new(text)
                    .with_border(PrimitiveStyle::with_stroke(PALETTE.primary, 2));

                // Create slide-in transition widget
                let mut transition = SlideInTransition::new(
                    container,
                    1000, // 500ms transition
                    Point::new(0, 150), // Slide up from bottom
                    PALETTE.background
                );
                
                // Create a wrapper that advances pages on touch release
                struct SlideInDemo {
                    transition: SlideInTransition<$crate::container::Container<Text<u8g2_fonts::U8g2TextStyle<embedded_graphics::pixelcolor::Rgb565>>>>,
                    page_num: usize,
                }
                
                impl SlideInDemo {
                    fn new(transition: SlideInTransition<$crate::container::Container<Text<u8g2_fonts::U8g2TextStyle<embedded_graphics::pixelcolor::Rgb565>>>>) -> Self {
                        Self {
                            transition,
                            page_num: 1,
                        }
                    }
                }
                
                impl $crate::DynWidget for SlideInDemo {
                    fn handle_touch(&mut self, _point: Point, _current_time: Instant, is_release: bool) -> Option<$crate::KeyTouch> {
                        // Advance to next page on touch release
                        if is_release {
                            self.page_num += 1;
                            let text = Text::new(
                                format!("Page {}\nLorem ipus\nderp herp!", self.page_num),
                                u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                            );
                            let container = Container::new(text)
                                .with_border(PrimitiveStyle::with_stroke(PALETTE.primary, 2));
                            
                            self.transition.switch_to(container);
                        }
                        None
                    }
                    
                    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
                        self.transition.handle_vertical_drag(prev_y, new_y, is_release)
                    }
                    
                    fn size_hint(&self) -> Option<Size> {
                        self.transition.size_hint()
                    }
                    
                    fn force_full_redraw(&mut self) {
                        self.transition.force_full_redraw()
                    }
                }
                
                impl Widget for SlideInDemo {
                    type Color = embedded_graphics::pixelcolor::Rgb565;
                    
                    fn draw<D: DrawTarget<Color = Self::Color>>(
                        &mut self,
                        target: &mut D,
                        current_time: Instant,
                    ) -> Result<(), D::Error> {
                        self.transition.draw(target, current_time)
                    }
                }
                
                let demo = SlideInDemo::new(transition);
                $run_macro!(Center::new(demo));
            }
            "firmware_upgrade_progress" | "firmware_upgrade_download" => {
                use $crate::firmware_upgrade::FirmwareUpgradeProgress;
                
                // Show downloading state at 65% progress
                let widget = FirmwareUpgradeProgress::downloading(0.65);
                $run_macro!(widget);
            }
            "firmware_upgrade_erase" => {
                use $crate::firmware_upgrade::FirmwareUpgradeProgress;
                
                // Show erasing state at 35% progress
                let widget = FirmwareUpgradeProgress::erasing(0.35);
                $run_macro!(widget);
            }
            "firmware_upgrade_passive" => {
                use $crate::firmware_upgrade::FirmwareUpgradeProgress;
                
                // Show passive state
                let widget = FirmwareUpgradeProgress::passive();
                $run_macro!(widget);
            }
            "progress" => {
                use $crate::{ProgressIndicator, Widget, Instant};
                use embedded_graphics::prelude::*;
                
                // Create a progress indicator that animates from 0 to 100%
                struct AnimatedProgress {
                    indicator: ProgressIndicator,
                    start_time: Option<Instant>,
                    duration_ms: u64,
                }
                
                impl AnimatedProgress {
                    fn new() -> Self {
                        Self {
                            indicator: ProgressIndicator::new(),
                            start_time: None,
                            duration_ms: 5000, // 5 seconds to complete
                        }
                    }
                }
                
                impl $crate::DynWidget for AnimatedProgress {
                    fn size_hint(&self) -> Option<Size> {
                        self.indicator.size_hint()
                    }
                    
                    fn force_full_redraw(&mut self) {
                        self.indicator.force_full_redraw()
                    }
                }
                
                impl Widget for AnimatedProgress {
                    type Color = embedded_graphics::pixelcolor::Rgb565;
                    
                    fn draw<D: DrawTarget<Color = Self::Color>>(
                        &mut self,
                        target: &mut D,
                        current_time: Instant,
                    ) -> Result<(), D::Error> {
                        // Initialize start time on first draw
                        if self.start_time.is_none() {
                            self.start_time = Some(current_time);
                        }
                        
                        // Calculate progress based on elapsed time
                        let elapsed = current_time.saturating_duration_since(self.start_time.unwrap());
                        let progress = $crate::Frac::from_ratio(elapsed as u32, self.duration_ms as u32);
                        
                        // Update the indicator's progress
                        self.indicator.set_progress(progress);
                        
                        // Draw the indicator
                        self.indicator.draw(target, current_time)
                    }
                }
                
                let widget = AnimatedProgress::new();
                
                $run_macro!(widget);
            }
            "firmware_upgrade" => {
                use $crate::FirmwareUpgradeConfirm;
                
                // Create a test firmware digest (SHA256 hash)
                let firmware_digest: [u8; 32] = [
                    0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90,
                    0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x11, 0x22,
                    0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00,
                    0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe,
                ];
                let size_bytes = 1_234_567; // ~1.2 MB
                
                let widget = FirmwareUpgradeConfirm::new($screen_size, firmware_digest, size_bytes);
                
                $run_macro!(widget);
            }
            _ => {
                panic!("Unknown demo: '{}'. Valid demos: bip39_entry, bip39_t9, hold_confirm, checkmark, welcome, vertical_slide, bip39_backup, fade_in_fade_out, device_name, bobbing_icon, swipe_up_chevron, keygen_check, sign_prompt, bitcoin_amount, slide_in, slide_in_old, progress, firmware_upgrade_progress, firmware_upgrade_download, firmware_upgrade_erase, firmware_upgrade_passive, firmware_upgrade", $demo);
            }
        }
    };
}
