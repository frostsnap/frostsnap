use crate::{
    column::Column, row::Row, sized_box::SizedBox, text::Text, 
    CrossAxisAlignment, DynWidget, Instant, MainAxisAlignment, Widget
};
use alloc::string::ToString;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Gray4,
};
use u8g2_fonts::U8g2TextStyle;

/// A widget for displaying P2TR (Taproot) addresses in a specific format:
/// - 1 row with the first chunk (grayed out)
/// - 5 rows with 3 chunks each (4 chars per chunk)
/// - Total: 62 characters displayed as 16 chunks
pub struct P2trAddressDisplay {
    column: Column<(
        Row<(Text<U8g2TextStyle<Gray4>>,), Gray4>,
        SizedBox<Gray4>,
        Row<(Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>), Gray4>,
        Row<(Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>), Gray4>,
        Row<(Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>), Gray4>,
        Row<(Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>), Gray4>,
        Row<(Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>), Gray4>,
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
        let grayed_style = U8g2TextStyle::new(crate::FONT_LARGE, Gray4::new(8)); // Grayed out for type indicator
        let spacer_width = 8; // Space between chunks
        
        // Helper to create a spacer
        let make_spacer = || SizedBox::<Gray4>::new(Size::new(spacer_width, 1));
        
        // First chunk on its own row (grayed out)
        let type_indicator = Row::new((
            Text::new(chunks.next().unwrap(), grayed_style),
        )).with_main_axis_alignment(MainAxisAlignment::Center);
        
        // Vertical spacer between type indicator and address content
        let vertical_spacer = SizedBox::<Gray4>::new(Size::new(1, 10));
        
        // Row 0: chunks 1, 2, 3 (now that chunk 0 is the type indicator)
        let row0 = Row::new((
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
        ));
        
        // Row 1: chunks 4, 5, 6
        let row1 = Row::new((
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
        ));
        
        // Row 2: chunks 7, 8, 9
        let row2 = Row::new((
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
        ));
        
        // Row 3: chunks 10, 11, 12
        let row3 = Row::new((
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
        ));
        
        // Row 4: chunks 13, 14, 15 (last chunk is only 2 chars)
        let row4 = Row::new((
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
            make_spacer(),
            Text::new(chunks.next().unwrap(), text_style.clone()),
        ));
        
        // Create column with all rows
        let column = Column::new((type_indicator, vertical_spacer, row0, row1, row2, row3, row4)).with_cross_axis_alignment(CrossAxisAlignment::Center);

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
