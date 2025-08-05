use crate::{
    DynWidget, Widget, Instant,
    row::Row,
    column::Column,
    text::Text,
    sized_box::SizedBox,
};
use alloc::string::ToString;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Gray4,
};
use u8g2_fonts::U8g2TextStyle;

/// A widget for displaying P2TR (Taproot) addresses in a specific format:
/// - 5 rows with 3 chunks each (4 chars per chunk)
/// - 1 row with 1 chunk (2 chars)
/// - Total: 62 characters displayed as 16 chunks
pub struct P2trAddressDisplay {
    column: Column<(
        Row<(Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>), Gray4>,
        Row<(Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>), Gray4>,
        Row<(Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>), Gray4>,
        Row<(Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>), Gray4>,
        Row<(Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>), Gray4>,
        Row<(Text<U8g2TextStyle<Gray4>>,), Gray4>,
    ), Gray4>,
}

impl P2trAddressDisplay {
    pub fn new(address: &str) -> Self {
        // P2TR addresses are always 62 characters (ASCII)
        // Split into chunks of 4 characters without allocations
        let mut chunks = (0..address.len()).step_by(4).map(move |start| {
            let end = (start + 4).min(address.len());
            address[start..end].to_string()
        });
        
        let text_style = U8g2TextStyle::new(crate::FONT_LARGE, Gray4::new(14));
        let spacer_width = 8; // Space between chunks
        
        // Helper to create a spacer
        let make_spacer = || SizedBox::<Gray4>::new(Size::new(spacer_width, 1));
        
        // P2TR addresses have exactly 16 chunks (15 full + 1 partial)
        // Row 0: chunks 0, 1, 2
        let row0 = Row::new((
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
        ));
        
        // Row 1: chunks 3, 4, 5
        let row1 = Row::new((
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
        ));
        
        // Row 2: chunks 6, 7, 8
        let row2 = Row::new((
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
        ));
        
        // Row 3: chunks 9, 10, 11
        let row3 = Row::new((
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
        ));
        
        // Row 4: chunks 12, 13, 14
        let row4 = Row::new((
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
        ));
        
        // Row 5: chunk 15 (only 2 chars)
        let row5 = Row::new((
            Text::new(chunks.next().unwrap(), text_style),
        ));
        
        // Create column with all rows
        let column = Column::new((row0, row1, row2, row3, row4, row5));

        Self { column }
    }
}

impl DynWidget for P2trAddressDisplay {
    fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) -> Option<crate::KeyTouch> {
        self.column.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.column.handle_vertical_drag(prev_y, new_y, is_release)
    }
    
    fn size_hint(&self) -> Option<Size> {
        self.column.size_hint()
    }
    
    fn force_full_redraw(&mut self) {
        self.column.force_full_redraw()
    }
}

impl Widget for P2trAddressDisplay {
    type Color = Gray4;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(&mut self, target: &mut D, current_time: Instant) -> Result<(), D::Error> {
        self.column.draw(target, current_time)
    }
}
