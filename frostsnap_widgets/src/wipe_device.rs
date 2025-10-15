use crate::{
    any_of::AnyOf, gray4_style::Gray4TextStyle, palette::PALETTE, Center, Column,
    CrossAxisAlignment, DynWidget, HoldToConfirm, Instant, MainAxisAlignment, PageSlider, Row,
    SizedBox, SuperDrawTarget, Text, Widget, WidgetList, HOLD_TO_CONFIRM_TIME_LONG_MS,
};
use alloc::string::ToString;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
};

// Warning colors for destructive wipe action
/// Darker red for button fill in wipe confirmation
const WIPE_BUTTON_FILL_COLOR: Rgb565 = Rgb565::new(25, 8, 4);
/// Brighter red for border/outline in wipe confirmation
const WIPE_BUTTON_BORDER_COLOR: Rgb565 = Rgb565::new(31, 14, 8);

/// Warning page for wipe device
#[derive(frostsnap_macros::Widget)]
pub struct WipeWarningPage {
    #[widget_delegate]
    center: Center<
        Column<(
            Row<(
                Text<Gray4TextStyle>, // Warning icon
                SizedBox<Rgb565>,
                Column<(SizedBox<Rgb565>, Text<Gray4TextStyle>)>, // "Warning" text with spacer
            )>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle>, // Title
            SizedBox<Rgb565>,
            Column<(Text<Gray4TextStyle>, Text<Gray4TextStyle>)>, // Warning message split into two lines
        )>,
    >,
}

impl WipeWarningPage {
    fn new() -> Self {
        // Use the warning icon as a Gray4 font glyph
        let warning_icon = Text::new(
            "⚠".to_string(),
            Gray4TextStyle::new(&frostsnap_fonts::WARNING_ICON, PALETTE.warning),
        );

        let icon_spacer = SizedBox::<Rgb565>::new(Size::new(5, 1)); // 5px horizontal spacing

        let warning_text = Text::new(
            "Warning".to_string(),
            Gray4TextStyle::new(
                &frostsnap_fonts::NOTO_SANS_18_MEDIUM,
                WIPE_BUTTON_BORDER_COLOR,
            ), // Same red as border
        );

        // Add a small spacer above the text to compensate for lack of descenders
        let text_top_spacer = SizedBox::<Rgb565>::new(Size::new(1, 5)); // Match sign_prompt spacing
        let text_with_spacer = Column::new((text_top_spacer, warning_text));

        // Put icon, spacer, and text column on same row
        let warning_row = Row::new((warning_icon, icon_spacer, text_with_spacer))
            .with_main_axis_alignment(MainAxisAlignment::Center);

        let spacer1 = SizedBox::<Rgb565>::new(Size::new(1, 10)); // Space after warning row (match spacer2 below)

        // Title in white
        let title_text = Text::new(
            "Wipe Device".to_string(),
            Gray4TextStyle::new(&frostsnap_fonts::NOTO_SANS_24_BOLD, PALETTE.on_background),
        );

        let spacer2 = SizedBox::<Rgb565>::new(Size::new(1, 10)); // Space after title

        // Warning message in grey - split into two lines
        let warning_line1 = Text::new(
            "This will permanently".to_string(),
            Gray4TextStyle::new(
                &frostsnap_fonts::NOTO_SANS_17_REGULAR,
                PALETTE.text_secondary,
            ),
        );
        let warning_line2 = Text::new(
            "delete all secret key data".to_string(),
            Gray4TextStyle::new(
                &frostsnap_fonts::NOTO_SANS_17_REGULAR,
                PALETTE.text_secondary,
            ),
        );
        let warning_text = Column::new((warning_line1, warning_line2))
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let column = Column::new((warning_row, spacer1, title_text, spacer2, warning_text))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        Self {
            center: Center::new(column),
        }
    }
}

/// Confirmation page for wipe device with red hold-to-confirm
pub struct WipeConfirmationPage {
    hold_confirm: HoldToConfirm<
        Column<(
            SizedBox<Rgb565>, // spacer0
            Row<(
                Text<Gray4TextStyle>, // Warning icon
                SizedBox<Rgb565>,
                Column<(SizedBox<Rgb565>, Text<Gray4TextStyle>)>, // "Warning" text with spacer
            )>,
            SizedBox<Rgb565>,                                     // spacer1
            Text<Gray4TextStyle>,                                 // wipe_text
            SizedBox<Rgb565>,                                     // spacer2
            Column<(Text<Gray4TextStyle>, Text<Gray4TextStyle>)>, // press_text split into two lines
            SizedBox<Rgb565>,                                     // spacer3
        )>,
    >,
    fade_started: bool,
}

