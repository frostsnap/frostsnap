use super::{Column, Container, HoldToConfirm, Padding};
use crate::{
    gray4_style::Gray4TextStyle, palette::PALETTE, CrossAxisAlignment, MainAxisAlignment, Text,
    HOLD_TO_CONFIRM_TIME_MS,
};
use alloc::{format, string::ToString};
use embedded_graphics::geometry::Size;
use frostsnap_fonts::{NOTO_SANS_17_REGULAR, NOTO_SANS_18_MEDIUM, NOTO_SANS_MONO_28_BOLD};

const FONT_CONFIRM_TITLE: &frostsnap_fonts::Gray4Font = &NOTO_SANS_18_MEDIUM;
const FONT_CONFIRM_TEXT: &frostsnap_fonts::Gray4Font = &NOTO_SANS_17_REGULAR;
const FONT_SECURITY_CODE: &frostsnap_fonts::Gray4Font = &NOTO_SANS_MONO_28_BOLD;

type CodeColumn = Column<(Text<Gray4TextStyle>, Text<Gray4TextStyle>)>;
type CodeContainer = Container<Padding<CodeColumn>>;
type SubtitleColumn = Column<(Text<Gray4TextStyle>, Text<Gray4TextStyle>)>;
type PromptColumn = Column<(SubtitleColumn, CodeContainer, Text<Gray4TextStyle>)>;

/// Widget for checking and confirming key generation
#[derive(frostsnap_macros::Widget)]
pub struct KeygenCheck {
    #[widget_delegate]
    hold_to_confirm: HoldToConfirm<PromptColumn>,
}

impl KeygenCheck {
    pub fn new(t_of_n: (u16, u16), security_check_code: [u8; 4]) -> Self {
        let t_of_n_text = format!("{} of {}", t_of_n.0, t_of_n.1);
        let hex_code = format!(
            "{:02x}{:02x} {:02x}{:02x}",
            security_check_code[0],
            security_check_code[1],
            security_check_code[2],
            security_check_code[3]
        );

        let t_of_n_widget = Text::new(
            t_of_n_text,
            Gray4TextStyle::new(FONT_CONFIRM_TITLE, PALETTE.primary),
        );

        let code_widget = Text::new(
            hex_code,
            Gray4TextStyle::new(FONT_SECURITY_CODE, PALETTE.primary),
        );

        let mut code_column = Column::new((t_of_n_widget, code_widget))
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        code_column.set_gap(0, 3);

        // ⚖️ extra top padding to balance font descender
        let padded_code_column = Padding::only(code_column)
            .left(10)
            .right(10)
            .top(7)
            .bottom(4)
            .build();
        let code_container = Container::new(padded_code_column)
            .with_border(PALETTE.outline, 2)
            .with_corner_radius(Size::new(8, 8));

        let subtitle_column = Column::new((
            Text::new(
                "Check this code matches".to_string(),
                Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
            ),
            Text::new(
                "on every device".to_string(),
                Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
            ),
        ))
        .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let title_text = Text::new(
            "Hold to Confirm".to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TITLE, PALETTE.on_background),
        );

        let prompt_column = Column::new((subtitle_column, code_container, title_text))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let hold_to_confirm = HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_MS, prompt_column);

        Self { hold_to_confirm }
    }

    pub fn is_confirmed(&self) -> bool {
        self.hold_to_confirm.is_completed()
    }
}
