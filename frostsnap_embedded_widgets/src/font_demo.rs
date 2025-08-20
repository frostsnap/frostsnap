use crate::{
    any_of::AnyOf,
    center::Center,
    gray4_text::Gray4Text,
    layout::{Column, MainAxisAlignment},
    noto_sans_24_bold::NOTO_SANS_24_BOLD,
    noto_sans_24_regular::NOTO_SANS_24_REGULAR,
    noto_sans_mono_21_bold::NOTO_SANS_MONO_21_BOLD,
    page_slider::PageSlider,
    WidgetList,
};
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};

type FontPage = AnyOf<(
    Center<Column<(Gray4Text, Gray4Text, Gray4Text)>>,
    Center<Column<(Gray4Text, Gray4Text, Gray4Text)>>,
    Center<Column<(Gray4Text, Gray4Text, Gray4Text)>>,
)>;

pub struct FontPageList;

impl WidgetList<FontPage> for FontPageList {
    fn len(&self) -> usize {
        3
    }

    fn get(&self, index: usize) -> Option<FontPage> {
        match index {
            0 => {
                // Page 1: Red fonts
                let text_regular = Gray4Text::new("Frostsnap", &NOTO_SANS_24_REGULAR, Rgb565::RED);
                let text_bold = Gray4Text::new("Frostsnap", &NOTO_SANS_24_BOLD, Rgb565::RED);
                let text_mono = Gray4Text::new("Frostsnap", &NOTO_SANS_MONO_21_BOLD, Rgb565::RED);
                
                let column = Column::new((text_regular, text_bold, text_mono))
                    .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
                Some(AnyOf::new(Center::new(column)))
            }
            1 => {
                // Page 2: Green fonts
                let text_regular = Gray4Text::new("Frostsnap", &NOTO_SANS_24_REGULAR, Rgb565::GREEN);
                let text_bold = Gray4Text::new("Frostsnap", &NOTO_SANS_24_BOLD, Rgb565::GREEN);
                let text_mono = Gray4Text::new("Frostsnap", &NOTO_SANS_MONO_21_BOLD, Rgb565::GREEN);
                
                let column = Column::new((text_regular, text_bold, text_mono))
                    .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
                Some(AnyOf::new(Center::new(column)))
            }
            2 => {
                // Page 3: Blue fonts
                let text_regular = Gray4Text::new("Frostsnap", &NOTO_SANS_24_REGULAR, Rgb565::BLUE);
                let text_bold = Gray4Text::new("Frostsnap", &NOTO_SANS_24_BOLD, Rgb565::BLUE);
                let text_mono = Gray4Text::new("Frostsnap", &NOTO_SANS_MONO_21_BOLD, Rgb565::BLUE);
                
                let column = Column::new((text_regular, text_bold, text_mono))
                    .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
                Some(AnyOf::new(Center::new(column)))
            }
            _ => None,
        }
    }
}

/// A demo widget showing different Gray4 font styles with PageSlider
#[derive(frostsnap_macros::Widget)]
pub struct FontDemo {
    #[widget_delegate]
    page_slider: PageSlider<FontPageList, FontPage>,
}

impl FontDemo {
    pub fn new() -> Self {
        let page_list = FontPageList;
        let page_slider = PageSlider::new(page_list, 100)
            .with_swipe_up_chevron();

        Self { page_slider }
    }
}

impl Default for FontDemo {
    fn default() -> Self {
        Self::new()
    }
}