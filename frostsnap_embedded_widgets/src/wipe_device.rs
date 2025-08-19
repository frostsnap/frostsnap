use crate::{
    PageSlider, WidgetList, Widget, DynWidget, Instant, SuperDrawTarget,
    HoldToConfirm, Text, Column, Row, Center, SizedBox, IconWidget,
    palette::PALETTE, MainAxisAlignment, CrossAxisAlignment, any_of::AnyOf,
    Sizing, KeyTouch,
};
use embedded_graphics::{
    pixelcolor::Rgb565,
    geometry::{Point, Size},
    draw_target::DrawTarget,
    text::Alignment,
};
use u8g2_fonts::U8g2TextStyle;
use alloc::string::{String, ToString};

/// Warning page for wipe device
#[derive(frostsnap_macros::Widget)]
pub struct WipeWarningPage {
    #[widget_delegate]
    center: Center<Column<(
        Row<(
            IconWidget<embedded_iconoir::Icon<Rgb565, embedded_iconoir::icons::size24px::actions::WarningTriangle>>,
            SizedBox<Rgb565>,
            Text<U8g2TextStyle<Rgb565>>,
        )>,
        SizedBox<Rgb565>,
        Text<U8g2TextStyle<Rgb565>>,
        SizedBox<Rgb565>,
        Text<U8g2TextStyle<Rgb565>>,
    )>>,
}

impl WipeWarningPage {
    fn new() -> Self {
        use embedded_iconoir::prelude::*;

        let warning_icon = IconWidget::new(
            embedded_iconoir::icons::size24px::actions::WarningTriangle::new(PALETTE.caution)
        );

        let icon_spacer = SizedBox::<Rgb565>::new(Size::new(5, 1)); // 5px horizontal spacing

        let warning_label = Text::new(
            "Warning".to_string(),
            U8g2TextStyle::new(crate::FONT_MED, PALETTE.error)
        );

        // Put icon, spacer, and "Warning" on same row with bottom alignment
        let warning_row = Row::new((warning_icon, icon_spacer, warning_label))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::End);  // Align to bottom

        let spacer1 = SizedBox::<Rgb565>::new(Size::new(1, 15)); // Space after warning row

        // Title in white
        let title_text = Text::new(
            "Wipe Device".to_string(),
            U8g2TextStyle::new(crate::FONT_LARGE, PALETTE.on_background)
        ).with_alignment(Alignment::Center);

        let spacer2 = SizedBox::<Rgb565>::new(Size::new(1, 10)); // Space after title

        // Warning message in grey
        let warning_text = Text::new(
            "All keys will be\npermanently deleted".to_string(),
            U8g2TextStyle::new(crate::FONT_MED, PALETTE.text_secondary)
        ).with_alignment(Alignment::Center);

        let column = Column::new((
            warning_row,
            spacer1,
            title_text,
            spacer2,
            warning_text,
        )).with_main_axis_alignment(MainAxisAlignment::Center);

        Self {
            center: Center::new(column),
        }
    }
}

/// Confirmation page for wipe device with red hold-to-confirm
pub struct WipeConfirmationPage {
    hold_confirm: HoldToConfirm<Column<(
        SizedBox<Rgb565>,  // spacer0
        Row<(
            IconWidget<embedded_iconoir::Icon<Rgb565, embedded_iconoir::icons::size24px::actions::WarningTriangle>>,
            SizedBox<Rgb565>,
            Text<U8g2TextStyle<Rgb565>>,
        )>,
        SizedBox<Rgb565>,  // spacer1
        Text<U8g2TextStyle<Rgb565>>,  // wipe_text
        SizedBox<Rgb565>,  // spacer2
        Text<U8g2TextStyle<Rgb565>>,  // press_text
        SizedBox<Rgb565>,  // spacer3
    )>>,
    fade_started: bool,
}

impl WipeConfirmationPage {
    fn new() -> Self {
        use embedded_iconoir::prelude::*;

        // Warning icon and text at top
        let warning_icon = IconWidget::new(
            embedded_iconoir::icons::size24px::actions::WarningTriangle::new(PALETTE.caution)
        );

        let icon_spacer = SizedBox::<Rgb565>::new(Size::new(5, 1));

        let warning_label = Text::new(
            "Warning".to_string(),
            U8g2TextStyle::new(crate::FONT_MED, PALETTE.error)
        );

        let warning_row = Row::new((warning_icon, icon_spacer, warning_label))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::End);

        let spacer0 = SizedBox::<Rgb565>::new(Size::new(1, 40)); // Space before warning row

        let spacer1 = SizedBox::<Rgb565>::new(Size::new(1, 15)); // Space between warning and "Hold to Wipe"

        let wipe_text = Text::new("Hold to Wipe", U8g2TextStyle::new(crate::FONT_MED, PALETTE.on_background));

        let spacer2 = SizedBox::<Rgb565>::new(Size::new(1, 15)); // Match sign_transaction spacing

        let press_text = Text::new("Press and hold\nfor 8 seconds", U8g2TextStyle::new(crate::FONT_SMALL, PALETTE.text_secondary))
            .with_alignment(Alignment::Center);

        let spacer3 = SizedBox::<Rgb565>::new(Size::new(1, 40)); // Match sign_transaction spacing

        let confirm_content = Column::new((spacer0, warning_row, spacer1, wipe_text, spacer2, press_text, spacer3))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        // 8 second hold time for safety with red colors for the destructive action
        // Use a darker red for fill and the error color for border
        let red_fill = Rgb565::new(25, 8, 4);     // Darker red for button fill
        let hold_confirm = HoldToConfirm::new_with_colors(
            8000,
            confirm_content,
            PALETTE.error,    // Red border progress
            red_fill,         // Red button fill (darker)
            PALETTE.error     // Red button outline (same as border)
        ).with_faded_out_button();

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

    fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) -> Option<crate::KeyTouch> {
        self.hold_confirm.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.hold_confirm.handle_vertical_drag(prev_y, new_y, is_release);
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
        let page_slider = PageSlider::new(page_list, 0)
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
}

impl Default for WipeDevice {
    fn default() -> Self {
        Self::new()
    }
}