/// Macro for selecting and running demo widgets
#[macro_export]
macro_rules! demo_widget {
    ($demo:expr, $screen_size:expr, $run_macro:ident) => {
        // Common imports for all demos
        use $crate::{
            SuperDrawTarget,
            text::Text, Column, Row, Container, palette::PALETTE,
            MainAxisAlignment, CrossAxisAlignment, Widget,
            HoldToConfirm, center::Center, Padding, SizedBox,
            FONT_SMALL, FONT_MED, FONT_LARGE, Instant,
            Alignment
        };
        use embedded_graphics::{
            prelude::*,
            pixelcolor::{Rgb565, BinaryColor},
        };
        use u8g2_fonts::U8g2TextStyle;
        use $crate::alloc::string::{String, ToString};

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
                let widget = Text::new("Hello World!", U8g2TextStyle::new(FONT_LARGE, PALETTE.on_background));
                $run_macro!(widget);
            }
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
                use embedded_graphics::pixelcolor::BinaryColor;
                let prompt_text = Text::new("Confirm\ntransaction", U8g2TextStyle::new(FONT_MED, PALETTE.on_background)).with_alignment(embedded_graphics::text::Alignment::Center);
                let widget = HoldToConfirm::new(2000, prompt_text);
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
                let text1 = Text::new("cross axis", u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let text2 = Text::new("start", u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let inner_column1 = Column::new((text1, text2))
                    .with_cross_axis_alignment($crate::CrossAxisAlignment::Start);

                // Second column with center cross-axis alignment
                let text3 = Text::new("cross axis", u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let text4 = Text::new("center", u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let inner_column2 = Column::new((text3, text4))
                    .with_cross_axis_alignment($crate::CrossAxisAlignment::Center);

                // Third column with End alignment (right-aligned)
                let text5 = Text::new("cross axis", u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let text6 = Text::new("end", u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let inner_column3 = Column::new((text5, text6))
                    .with_cross_axis_alignment($crate::CrossAxisAlignment::End);

                // Outer column containing all three inner columns (default center alignment)
                let widget = Column::new((inner_column1, inner_column2, inner_column3));
                $run_macro!(widget);
            }
            "row_cross_axis" => {
                use $crate::{text::Text, Row, Column, Container, palette::PALETTE};

                // First row with Start alignment (top-aligned)
                let text1 = Text::new("cross axis", u8g2_fonts::U8g2TextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
                let text2 = Text::new("start", u8g2_fonts::U8g2TextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
                let inner_row1 = Row::new((text1, text2))
                    .with_cross_axis_alignment($crate::CrossAxisAlignment::Start)
                    .with_debug_borders(true);
                let container1 = Container::with_size(inner_row1, Size::new(240, 80))
                    .with_border(PALETTE.primary, 2);

                // Second row with center cross-axis alignment
                let text3 = Text::new("cross axis", u8g2_fonts::U8g2TextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
                let text4 = Text::new("center", u8g2_fonts::U8g2TextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
                let inner_row2 = Row::new((text3, text4))
                    .with_cross_axis_alignment($crate::CrossAxisAlignment::Center)
                    .with_debug_borders(true);
                let container2 = Container::with_size(inner_row2, Size::new(240, 80))
                    .with_border(PALETTE.primary, 2);

                // Third row with End alignment (bottom-aligned)
                let text5 = Text::new("cross axis", u8g2_fonts::U8g2TextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
                let text6 = Text::new("end", u8g2_fonts::U8g2TextStyle::new($crate::FONT_SMALL, PALETTE.on_background));
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
                let text_a = Text::new("A", u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let text_b = Text::new("B", u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let start_row = Row::new((text_a, text_b))
                    .with_main_axis_alignment($crate::MainAxisAlignment::Start)
                    .with_debug_borders(true);
                let start_container = Container::new(start_row).with_border(PALETTE.primary, 2);

                // Second row with Center alignment
                let text_c = Text::new("C", u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let text_d = Text::new("D", u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
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
                let text1 = Text::new("main axis alignment", u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let text2 = Text::new("start", u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let start_column = Column::new((text1, text2)).with_debug_borders(true);
                let start_container = Container::new(start_column).with_border(PALETTE.primary, 2);

                // Second column with Center alignment
                let text3 = Text::new("main axis alignment", u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.on_background));
                let text4 = Text::new("center", u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background));
                let center_column = Column::new((text3, text4))
                    .with_main_axis_alignment($crate::MainAxisAlignment::Center).with_debug_borders(true);
                let center_container = Container::new(center_column).with_border(PALETTE.primary, 2);

                // Outer column containing both containers
                let widget = Column::new((start_container, center_container));
                $run_macro!(widget);
            }
            "bip39_backup" => {
                use $crate::bip39::Bip39BackupDisplay;
                use embedded_graphics::prelude::*;

                let share_index = 42;

                // Create the backup display - it now uses PageSlider internally and outputs Rgb565
                let widget = Bip39BackupDisplay::new(TEST_WORD_INDICES, share_index);

                $run_macro!(widget);
            }
            "fade_in_fade_out" => {
                use $crate::{fader::Fader, text::Text, palette::PALETTE};

                // Simple text widget that will fade in/out
                let text = Text::new("Fade Demo", u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background));

                // Create a fader starting faded out
                let mut fader = Fader::new_faded_out(text);
                // Start the fade-in immediately
                fader.start_fade_in(1000, 50, PALETTE.background);

                $run_macro!(fader);
            }
            "device_name" => {
                use $crate::DeviceNameScreen;

                // Create device name screen with a long name to test
                let mut device_name_screen = DeviceNameScreen::new("Frank L".into());

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
                        (taproot_address, bitcoin::Amount::from_sat(500_001)), // 0.00500001 BTC
                        // (segwit_address, bitcoin::Amount::from_sat(150_000)), // 0.0015 BTC
                        // (legacy_address, bitcoin::Amount::from_sat(50_000)), // 0.0005 BTC
                    ],
                    fee: bitcoin::Amount::from_sat(125_000), // 0.00125 BTC (high fee for demo)
                };

                // Create the sign prompt widget
                let widget = SignPrompt::new(prompt);

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
                use $crate::{PageSlider, WidgetList, Widget, center::Center, text::Text, Column, Row, Container, palette::PALETTE};
                use embedded_graphics::prelude::*;
                use embedded_graphics::pixelcolor::Rgb565;
                use u8g2_fonts::U8g2TextStyle;

                // Type aliases to simplify the complex nested types
                type StyledText = Text<U8g2TextStyle<Rgb565>>;
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
                                U8g2TextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                            ),
                            Text::new(
                                number_words.get(start_num).unwrap_or(&"many").to_string(),
                                U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                            )
                        ));

                        let row2 = Row::new((
                            Text::new(
                                $crate::alloc::format!("{}.", start_num + 1),
                                U8g2TextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                            ),
                            Text::new(
                                number_words.get(start_num + 1).unwrap_or(&"many").to_string(),
                                U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                            )
                        ));

                        let row3 = Row::new((
                            Text::new(
                                $crate::alloc::format!("{}.", start_num + 2),
                                U8g2TextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                            ),
                            Text::new(
                                number_words.get(start_num + 2).unwrap_or(&"many").to_string(),
                                U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                            )
                        ));

                        let column = Column::new((row1, row2, row3));
                        let container = Container::new(column)
                            .with_border(PALETTE.primary, 2);
                        Some(Center::new(container))
                    }
                }

                // Create the PageSlider with infinite text pages
                let page_slider = PageSlider::new(InfiniteTextPages, 100);
                let widget = page_slider;

                $run_macro!(widget);
            }
            "firmware_upgrade_progress" | "firmware_upgrade_download" => {
                use $crate::{ firmware_upgrade::FirmwareUpgradeProgress, Padding };

                // Show downloading state at 65% progress
                let widget = Padding::symmetric(20, 0, FirmwareUpgradeProgress::downloading(0.65)) ;
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
            "all_words" => {
                use $crate::bip39::AllWordsPage;

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
                        u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                    ),
                    Text::new(
                        "one",
                        u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                    )
                ));

                let row2 = Row::new((
                    Text::new(
                        "2.",
                        u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                    ),
                    Text::new(
                        "two",
                        u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
                    )
                ));

                let row3 = Row::new((
                    Text::new(
                        "3.",
                        u8g2_fonts::U8g2TextStyle::new($crate::FONT_MED, PALETTE.text_secondary)
                    ),
                    Text::new(
                        "three",
                        u8g2_fonts::U8g2TextStyle::new($crate::FONT_LARGE, PALETTE.on_background)
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
                    Text::new("Background", U8g2TextStyle::new(FONT_LARGE, PALETTE.surface_variant)),
                    Size::new(200, 150)
                )
                .with_fill(PALETTE.surface)
                .with_border(PALETTE.primary, 2);

                // Create some text to overlay
                let centered_text = Text::new(
                    "Centered",
                    U8g2TextStyle::new(FONT_MED, PALETTE.primary)
                ).with_alignment(embedded_graphics::text::Alignment::Center);

                // Create a small icon-like widget positioned at top-right
                let icon = Container::with_size(
                    Text::new("!", U8g2TextStyle::new(FONT_SMALL, PALETTE.on_background)),
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
                    Text::new("First", U8g2TextStyle::new(FONT_MED, PALETTE.on_background)),
                    Text::new("Second", U8g2TextStyle::new(FONT_MED, PALETTE.tertiary)),
                    Text::new("Third", U8g2TextStyle::new(FONT_MED, PALETTE.on_background)),
                    Text::new("Fourth", U8g2TextStyle::new(FONT_MED, PALETTE.tertiary)),
                    Text::new("Fifth", U8g2TextStyle::new(FONT_MED, PALETTE.on_background)),
                ];
                
                let widget = Column::new(texts)
                    .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center);
                    
                $run_macro!(widget);
            }
            "vec_column" => {
                use $crate::{text::Text, Column, Switcher, palette::PALETTE, DynWidget, Widget};
                use $crate::alloc::vec::Vec;
                use embedded_graphics::prelude::*;

                // Interactive demo that adds text widgets on touch
                struct VecColumnDemo {
                    texts: Vec<Text<u8g2_fonts::U8g2TextStyle<Rgb565>>>,
                    switcher: Switcher<Column<Vec<Text<u8g2_fonts::U8g2TextStyle<Rgb565>>>>>,
                    touch_count: usize,
                }

                impl VecColumnDemo {
                    fn new() -> Self {
                        // Start with one text widget
                        let mut texts = Vec::new();
                        texts.push(Text::new(
                            "Touch to add more!",
                            U8g2TextStyle::new(FONT_MED, PALETTE.on_background)
                        ));

                        // Create initial column
                        let column = Column::new(texts.clone())
                            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

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
                            U8g2TextStyle::new(FONT_MED, PALETTE.tertiary)
                        ));

                        // Create NEW column with the updated vec (do not mutate existing!)
                        let new_column = Column::new(self.texts.clone())
                            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

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
            _ => {
                panic!("Unknown demo: '{}'. Valid demos: bip39_entry, bip39_t9, hold_confirm, checkmark, welcome, column_cross_axis, column_center, row_cross_axis, row_center, row_inside_column, bip39_backup, all_words, fade_in_fade_out, device_name, bobbing_icon, swipe_up_chevron, keygen_check, sign_prompt, bitcoin_amount, slide_in, slide_in_old, progress, firmware_upgrade_progress, firmware_upgrade_download, firmware_upgrade_erase, firmware_upgrade_passive, firmware_upgrade, stack, array_column, vec_column", $demo);
            }
        }
    };
}