impl WipeConfirmationPage {
    fn new() -> Self {
        // Warning icon and text at top
        let warning_icon = Text::new(
            "⚠".to_string(),
            Gray4TextStyle::new(&frostsnap_fonts::WARNING_ICON, PALETTE.warning),
        );

        let icon_spacer = SizedBox::<Rgb565>::new(Size::new(5, 1));

        let warning_label = Text::new(
            "Warning".to_string(),
            Gray4TextStyle::new(
                &frostsnap_fonts::NOTO_SANS_18_MEDIUM,
                WIPE_BUTTON_BORDER_COLOR,
            ), // Same red as border
        );

        // Add a small spacer above the text to compensate for lack of descenders
        let text_top_spacer = SizedBox::<Rgb565>::new(Size::new(1, 5)); // Match warning page spacing
        let text_with_spacer = Column::new((text_top_spacer, warning_label));

        let warning_row = Row::new((warning_icon, icon_spacer, text_with_spacer))
            .with_main_axis_alignment(MainAxisAlignment::Center);

        let spacer0 = SizedBox::<Rgb565>::new(Size::new(1, 40)); // Space before warning row

        let spacer1 = SizedBox::<Rgb565>::new(Size::new(1, 15)); // Space between warning and "Hold to Wipe"

        let wipe_text = Text::new(
            "Hold to Wipe Device".to_string(),
            Gray4TextStyle::new(&frostsnap_fonts::NOTO_SANS_18_MEDIUM, PALETTE.on_background),
        );

        let spacer2 = SizedBox::<Rgb565>::new(Size::new(1, 15)); // Match sign_prompt spacing

        // Split press text into two lines (matching sign_prompt style)
        let press_line1 = Text::new(
            "Press and hold".to_string(),
            Gray4TextStyle::new(
                &frostsnap_fonts::NOTO_SANS_17_REGULAR,
                PALETTE.text_secondary,
            ),
        );
        let press_line2 = Text::new(
            "for 8 seconds".to_string(),
            Gray4TextStyle::new(
                &frostsnap_fonts::NOTO_SANS_17_REGULAR,
                PALETTE.text_secondary,
            ),
        );
        let press_text = Column::new((press_line1, press_line2))
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let spacer3 = SizedBox::<Rgb565>::new(Size::new(1, 40)); // Match sign_prompt spacing

        let confirm_content = Column::new((
            spacer0,
            warning_row,
            spacer1,
            wipe_text,
            spacer2,
            press_text,
            spacer3,
        ))
        .with_main_axis_alignment(MainAxisAlignment::Center)
        .with_cross_axis_alignment(CrossAxisAlignment::Center);

        // 8 second hold time for safety with red colors for the destructive action
        // Use same color relationship as green: border is brighter than fill
        // Green: fill(2,34,9) -> border(3,46,16)
        // Red: fill(25,8,4) -> border calculated proportionally
        let hold_confirm = HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_LONG_MS, confirm_content)
            .with_custom_colors(
                WIPE_BUTTON_BORDER_COLOR, // Brighter red for border progress
                WIPE_BUTTON_FILL_COLOR,   // Darker red for button fill
                WIPE_BUTTON_BORDER_COLOR, // Brighter red for button outline
            )
            .with_faded_out_button();

        Self {
            hold_confirm,
            fade_started: false,
        }
    }

    pub fn is_confirmed(&self) -> bool {
        self.hold_confirm.is_completed()
    }

    pub fn fade_in_button(&mut self) {
        if !self.fade_started {
            self.hold_confirm.fade_in_button();
            self.fade_started = true;
        }
    }
}

impl DynWidget for WipeConfirmationPage {
    fn set_constraints(&mut self, max_size: Size) {
        self.hold_confirm.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.hold_confirm.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.hold_confirm
            .handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.hold_confirm
            .handle_vertical_drag(prev_y, new_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        self.hold_confirm.force_full_redraw();
    }
}

impl Widget for WipeConfirmationPage {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.hold_confirm.draw(target, current_time)
    }
}

/// Type alias for the wipe device pages
type WipeDevicePage = AnyOf<(WipeWarningPage, WipeConfirmationPage)>;

/// List of pages for wipe device flow
pub struct WipeDevicePageList;

impl WidgetList<WipeDevicePage> for WipeDevicePageList {
    fn len(&self) -> usize {
        2 // Warning page and confirmation page
    }

    fn get(&self, index: usize) -> Option<WipeDevicePage> {
        match index {
            0 => Some(AnyOf::new(WipeWarningPage::new())),
            1 => Some(AnyOf::new(WipeConfirmationPage::new())),
            _ => None,
        }
    }
}

/// Main wipe device widget with page slider
#[derive(frostsnap_macros::Widget)]
pub struct WipeDevice {
    #[widget_delegate]
    page_slider: PageSlider<WipeDevicePageList, WipeDevicePage>,
}

impl WipeDevice {
    pub fn new() -> Self {
        let page_list = WipeDevicePageList;
        let page_slider = PageSlider::new(page_list)
            .with_on_page_ready(|page| {
                // Try to downcast to WipeConfirmationPage
                if let Some(confirmation_page) = page.downcast_mut::<WipeConfirmationPage>() {
                    // Fade in the button when the confirmation page is ready
                    confirmation_page.fade_in_button();
                }
            })
            .with_swipe_up_chevron();

        Self { page_slider }
    }

    /// Check if the user has confirmed the wipe
    pub fn is_confirmed(&mut self) -> bool {
        // Check if we're on the confirmation page and it's confirmed
        let page = self.page_slider.current_widget();
        if let Some(confirmation_page) = page.downcast_ref::<WipeConfirmationPage>() {
            return confirmation_page.is_confirmed();
        }
        false
    }
}

impl Default for WipeDevice {
    fn default() -> Self {
        Self::new()
    }
}
