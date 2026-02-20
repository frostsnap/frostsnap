use crate::{
    palette::PALETTE, prelude::*, DefaultTextStyle, Frac, Padding, ProgressIndicator, FONT_MED,
};
use alloc::boxed::Box;
use embedded_graphics::text::Alignment;

#[derive(frostsnap_macros::Widget)]
pub struct EraseProgress {
    #[widget_delegate]
    widget: Box<Column<(Text, Padding<ProgressIndicator>)>>,
}

impl EraseProgress {
    pub fn new(progress: Frac) -> Self {
        let title = Text::new(
            "Erasing...",
            DefaultTextStyle::new(FONT_MED, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);

        let mut progress_indicator = ProgressIndicator::new();
        progress_indicator.set_progress(progress);

        let padded_progress = Padding::symmetric(20, 0, progress_indicator);

        let widget = Column::new((title, padded_progress))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

        Self {
            widget: Box::new(widget),
        }
    }

    pub fn update_progress(&mut self, progress: Frac) {
        self.widget.children.1.child.set_progress(progress);
    }
}
