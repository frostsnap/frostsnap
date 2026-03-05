use crate::{
    gray4_style::Gray4TextStyle, palette::PALETTE, Center, Column, CrossAxisAlignment, GrayToAlpha,
    HoldToConfirm, HoldToConfirmColors, Image, MainAxisAlignment, PageSlider, Row, SizedBox, Text,
    HOLD_TO_CONFIRM_TIME_LONG_MS,
};
use alloc::string::ToString;
use embedded_graphics::{
    geometry::Size,
    pixelcolor::{Gray8, Rgb565},
};
use tinybmp::Bmp;

type WarningIcon = Image<GrayToAlpha<Bmp<'static, Gray8>, Rgb565>>;

const WARNING_ICON_DATA: &[u8] = include_bytes!("../assets/warning-icon-24x24.bmp");

const ERASE_BUTTON_FILL_COLOR: Rgb565 = Rgb565::new(25, 8, 4);
const ERASE_BUTTON_BORDER_COLOR: Rgb565 = Rgb565::new(31, 14, 8);

#[derive(Clone, frostsnap_macros::Widget)]
pub struct EraseWarningPage {
    #[widget_delegate]
    center: Center<
        Column<(
            Row<(WarningIcon, Text<Gray4TextStyle>)>,
            Text<Gray4TextStyle>,
            Column<(Text<Gray4TextStyle>, Text<Gray4TextStyle>)>,
        )>,
    >,
}

impl EraseWarningPage {
    fn new() -> Self {
        let warning_icon = Image::new(GrayToAlpha::new(
            Bmp::<Gray8>::from_slice(WARNING_ICON_DATA).expect("Failed to load warning BMP"),
            PALETTE.warning,
        ));

        let warning_text = Text::new(
            "Warning".to_string(),
            Gray4TextStyle::new(
                &frostsnap_fonts::NOTO_SANS_18_MEDIUM,
                ERASE_BUTTON_BORDER_COLOR,
            ),
        );

        let warning_row = Row::builder()
            .push(warning_icon)
            .gap(4)
            .push(warning_text)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::End);

        let title_text = Text::new(
            "Erase Device".to_string(),
            Gray4TextStyle::new(&frostsnap_fonts::NOTO_SANS_24_BOLD, PALETTE.on_background),
        );

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

        let column = Column::builder()
            .push(warning_row)
            .gap(10)
            .push(title_text)
            .gap(10)
            .push(warning_text)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        Self {
            center: Center::new(column),
        }
    }
}

#[derive(Clone, frostsnap_macros::Widget)]
pub struct EraseConfirmationPage {
    #[widget_delegate]
    hold_confirm: HoldToConfirm<
        Column<(
            SizedBox<Rgb565>,
            Row<(WarningIcon, Text<Gray4TextStyle>)>,
            Text<Gray4TextStyle>,
            Column<(Text<Gray4TextStyle>, Text<Gray4TextStyle>)>,
            SizedBox<Rgb565>,
        )>,
    >,
}

impl EraseConfirmationPage {
    fn new() -> Self {
        let warning_icon = Image::new(GrayToAlpha::new(
            Bmp::<Gray8>::from_slice(WARNING_ICON_DATA).expect("Failed to load warning BMP"),
            PALETTE.warning,
        ));

        let warning_label = Text::new(
            "Warning".to_string(),
            Gray4TextStyle::new(
                &frostsnap_fonts::NOTO_SANS_18_MEDIUM,
                ERASE_BUTTON_BORDER_COLOR,
            ),
        );

        let warning_row = Row::builder()
            .push(warning_icon)
            .gap(4)
            .push(warning_label)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::End);

        let erase_text = Text::new(
            "Hold to Erase Device".to_string(),
            Gray4TextStyle::new(&frostsnap_fonts::NOTO_SANS_18_MEDIUM, PALETTE.on_background),
        );

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

        let confirm_content = Column::builder()
            .push(SizedBox::<Rgb565>::new(Size::new(1, 20)))
            .push(warning_row)
            .gap(15)
            .push(erase_text)
            .gap(15)
            .push(press_text)
            .push(SizedBox::<Rgb565>::new(Size::new(1, 20)))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        // 🎨 border brighter than fill, matching the green confirm pattern
        let hold_confirm = HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_LONG_MS, confirm_content)
            .with_colors(HoldToConfirmColors {
                border: ERASE_BUTTON_BORDER_COLOR,
                button_fill: ERASE_BUTTON_FILL_COLOR,
                button_stroke: ERASE_BUTTON_BORDER_COLOR,
                checkmark: PALETTE.on_error,
            })
            .with_faded_out_button();

        Self { hold_confirm }
    }

    pub fn is_confirmed(&self) -> bool {
        self.hold_confirm.is_completed()
    }

    pub fn fade_in_button(&mut self) {
        self.hold_confirm.fade_in_button();
    }
}

#[derive(frostsnap_macros::Widget)]
pub struct EraseDevice {
    #[widget_delegate]
    page_slider: PageSlider<(EraseWarningPage, EraseConfirmationPage)>,
}

impl EraseDevice {
    pub fn new() -> Self {
        let page_list = (EraseWarningPage::new(), EraseConfirmationPage::new());
        let page_slider = PageSlider::new(page_list)
            .with_on_page_ready(|page| {
                if let Some(confirmation_page) = page.downcast_mut::<EraseConfirmationPage>() {
                    confirmation_page.fade_in_button();
                }
            })
            .with_swipe_up_chevron();

        Self { page_slider }
    }

    pub fn is_confirmed(&mut self) -> bool {
        let page = self.page_slider.current_widget();
        if let Some(confirmation_page) = page.downcast_ref::<EraseConfirmationPage>() {
            return confirmation_page.is_confirmed();
        }
        false
    }
}

impl Default for EraseDevice {
    fn default() -> Self {
        Self::new()
    }
}
