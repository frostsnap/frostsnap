/// Macro for selecting and running demo widgets
#[macro_export]
macro_rules! demo_widget {
    ($demo:expr,  $run_macro:ident) => {
        // Common imports for all demos
        use $crate::{
            palette::PALETTE,
            HoldToConfirm,
            FONT_SMALL, FONT_MED, FONT_LARGE,
            prelude::*,
            DefaultTextStyle,
        };
        use embedded_graphics::{
            prelude::*,
            pixelcolor::{Rgb565, BinaryColor},
        };
        use $crate::alloc::string::{String, ToString};
        use $crate::HOLD_TO_CONFIRM_TIME_MS;

        // Shared test word indices for demos that need them
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

        match $demo.as_ref() {
            "hello_world" => {
                use $crate::text::Text;
                let widget = Text::new("Hello World!", DefaultTextStyle::new(FONT_LARGE, PALETTE.on_background));
                $run_macro!(widget);
            }
            "bip39_entry" => {
                let mut widget = $crate::backup::EnterShareScreen::new();
                if cfg!(feature = "prefill-words") {
                    widget.prefill_test_words();
                }
                $run_macro!(widget);
            }
            "log_touches" => {
                use $crate::{TouchListener, Center, Text, Key};
                // Debug logging is now in device crate - this demo just shows touch listener

                // Create centered text with instructions
                let text = Text::new("Touch me!", DefaultTextStyle::new(FONT_LARGE, PALETTE.on_background))
                    .with_alignment(embedded_graphics::text::Alignment::Center);
                let centered = Center::new(text);

                // Wrap it with TouchListener (logging would happen in device crate if enabled)
                let touch_listener = TouchListener::new(centered, |_point, _time, _is_release, _widget| {
                    None::<Key>
                });
                $run_macro!(touch_listener);
            }
            "numeric_keyboard" => {
                let widget = $crate::backup::NumericKeyboard::new();
                $run_macro!(widget);
            }
            "confirm_touch" | "hold_confirm" | "hold_checkmark" | "hold_to_confirm" => {
                use $crate::{text::Text, HoldToConfirm, palette::PALETTE};
                use embedded_graphics::pixelcolor::BinaryColor;
                let prompt_text = Text::new("Confirm\ntransaction", DefaultTextStyle::new(FONT_MED, PALETTE.on_background)).with_alignment(embedded_graphics::text::Alignment::Center);
                let widget = HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_MS, prompt_text);
                $run_macro!(widget);
            }
            "welcome" => {
                use $crate::welcome::Welcome;
                let widget = Welcome::new();
                $run_macro!(widget);
            }
            "column_cross_axis" => {
                use $crate::{text::Text, Column, palette::PALETTE};

                // First column with Start alignment (left-aligned)
                let text1 = Text::new("cross axis", DefaultTextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let text2 = Text::new("start", DefaultTextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let inner_column1 = Column::new((text1, text2))
                    .with_cross_axis_alignment($crate::CrossAxisAlignment::Start);

                // Second column with center cross-axis alignment
                let text3 = Text::new("cross axis", DefaultTextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let text4 = Text::new("center", DefaultTextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let inner_column2 = Column::new((text3, text4))
                    .with_cross_axis_alignment($crate::CrossAxisAlignment::Center);

                // Third column with End alignment (right-aligned)
                let text5 = Text::new("cross axis", DefaultTextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let text6 = Text::new("end", DefaultTextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let inner_column3 = Column::new((text5, text6))
                    .with_cross_axis_alignment($crate::CrossAxisAlignment::End);

                // Outer column containing all three inner columns (default center alignment)
                let widget = Column::new((inner_column1, inner_column2, inner_column3));
                $run_macro!(widget);
            }
            "row_cross_axis" => {
                use $crate::{text::Text, Row, Column, Container, palette::PALETTE};

                // First row with Start alignment (top-aligned)
                let text1 = Text::new("cross axis", DefaultTextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
                let text2 = Text::new("start", DefaultTextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
                let inner_row1 = Row::new((text1, text2))
                    .with_cross_axis_alignment($crate::CrossAxisAlignment::Start)
                    .with_debug_borders(true);
                let container1 = Container::with_size(inner_row1, Size::new(240, 80))
                    .with_border(PALETTE.primary, 2);

                // Second row with center cross-axis alignment
                let text3 = Text::new("cross axis", DefaultTextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
                let text4 = Text::new("center", DefaultTextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
                let inner_row2 = Row::new((text3, text4))
                    .with_cross_axis_alignment($crate::CrossAxisAlignment::Center)
                    .with_debug_borders(true);
                let container2 = Container::with_size(inner_row2, Size::new(240, 80))
                    .with_border(PALETTE.primary, 2);

                // Third row with End alignment (bottom-aligned)
                let text5 = Text::new("cross axis", DefaultTextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
                let text6 = Text::new("end", DefaultTextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
                let inner_row3 = Row::new((text5, text6))
                    .with_cross_axis_alignment($crate::CrossAxisAlignment::End)
                    .with_debug_borders(true);
                let container3 = Container::with_size(inner_row3, Size::new(240, 80))
                    .with_border(PALETTE.primary, 2);

                // Outer column containing all three containers
                let widget = Column::new((container1, container2, container3));
                $run_macro!(widget);
            }
            "row_center" => {
                use $crate::{text::Text, Row, Container, palette::PALETTE};

                // First row with Start alignment
                let text_a = Text::new("A", DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let text_b = Text::new("B", DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let start_row = Row::new((text_a, text_b))
                    .with_main_axis_alignment($crate::MainAxisAlignment::Start)
                    .with_debug_borders(true);
                let start_container = Container::new(start_row).with_border(PALETTE.primary, 2);

                // Second row with Center alignment
                let text_c = Text::new("C", DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let text_d = Text::new("D", DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let center_row = Row::new((text_c, text_d))
                    .with_main_axis_alignment($crate::MainAxisAlignment::Center)
                    .with_debug_borders(true);
                let center_container = Container::new(center_row).with_border(PALETTE.primary, 2);

                // Outer row containing both containers
                let widget = Row::new((start_container, center_container));
                $run_macro!(widget);
            }
            "column_center" => {
                use $crate::{text::Text, Column, Container, palette::PALETTE};

                // First column with Start alignment
                let text1 = Text::new("main axis alignment", DefaultTextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let text2 = Text::new("start", DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let start_column = Column::new((text1, text2)).with_debug_borders(true);
                let start_container = Container::new(start_column).with_border(PALETTE.primary, 2);

                // Second column with Center alignment
                let text3 = Text::new("main axis alignment", DefaultTextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let text4 = Text::new("center", DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let center_column = Column::new((text3, text4))
                    .with_main_axis_alignment($crate::MainAxisAlignment::Center).with_debug_borders(true);
                let center_container = Container::new(center_column).with_border(PALETTE.primary, 2);

                // Outer column containing both containers
                let widget = Column::new((start_container, center_container));
                $run_macro!(widget);
            }
            "bip39_backup" => {
                use $crate::backup::BackupDisplay;
                use embedded_graphics::prelude::*;

                let share_index = 42;

                // Create the backup display - it now uses PageSlider internally and outputs Rgb565
                let widget = BackupDisplay::new(TEST_WORD_INDICES, share_index);

                $run_macro!(widget);
            }
            "fade_in" => {
                use $crate::{fader::Fader, text::Text, palette::PALETTE};

                // Simple text widget that will fade in
                let text = Text::new("Fade Demo", DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background));

                // Create a fader starting faded out
                let mut fader = Fader::new_faded_out(text);
                // Start the fade-in immediately
                fader.start_fade_in(1000);

                $run_macro!(fader);
            }
            "fade_switcher" => {
                use $crate::{FadeSwitcher, Container, Padding, palette::PALETTE};
                use embedded_graphics::prelude::*;
                use embedded_graphics::pixelcolor::RgbColor;

                struct FadeSwitcherDemo {
                    fade_switcher: FadeSwitcher<Center<Container<Padding<Text>>>>,
                    last_switch_time: Option<Instant>,
                    showing_a: bool,
                }

                impl FadeSwitcherDemo {
                    fn new() -> Self {
                        let text = Text::new(
                            "Lorem ipsum\ndolor sit\namet,\nconsectetur\nadipiscing",
                            DefaultTextStyle::new(FONT_SMALL, PALETTE.on_background)
                        );
                        let padded = Padding::all(10, text);
                        let widget_a = Container::new(padded)
                            .with_fill(PALETTE.surface)
                            .with_border(PALETTE.primary, 2);

                        Self {
                            fade_switcher: FadeSwitcher::new(Center::new(widget_a), 500),
                            last_switch_time: None,
                            showing_a: true,
                        }
                    }
                }

                impl $crate::DynWidget for FadeSwitcherDemo {
                    fn set_constraints(&mut self, max_size: Size) {
                        self.fade_switcher.set_constraints(max_size);
                    }

                    fn sizing(&self) -> $crate::Sizing {
                        self.fade_switcher.sizing()
                    }

                    fn force_full_redraw(&mut self) {
                        self.fade_switcher.force_full_redraw();
                    }
                }

                impl $crate::Widget for FadeSwitcherDemo {
                    type Color = Rgb565;

                    fn draw<D>(
                        &mut self,
                        target: &mut SuperDrawTarget<D, Self::Color>,
                        current_time: Instant,
                    ) -> Result<(), D::Error>
                    where
                        D: DrawTarget<Color = Self::Color>,
                    {
                        if self.last_switch_time.is_none() {
                            self.last_switch_time = Some(current_time);
                        }

                        let elapsed = current_time.saturating_duration_since(self.last_switch_time.unwrap());

                        if elapsed >= 3000 {
                            if self.showing_a {
                                let pink = Rgb565::new(31, 20, 31);
                                let text_b = Text::new("", DefaultTextStyle::new(FONT_SMALL, PALETTE.on_background));
                                let padded_b = Padding::all(0, text_b);
                                let widget_b = Container::with_size(padded_b, Size::new(20, 20))
                                    .with_fill(pink);
                                self.fade_switcher.switch_to(Center::new(widget_b));
                            } else {
                                let text = Text::new(
                                    "Lorem ipsum\ndolor sit\namet,\nconsectetur\nadipiscing",
                                    DefaultTextStyle::new(FONT_SMALL, PALETTE.on_background)
                                );
                                let padded = Padding::all(10, text);
                                let widget_a = Container::new(padded)
                                    .with_fill(PALETTE.surface)
                                    .with_border(PALETTE.primary, 2);
                                self.fade_switcher.switch_to(Center::new(widget_a));
                            }
                            self.showing_a = !self.showing_a;
                            self.last_switch_time = Some(current_time);
                        }

                        self.fade_switcher.draw(target, current_time)
                    }
                }

                let widget = FadeSwitcherDemo::new();
                $run_macro!(widget);
            }
            "device_name" => {
                use $crate::DeviceNameScreen;

                // Create device name screen with a long name to test
                let mut device_name_screen = DeviceNameScreen::new("Frank L".into());

                $run_macro!(device_name_screen);
            }
            "bobbing_icon" => {
                use $crate::bobbing_carat::BobbingCarat;

                // Create the bobbing carat widget with colors
                let bobbing_carat = BobbingCarat::new(PALETTE.on_background, PALETTE.background);

                // Center it on screen
                let centered = Center::new(bobbing_carat);

                $run_macro!(centered);
            }
            "swipe_up_chevron" => {
                use $crate::SwipeUpChevron;

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
                use $crate::sign_prompt::SignTxPrompt;
                use frostsnap_core::bitcoin_transaction::PromptSignBitcoinTx;
                use core::str::FromStr;

                // Create dummy transaction data with all address types

                // P2TR - Taproot address (starts with bc1p)
                let p2tr_address = bitcoin::Address::from_str("bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297")
                    .unwrap()
                    .assume_checked();

                // P2WSH - Native SegWit script hash (starts with bc1q, longer)
                let p2wsh_address = bitcoin::Address::from_str("bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3")
                    .unwrap()
                    .assume_checked();

                // P2WPKH - Native SegWit pubkey hash (starts with bc1q, shorter)
                let p2wpkh_address = bitcoin::Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
                    .unwrap()
                    .assume_checked();

                // P2SH - Script hash (starts with 3)
                let p2sh_address = bitcoin::Address::from_str("3EktnHQD7RiAE6uzMj2ZifT9YgRrkSgzQX")
                    .unwrap()
                    .assume_checked();

                // P2PKH - Legacy pubkey hash (starts with 1)
                let p2pkh_address = bitcoin::Address::from_str("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa")
                    .unwrap()
                    .assume_checked();

                let prompt = PromptSignBitcoinTx {
                    foreign_recipients: $crate::alloc::vec![
                        (p2tr_address, bitcoin::Amount::from_sat(100_000)),   // 0.001 BTC
                        (p2wsh_address, bitcoin::Amount::from_sat(200_000)),  // 0.002 BTC
                        (p2wpkh_address, bitcoin::Amount::from_sat(300_000)), // 0.003 BTC
                        (p2sh_address, bitcoin::Amount::from_sat(400_000)),   // 0.004 BTC
                        (p2pkh_address, bitcoin::Amount::from_sat(500_000)),  // 0.005 BTC
                    ],
                    fee: bitcoin::Amount::from_sat(80_000), // 0.00080 BTC (>5% fee triggers warning)
                    fee_rate_sats_per_vbyte: Some(12.5), // Example: 12.5 sats/vB fee rate
                };

                // Create the sign prompt widget
                let widget = SignTxPrompt::new(prompt);

                $run_macro!(widget);
            }
            "bitcoin_amount" => {
                use $crate::{bitcoin_amount_display::BitcoinAmountDisplay, Column, MainAxisAlignment};

                // Create a simple BitcoinAmountDisplay with 21 BTC
                let amount_display = BitcoinAmountDisplay::new(21_000_000); // 21 BTC

                // Put it in a Column with MainAxisAlignment::Center like in sign_prompt
                let widget = Column::new((amount_display,))
                    .with_main_axis_alignment(MainAxisAlignment::Center);

                $run_macro!(widget);
            }
            "slide_in" => {
                use $crate::{PageSlider, WidgetList};
                use embedded_graphics::prelude::*;
                use embedded_graphics::pixelcolor::Rgb565;

                // Type aliases to simplify the complex nested types
                type StyledText = Text;
                type NumberRow = Row<(StyledText, StyledText)>;
                type ThreeRowColumn = Column<(NumberRow, NumberRow, NumberRow)>;
                type PageWidget = Center<Container<ThreeRowColumn>>;

                // Create a WidgetList that generates column widgets with rows on the fly
                struct InfiniteTextPages;

                impl WidgetList<PageWidget> for InfiniteTextPages {
                    fn len(&self) -> usize {
                        usize::MAX // Infinite pages!
                    }

                    fn get(&self, index: usize) -> Option<PageWidget> {
                        let number_words = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
                                            "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen",
                                            "seventeen", "eighteen", "nineteen", "twenty"];

                        let start_num = index * 3 + 1; // Each page has 3 items

                        // Create three rows with number and word
                        let row1 = Row::new((
                            Text::new(
                                $crate::alloc::format!("{}.", start_num),
                                DefaultTextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                            ),
                            Text::new(
                                number_words.get(start_num).unwrap_or(&"many").to_string(),
                                DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                            )
                        ));

                        let row2 = Row::new((
                            Text::new(
                                $crate::alloc::format!("{}.", start_num + 1),
                                DefaultTextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                            ),
                            Text::new(
                                number_words.get(start_num + 1).unwrap_or(&"many").to_string(),
                                DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                            )
                        ));

                        let row3 = Row::new((
                            Text::new(
                                $crate::alloc::format!("{}.", start_num + 2),
                                DefaultTextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                            ),
                            Text::new(
                                number_words.get(start_num + 2).unwrap_or(&"many").to_string(),
                                DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                            )
                        ));

                        let column = Column::new((row1, row2, row3));
                        let container = Container::new(column)
                            .with_border(PALETTE.primary, 2);
                        Some(Center::new(container))
                    }
                }

                // Create the PageSlider with infinite text pages
                let page_slider = PageSlider::new(InfiniteTextPages);
                let widget = page_slider;

                $run_macro!(widget);
            }
            "firmware_upgrade_progress" | "firmware_upgrade_download" => {
                use $crate::{ firmware_upgrade::FirmwareUpgradeProgress, Padding };

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
                    fn set_constraints(&mut self, max_size: Size) {
                        self.indicator.set_constraints(max_size);
                    }

                    fn sizing(&self) -> $crate::Sizing {
                        self.indicator.sizing()
                    }

                    fn force_full_redraw(&mut self) {
                        self.indicator.force_full_redraw()
                    }
                }

                impl Widget for AnimatedProgress {
                    type Color = Rgb565;

                    fn draw<D>(
                        &mut self,
                        target: &mut SuperDrawTarget<D, Self::Color>,
                        current_time: $crate::Instant,
                    ) -> Result<(), D::Error>
                    where
                        D: DrawTarget<Color = Self::Color>, {
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

                let widget = FirmwareUpgradeConfirm::new(firmware_digest, size_bytes);

                $run_macro!(widget);
            }
            "sign_test_message" => {
                use $crate::SignMessageConfirm;

                let test_message = "This is a very long test message that will be wrapped across multiple lines to demonstrate the StringWrap functionality and visual clipping in the container box.";
                let widget = SignMessageConfirm::new(test_message.to_string());

                $run_macro!(widget);
            }
            "all_words" => {
                use $crate::backup::AllWordsPage;

                // Use the actual AllWordsPage widget with test word indices
                let all_words_page = AllWordsPage::new(&TEST_WORD_INDICES, 42);
                $run_macro!(all_words_page);
            }
            "row_inside_column" => {
                use $crate::{text::Text, Column, Row, Container, palette::PALETTE};

                // Create three rows with number and word
                let row1 = Row::new((
                    Text::new(
                        "1.",
                        DefaultTextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                    ),
                    Text::new(
                        "one",
                        DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                    )
                ));

                let row2 = Row::new((
                    Text::new(
                        "2.",
                        DefaultTextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                    ),
                    Text::new(
                        "two",
                        DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                    )
                ));

                let row3 = Row::new((
                    Text::new(
                        "3.",
                        DefaultTextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                    ),
                    Text::new(
                        "three",
                        DefaultTextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                    )
                ));

                let column = Column::new((row1, row2, row3));
                let widget = Container::new(column)
                    .with_border(PALETTE.primary, 2);

                $run_macro!(widget);
            }
            "stack" => {
                use $crate::{Stack, Alignment, Container, text::Text, palette::PALETTE};
                use embedded_graphics::primitives::{Rectangle, PrimitiveStyle};

                // Create a background container
                let background = Container::with_size(
                    Text::new("Background", DefaultTextStyle::new(FONT_LARGE, PALETTE.surface_variant)),
                    Size::new(200, 150)
                )
                .with_fill(PALETTE.surface)
                .with_border(PALETTE.primary, 2);

                // Create some text to overlay
                let centered_text = Text::new(
                    "Centered",
                    DefaultTextStyle::new(FONT_MED, PALETTE.primary)
                ).with_alignment(embedded_graphics::text::Alignment::Center);

                // Create a small icon-like widget positioned at top-right
                let icon = Container::with_size(
                    Text::new("!", DefaultTextStyle::new(FONT_SMALL, PALETTE.on_background)),
                    Size::new(20, 20)
                )
                .with_fill(PALETTE.error)
                .with_corner_radius(Size::new(10, 10));

                // Build the stack
                let stack = Stack::builder()
                    .push(background)
                    .push(centered_text)  // This will be centered
                    .push_positioned(icon, 170, 10)  // Position in top-right
                    .with_alignment(Alignment::Center);

                let widget = Center::new(stack);
                $run_macro!(widget);
            }
            "array_column" => {
                use $crate::{text::Text, Column, palette::PALETTE};
                use embedded_graphics::prelude::*;

                // Create a column from a fixed-size array
                let texts = [
                    Text::new("First", DefaultTextStyle::new(FONT_MED, PALETTE.on_background)),
                    Text::new("Second", DefaultTextStyle::new(FONT_MED, PALETTE.tertiary)),
                    Text::new("Third", DefaultTextStyle::new(FONT_MED, PALETTE.on_background)),
                    Text::new("Fourth", DefaultTextStyle::new(FONT_MED, PALETTE.tertiary)),
                    Text::new("Fifth", DefaultTextStyle::new(FONT_MED, PALETTE.on_background)),
                ];

                let widget = Column::new(texts)
                    .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center);

                $run_macro!(widget);
            }
            "word_selector" => {
                use $crate::backup::WordSelector;
                use frost_backup::bip39_words::words_with_prefix;

                // Get all words starting with "CAR" (BIP39 words are uppercase)
                let words = words_with_prefix("CAR");
                let widget = WordSelector::new(words, "CAR");

                $run_macro!(widget);
            }
            "vec_column" => {
                use $crate::{text::Text, Column, Switcher, palette::PALETTE, DynWidget, Widget};
                use $crate::alloc::vec::Vec;
                use embedded_graphics::prelude::*;

                // Interactive demo that adds text widgets on touch
                struct VecColumnDemo {
                    texts: Vec<Text>,
                    switcher: Switcher<Align<Column<Vec<Text>>>>,
                    touch_count: usize,
                }

                impl VecColumnDemo {
                    fn new() -> Self {
                        // Start with one text widget
                        let mut texts = Vec::new();
                        texts.push(Text::new(
                            "Touch to add more!",
                            DefaultTextStyle::new(FONT_MED, PALETTE.on_background)
                        ));

                        // Create initial column
                        let column = Align::new(
                            Column::new(texts.clone())
                                .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly)
                        ).alignment(Alignment::TopCenter);

                        Self {
                            texts,
                            switcher: Switcher::new(column),
                            touch_count: 0,
                        }
                    }

                    fn add_text(&mut self) {
                        self.touch_count += 1;

                        // Add new text to the vec
                        self.texts.push(Text::new(
                            match self.touch_count {
                                1 => "First touch!",
                                2 => "Second touch!",
                                3 => "Third touch!",
                                4 => "Fourth touch!",
                                5 => "Fifth touch!",
                                6 => "Sixth touch!",
                                7 => "Seventh touch!",
                                8 => "Eighth touch!",
                                9 => "Ninth touch!",
                                _ => "Many touches!",
                            },
                            DefaultTextStyle::new(FONT_MED, PALETTE.tertiary)
                        ));

                        // Create NEW column with the updated vec (do not mutate existing!)
                        let new_column = Align::new(
                            Column::new(self.texts.clone())
                                .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly)
                        ).alignment(Alignment::TopCenter);

                        // Switch to the new column
                        self.switcher.switch_to(new_column);
                    }
                }

                impl DynWidget for VecColumnDemo {
                    fn set_constraints(&mut self, max_size: Size) {
                        self.switcher.set_constraints(max_size);
                    }

                    fn sizing(&self) -> $crate::Sizing {
                        self.switcher.sizing()
                    }

                    fn handle_touch(
                        &mut self,
                        _point: Point,
                        _current_time: Instant,
                        is_release: bool,
                    ) -> Option<$crate::KeyTouch> {
                        if !is_release {
                            self.add_text();
                        }
                        None
                    }

                    fn force_full_redraw(&mut self) {
                        self.switcher.force_full_redraw();
                    }
                }

                impl Widget for VecColumnDemo {
                    type Color = Rgb565;

                    fn draw<D>(
                        &mut self,
                        target: &mut SuperDrawTarget<D, Self::Color>,
                        current_time: Instant,
                    ) -> Result<(), D::Error>
                    where
                        D: DrawTarget<Color = Self::Color>,
                    {
                        self.switcher.draw(target, current_time)
                    }
                }

                let widget = VecColumnDemo::new();
                $run_macro!(widget);
            }
            "address" | "address_display" => {
                use $crate::AddressDisplay;
                use bitcoin::Address;
                use core::str::FromStr;

                // Sample Bitcoin address (just the address, no derivation path)
                let address_str = "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh";
                let address = Address::from_str(address_str).unwrap().assume_checked();

                let widget = AddressDisplay::new(address);
                $run_macro!(widget);
            }
            "address_with_path" => {
                use $crate::AddressWithPath;
                use bitcoin::Address;
                use core::str::FromStr;

                // Sample Taproot address with actual device derivation path
                // Path format: account_kind/account_index/keychain/address_index
                // For Taproot (Segwitv1): 0/0/0/0 = first external address
                let address_str = "bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297";
                let address = Address::from_str(address_str).unwrap().assume_checked();
                let derivation_path = "0/0/0/0".to_string();
                let index = 0;

                let widget = AddressWithPath::new_with_seed(address, derivation_path, index, 42);
                $run_macro!(widget);
            }
            "taproot_address" => {
                use $crate::AddressDisplay;
                use bitcoin::Address;
                use core::str::FromStr;

                // Sample Taproot Bitcoin address
                let address_str = "bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297";
                let address = Address::from_str(address_str).unwrap().assume_checked();

                let widget = AddressDisplay::new(address);
                $run_macro!(widget);
            }
            "p2pkh_address" => {
                use $crate::AddressDisplay;
                use bitcoin::Address;
                use core::str::FromStr;

                // Sample P2PKH (legacy) Bitcoin address
                let address_str = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
                let address = Address::from_str(address_str).unwrap().assume_checked();

                let widget = AddressDisplay::new(address);
                $run_macro!(widget);
            }
            "standby" => {
                use $crate::Standby;
                use frostsnap_core::{message::HeldShare, device::KeyPurpose, schnorr_fun::frost::SecretShare};
                use bitcoin;

                // Create a stub HeldShare for demo purposes (normal mode with access structure)
                let held_share = HeldShare {
                    access_structure_ref: Some(frostsnap_core::AccessStructureRef {
                        key_id: frostsnap_core::KeyId([0u8; 32]),
                        access_structure_id: frostsnap_core::AccessStructureId([1u8; 32]),
                    }),
                    // ShareImage is just a Point, we can use a generator point for demo
                    share_image: {
                        use frostsnap_core::schnorr_fun::fun::prelude::*;
                        SecretShare {
                            index: Scalar::<Public, NonZero>::one(),
                            share: Scalar::<Secret, Zero>::zero(),
                        }.share_image()
                    },
                    threshold: 2,
                    key_name: "Family Wallet".to_string(),
                    purpose: KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin),
                };

                let device_name = "Alice";
                let widget = Standby::new(device_name, held_share);
                $run_macro!(widget);
            }
            "standby_recovery" => {
                use $crate::Standby;
                use frostsnap_core::{message::HeldShare, device::KeyPurpose, schnorr_fun::frost::SecretShare};
                use bitcoin;

                // Create a stub HeldShare for demo purposes (recovery mode - no access structure)
                let held_share = HeldShare {
                    access_structure_ref: None,
                    // ShareImage is just a Point, we can use a generator point for demo
                    share_image: {
                        use frostsnap_core::schnorr_fun::fun::prelude::*;
                        SecretShare {
                            index: Scalar::<Public, NonZero>::one(),
                            share: Scalar::<Secret, Zero>::zero(),
                        }.share_image()
                    },
                    threshold: 2,
                    key_name: "Family Wallet".to_string(),
                    purpose: KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin),
                };

                let device_name = "Alice";
                let widget = Standby::new(device_name, held_share);
                $run_macro!(widget);
            }
            "device_name_cursor" => {
                use $crate::device_name::DeviceName;

                // Create DeviceName widget with cursor enabled
                let mut device_name = DeviceName::new("Frank L");
                device_name.enable_cursor();

                $run_macro!(device_name);
            }
            "screen_test" => {
                use $crate::screen_test::ScreenTest;

                // Create DeviceName widget with cursor enabled
                let mut screen_test = ScreenTest::new();

                $run_macro!(screen_test);
            }
            "multiline_string" => {
                use $crate::text::Text;

                let multiline_text = Text::new(
                    "This is a\nmultiline\nstring      with\nGray4 font",
                    DefaultTextStyle::new(FONT_MED, PALETTE.on_background)
                ).with_alignment(embedded_graphics::text::Alignment::Center);

                let container = Container::new(multiline_text).with_border(PALETTE.primary, 2);
                let widget = Center::new(container);

                $run_macro!(widget);
            }
            "wipe_device" => {
                use $crate::WipeDevice;

                let widget = WipeDevice::new();
                $run_macro!(widget);
            }
            _ => {
                panic!("Unknown demo: '{}'. Valid demos: hello_world, bip39_entry, log_touches, numeric_keyboard, hold_confirm, welcome, column_cross_axis, column_center, row_cross_axis, row_center, row_inside_column, bip39_backup, all_words, fade_in, fade_switcher, device_name, device_name_cursor, bobbing_icon, swipe_up_chevron, keygen_check, sign_prompt, bitcoin_amount, slide_in, firmware_upgrade_progress, firmware_upgrade_download, firmware_upgrade_erase, firmware_upgrade_passive, progress, firmware_upgrade, array_column, vec_column, word_selector, address, screen_test, multiline_string, wipe_device, standby, standby_recovery", $demo);
            }
        }
    };
}
