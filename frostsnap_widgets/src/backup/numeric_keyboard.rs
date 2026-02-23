use crate::DefaultTextStyle;
use crate::{
    icons::IconWidget, palette::PALETTE, prelude::*, touch_listener::TouchListener, Key, FONT_MED,
};
use alloc::string::String;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_iconoir::prelude::*;
use frostsnap_macros::Widget;

// Type alias to simplify the complex type
type StyledText = Text;

/// A button widget that displays a numeric key
#[derive(Widget)]
pub struct NumericButton {
    key: char,
    enabled: bool,
    #[widget_delegate]
    inner: Container<Align<Padding<StyledText>>>,
}

impl NumericButton {
    fn new(key: char, enabled: bool) -> TouchListener<Self> {
        // Create text for the button
        let text_color = if enabled {
            PALETTE.primary
        } else {
            PALETTE.text_disabled
        };

        let text = Text::new(
            String::from(key),
            DefaultTextStyle::new(FONT_MED, text_color),
        );

        let text = Padding::only(text).top(6).build();

        // Center align the text
        let aligned_text = Align::new(text);

        // Wrap in a Container with button styling — always use normal surface color
        let container = Container::new(aligned_text)
            .with_expanded()
            .with_fill(PALETTE.surface)
            .with_corner_radius(Size::new(8, 8))
            .with_aa_background(PALETTE.background);

        let button = Self {
            key,
            enabled,
            inner: container,
        };

        // Return a TouchListener that returns the key when pressed
        TouchListener::new(button, move |_, _, is_release, child| {
            if !child.enabled || is_release {
                None
            } else {
                Some(Key::Keyboard(child.key))
            }
        })
    }

    fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;

            // Only grey out the text, keep the button surface color
            let text_color = if enabled {
                PALETTE.primary
            } else {
                PALETTE.text_disabled
            };

            self.inner
                .child
                .child
                .child
                .set_character_style(DefaultTextStyle::new(FONT_MED, text_color));
        }
    }
}

/// A button widget that displays a checkmark
#[derive(Widget)]
pub struct CheckmarkButton {
    enabled: bool,
    #[widget_delegate]
    inner: Container<
        Align<
            IconWidget<
                embedded_iconoir::Icon<Rgb565, embedded_iconoir::icons::size24px::actions::Check>,
            >,
        >,
    >,
}

impl CheckmarkButton {
    fn new(enabled: bool) -> TouchListener<Self> {
        // Use smaller size24px icon and set color based on enabled state
        let icon_color = if enabled {
            PALETTE.tertiary
        } else {
            PALETTE.text_disabled
        };

        let icon = IconWidget::new(embedded_iconoir::icons::size24px::actions::Check::new(
            icon_color,
        ));

        // Center align the icon
        let aligned_icon = Align::new(icon);

        // Wrap in a Container with button styling
        let container_fill = if enabled {
            PALETTE.primary_container
        } else {
            PALETTE.surface
        };

        let container = Container::new(aligned_icon)
            .with_expanded()
            .with_fill(container_fill)
            .with_corner_radius(Size::new(8, 8))
            .with_aa_background(PALETTE.background);

        let button = Self {
            enabled,
            inner: container,
        };

        // Return a TouchListener that returns the confirm key when pressed
        TouchListener::new(button, move |_, _, is_release, child| {
            if !child.enabled || is_release {
                None
            } else {
                Some(Key::Keyboard('✓'))
            }
        })
    }

    fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;

            let icon_color = if enabled {
                PALETTE.tertiary
            } else {
                PALETTE.text_disabled
            };

            let container_fill = if enabled {
                PALETTE.primary_container
            } else {
                PALETTE.surface
            };

            self.inner.child.child.set_color(icon_color);
            self.inner.set_fill(container_fill);
        }
    }
}

type NumericRow = Row<(
    TouchListener<NumericButton>,
    TouchListener<NumericButton>,
    TouchListener<NumericButton>,
)>;

type BottomRow = Row<(
    Container<()>,
    TouchListener<NumericButton>,
    TouchListener<CheckmarkButton>,
)>;

/// A widget that displays a numeric keyboard with digits 0-9 and a checkmark
#[derive(Widget)]
pub struct NumericKeyboard {
    #[widget_delegate]
    keyboard: Padding<Column<alloc::boxed::Box<(NumericRow, NumericRow, NumericRow, BottomRow)>>>,
}

impl NumericKeyboard {
    pub fn new() -> Self {
        // Create the keyboard layout:
        // 1 2 3
        // 4 5 6
        // 7 8 9
        // _ 0 ✓
        let gap = 4;

        let mut row1 = Row::new((
            NumericButton::new('1', true),
            NumericButton::new('2', true),
            NumericButton::new('3', true),
        ));
        row1.set_all_flex(1);
        row1.set_uniform_gap(gap);

        let mut row2 = Row::new((
            NumericButton::new('4', true),
            NumericButton::new('5', true),
            NumericButton::new('6', true),
        ));
        row2.set_all_flex(1);
        row2.set_uniform_gap(gap);

        let mut row3 = Row::new((
            NumericButton::new('7', true),
            NumericButton::new('8', true),
            NumericButton::new('9', true),
        ));
        row3.set_all_flex(1);
        row3.set_uniform_gap(gap);

        // Bottom row with empty space, 0, and checkmark
        // Start with 0 and checkmark disabled (no digits entered yet)
        let empty_button = Container::new(()).with_expanded(); // Placeholder button (always disabled)
        let mut row4 = Row::new((
            empty_button,
            NumericButton::new('0', false), // Initially disabled
            CheckmarkButton::new(false),    // Initially disabled
        ));
        row4.set_all_flex(1);
        row4.set_uniform_gap(gap);

        // Create the column with all rows (boxed to move to heap)
        let mut keyboard = Column::new(alloc::boxed::Box::new((row1, row2, row3, row4)));
        keyboard.set_all_flex(1);
        keyboard.set_uniform_gap(gap);

        let keyboard = Padding::all(gap, keyboard);

        Self { keyboard }
    }

    /// Helper method to enable/disable the 0 button and checkmark based on whether any digits have been entered
    pub fn set_bottom_buttons_enabled(&mut self, enabled: bool) {
        // Access the bottom row (4th row in the column)
        let column = &mut self.keyboard.child;
        let bottom_row = &mut column.children.3;

        // Update the 0 button (second item in bottom row)
        bottom_row.children.1.child.set_enabled(enabled);

        // Update the checkmark button (third item in bottom row)
        bottom_row.children.2.child.set_enabled(enabled);
    }
}

impl Default for NumericKeyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for NumericKeyboard {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NumericKeyboard").finish()
    }
}
