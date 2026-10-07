use crate::{
    address_display::AddressDisplay,
    any_of::AnyOf,
    gray4_style::Gray4TextStyle,
    page_slider::PageSlider,
    palette::PALETTE,
    prelude::*,
    sign_prompt::ConfirmationPage,
    widget_list::{WidgetList, WidgetListItem},
};
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};
use embedded_graphics::{
    geometry::Point,
    text::{renderer::TextRenderer, Alignment, Baseline},
};
use frostsnap_core::tweak::BitcoinBip32Path;
use frostsnap_fonts::{Gray4Font, NOTO_SANS_17_REGULAR, NOTO_SANS_18_LIGHT};

const FONT_PAGE_HEADER: &Gray4Font = &NOTO_SANS_18_LIGHT;
const FONT_MESSAGE: &Gray4Font = &NOTO_SANS_17_REGULAR;
const FONT_FOOTNOTE: &Gray4Font = &NOTO_SANS_17_REGULAR;
const MESSAGE_WIDTH_PX: u32 = 220;
const MESSAGE_LINES_PER_PAGE: usize = 7;

/// Splits a word wider than `max_width` across lines so none of it runs off screen.
fn wrap_lines(text: &str, style: &Gray4TextStyle, max_width: u32) -> Vec<String> {
    let width = |s: &str| {
        style
            .measure_string(s, Point::zero(), Baseline::Top)
            .bounding_box
            .size
            .width
    };
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if width(&candidate) <= max_width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(core::mem::take(&mut line));
            }
            for ch in word.chars() {
                line.push(ch);
                if width(&line) > max_width {
                    line.pop();
                    lines.push(core::mem::take(&mut line));
                    line.push(ch);
                }
            }
        }
        lines.push(line);
    }
    lines
}

/// One page of the message being signed.
#[derive(frostsnap_macros::Widget)]
pub struct MessagePage {
    #[widget_delegate]
    center: Center<Padding<Column<(Text<Gray4TextStyle>, Text<Gray4TextStyle>)>>>,
}

impl MessagePage {
    #[inline(never)]
    fn new(lines: &str, page: usize, page_count: usize) -> Self {
        let title = if page_count > 1 {
            format!("Sign Message (page {}/{})", page + 1, page_count)
        } else {
            "Sign Message".to_string()
        };
        let title = Text::new(
            title,
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );
        let body = Text::new(
            lines,
            Gray4TextStyle::new(FONT_MESSAGE, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);

        let mut column = Column::new((title, body))
            .with_main_axis_alignment(MainAxisAlignment::Start)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        column.set_gap(0, 16);
        let padded = Padding::only(column).bottom(40).build();

        Self {
            center: Center::new(padded),
        }
    }
}

/// Same layout as the transaction prompt's `AddressPage`, so the address is checked the same way.
#[derive(frostsnap_macros::Widget)]
pub struct SigningAddressPage {
    #[widget_delegate]
    center: Center<Padding<Column<(Text<Gray4TextStyle>, AddressDisplay, Text<Gray4TextStyle>)>>>,
}

impl SigningAddressPage {
    #[inline(never)]
    fn new(address: &bitcoin::Address, bip32_path: &BitcoinBip32Path, rand_seed: u32) -> Self {
        let title = Text::new(
            "With Address",
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );
        let address_display = AddressDisplay::new_with_seed(address.clone(), rand_seed);
        let footnote = Text::new(
            bip32_path.label(),
            Gray4TextStyle::new(FONT_FOOTNOTE, PALETTE.text_secondary),
        );

        let mut column = Column::new((title, address_display, footnote))
            .with_main_axis_alignment(MainAxisAlignment::Start);
        column.set_gap(0, 10);
        column.set_gap(1, 8);
        let padded = Padding::only(column).bottom(40).build();

        Self {
            center: Center::new(padded),
        }
    }
}

type Bip322Page = AnyOf<(MessagePage, SigningAddressPage, ConfirmationPage)>;

#[derive(Clone)]
pub struct Bip322PageList {
    /// The wrapped message, one entry per page.
    message_pages: Vec<String>,
    address: bitcoin::Address,
    bip32_path: BitcoinBip32Path,
    rand_seed: u32,
}

impl WidgetList for Bip322PageList {
    type Widget = Bip322Page;

    fn len(&self) -> usize {
        self.message_pages.len() + 2
    }

    fn get(&self, index: usize) -> Option<WidgetListItem<Bip322Page>> {
        let message_page_count = self.message_pages.len();
        let item = if index < message_page_count {
            WidgetListItem::new(Bip322Page::new(MessagePage::new(
                &self.message_pages[index],
                index,
                message_page_count,
            )))
        } else if index == message_page_count {
            WidgetListItem::new(Bip322Page::new(SigningAddressPage::new(
                &self.address,
                &self.bip32_path,
                self.rand_seed,
            )))
            .with_framebuffer_transitions(true)
        } else if index == message_page_count + 1 {
            WidgetListItem::new(Bip322Page::new(ConfirmationPage::new()))
        } else {
            return None;
        };
        Some(item)
    }

    fn can_go_prev(&self, from_index: usize, current_widget: &Bip322Page) -> bool {
        if from_index == 0 {
            return false;
        }
        if let Some(confirmation_page) = current_widget.downcast_ref::<ConfirmationPage>() {
            return !confirmation_page.is_confirmed();
        }
        true
    }
}

/// BIP-322 message signing prompt: the message, then the address it is signed
/// under, then hold to sign.
#[derive(frostsnap_macros::Widget)]
pub struct Bip322Confirm {
    #[widget_delegate]
    page_slider: Box<PageSlider<Bip322PageList>>,
}

impl Bip322Confirm {
    #[inline(never)]
    pub fn new(
        message: String,
        address: bitcoin::Address,
        bip32_path: BitcoinBip32Path,
        rand_seed: u32,
    ) -> Self {
        // The font skips characters it has no glyph for, which would hide them from the signer.
        let message: String = message
            .chars()
            .map(|c| match c {
                '\n' | ' ' => c,
                c if FONT_MESSAGE.get_glyph(c).is_some() => c,
                _ => '?',
            })
            .collect();
        let style = Gray4TextStyle::new(FONT_MESSAGE, PALETTE.on_background);
        let lines = wrap_lines(&message, &style, MESSAGE_WIDTH_PX);
        let message_pages = lines
            .chunks(MESSAGE_LINES_PER_PAGE)
            .map(|chunk| chunk.join("\n"))
            .collect();
        let page_list = Bip322PageList {
            message_pages,
            address,
            bip32_path,
            rand_seed,
        };
        let mut page_slider = Box::new(PageSlider::new(page_list));
        page_slider.set_on_page_ready(|page| {
            if let Some(confirmation_page) = page.downcast_mut::<ConfirmationPage>() {
                confirmation_page.hold_confirm.fade_in_button();
            }
        });
        page_slider.enable_swipe_up_chevron();

        Self { page_slider }
    }

    pub fn is_confirmed(&mut self) -> bool {
        let current_widget = self.page_slider.current_widget();
        current_widget
            .downcast_ref::<ConfirmationPage>()
            .is_some_and(|page| page.is_confirmed())
    }

    pub fn is_finished(&mut self) -> bool {
        let current_widget = self.page_slider.current_widget();
        current_widget
            .downcast_ref::<ConfirmationPage>()
            .is_some_and(|page| page.is_finished())
    }
}
