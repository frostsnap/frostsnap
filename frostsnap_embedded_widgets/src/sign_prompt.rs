use crate::{
    page_by_page::PageByPage, 
    Widget, DynWidget, Instant,
    column::{Column, MainAxisAlignment, CrossAxisAlignment},
    text::Text,
    bitcoin_amount_display::BitcoinAmountDisplay,
    sized_box::SizedBox,
    color_map::ColorMap,
};
use embedded_graphics::draw_target::DrawTargetExt;
use alloc::{format, string::{String, ToString}, vec::Vec, boxed::Box};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Gray4,
};
use u8g2_fonts::U8g2TextStyle;
use frostsnap_core::bitcoin_transaction::PromptSignBitcoinTx;

/// Macro to try downcasting to each widget type and call a method
macro_rules! try_downcast {
    ($widget:expr, $method:ident($($args:expr),*)) => {{
        use core::any::Any;
        let any_widget = $widget.as_mut() as &mut dyn Any;
        if let Some(page) = any_widget.downcast_mut::<AmountPage>() {
            page.$method($($args),*)
        } else if let Some(page) = any_widget.downcast_mut::<AddressPage>() {
            page.$method($($args),*)
        } else if let Some(page) = any_widget.downcast_mut::<FeePage>() {
            page.$method($($args),*)
        } else if let Some(page) = any_widget.downcast_mut::<WarningPage>() {
            page.$method($($args),*)
        } else {
            unreachable!("Widget type not recognized")
        }
    }};
}

/// A widget that displays transaction details for signing
pub struct SignPromptDisplay {
    prompt: PromptSignBitcoinTx,
    current_page: usize,
    size: Size,
    
    // Current widget stored dynamically
    current_widget: Box<dyn crate::AnyDynWidget>,
}

/// Page widget for displaying amount to send
struct AmountPage {
    column: Column<(Text<U8g2TextStyle<Gray4>>, SizedBox<Gray4>, ColorMap<BitcoinAmountDisplay, Gray4>, SizedBox<Gray4>, Text<U8g2TextStyle<Gray4>>), Gray4>,
}

impl AmountPage {
    fn new(_size: Size, index: usize, amount_sats: u64) -> Self {
        let title = Text::new(
            format!("Send Amount #{}", index + 1),
            U8g2TextStyle::new(crate::FONT_MED, Gray4::new(8)) // Medium gray for secondary text
        );
        
        let spacer = SizedBox::<Gray4>::new(Size::new(1, 15)); // 15px height spacing
        
        let amount_display = BitcoinAmountDisplay::new(amount_sats).color_map(|c| match c {
            embedded_graphics::pixelcolor::BinaryColor::Off => Gray4::new(6), // Disabled emphasis (~38%) for non-significant
            embedded_graphics::pixelcolor::BinaryColor::On => Gray4::new(11), // Primary color for significant digits
        });
        
        let btc_spacer = SizedBox::<Gray4>::new(Size::new(1, 10)); // 10px spacing before BTC
        
        let btc_text = Text::new(
            "BTC".to_string(),
            U8g2TextStyle::new(crate::FONT_MED, Gray4::new(8)) // Larger font, medium gray
        );
        
        let column = Column::new((title, spacer, amount_display, btc_spacer, btc_text))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        
        Self {
            column,
        }
    }
}

// Remove duplicate - this is now implemented below

impl crate::DynWidget for AmountPage {
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

impl Widget for AmountPage {
    type Color = Gray4;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(&mut self, target: &mut D, current_time: Instant) -> Result<(), D::Error> {
        self.column.draw(target, current_time)
    }
    
}

/// Page widget for displaying recipient address
struct AddressPage {
    title: Text<U8g2TextStyle<Gray4>>,
    spacer: SizedBox<Gray4>,
    address_display: Box<dyn crate::AnyDynWidget>,
}

impl AddressPage {
    fn new(index: usize, address: &str, _size: Size) -> Self {
        let title = Text::new(
            format!("Address #{}", index + 1),
            U8g2TextStyle::new(crate::FONT_MED, Gray4::new(8))
        );
        
        let spacer = SizedBox::<Gray4>::new(Size::new(1, 20));
        
        // Determine address type and create appropriate display widget
        let address_display: Box<dyn crate::AnyDynWidget> = if address.starts_with("bc1p") {
            // P2TR address (Taproot)
            Box::new(crate::p2tr_address_display::P2trAddressDisplay::new(address))
        } else {
            // For now, fall back to simple text display for other address types
            // In the future, we can add P2wpkhAddressDisplay, P2pkhAddressDisplay, etc.
            let chunks: Vec<String> = address.chars()
                .collect::<Vec<_>>()
                .chunks(4)
                .map(|chunk| chunk.iter().collect::<String>())
                .collect();
            
            let mut formatted_lines = Vec::new();
            for row_chunks in chunks.chunks(3) {
                let line = row_chunks.join("  ");
                formatted_lines.push(line);
            }
            
            let address_text = Text::new(
                formatted_lines.join("\n"),
                U8g2TextStyle::new(crate::FONT_LARGE, Gray4::new(14))
            );
            
            Box::new(address_text)
        };
        
        Self {
            title,
            spacer,
            address_display,
        }
    }
    
}

impl DynWidget for AddressPage {
    fn handle_touch(&mut self, _point: Point, _current_time: Instant, _is_release: bool) -> Option<crate::KeyTouch> {
        // For now, address display is not interactive
        None
    }
    
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // No vertical drag handling needed
    }
    
    fn size_hint(&self) -> Option<Size> {
        // Return None to use full available space
        None
    }
    
    fn force_full_redraw(&mut self) {
        self.title.force_full_redraw();
        self.spacer.force_full_redraw();
        self.address_display.force_full_redraw();
    }
}

impl Widget for AddressPage {
    type Color = Gray4;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(&mut self, target: &mut D, current_time: Instant) -> Result<(), D::Error> {
        use core::any::Any;
        
        // Create a column-like layout manually
        let bounds = target.bounding_box();
        
        // Draw title
        let title_height = self.title.size_hint().map(|s| s.height).unwrap_or(20);
        let title_area = embedded_graphics::primitives::Rectangle::new(
            bounds.top_left,
            Size::new(bounds.size.width, title_height)
        );
        self.title.draw(&mut target.cropped(&title_area), current_time)?;
        
        // Draw spacer (just advance position)
        let spacer_height = self.spacer.size_hint().map(|s| s.height).unwrap_or(20);
        let content_start_y = bounds.top_left.y + title_height as i32 + spacer_height as i32;
        
        // Draw address display
        let content_area = embedded_graphics::primitives::Rectangle::new(
            Point::new(bounds.top_left.x, content_start_y),
            Size::new(bounds.size.width, bounds.size.height.saturating_sub(title_height + spacer_height))
        );
        
        // Downcast and draw the appropriate address display widget
        let any_widget = self.address_display.as_mut() as &mut dyn Any;
        if let Some(p2tr_display) = any_widget.downcast_mut::<crate::p2tr_address_display::P2trAddressDisplay>() {
            p2tr_display.draw(&mut target.cropped(&content_area), current_time)?;
        } else if let Some(text_display) = any_widget.downcast_mut::<Text<U8g2TextStyle<Gray4>>>() {
            text_display.draw(&mut target.cropped(&content_area), current_time)?;
        }
        
        Ok(())
    }
}

/// Page widget for displaying network fee
struct FeePage {
    column: Column<(
        Text<U8g2TextStyle<Gray4>>,
        ColorMap<BitcoinAmountDisplay, Gray4>,
        Text<U8g2TextStyle<Gray4>>,
    ), Gray4>,
}

impl FeePage {
    fn new(_size: Size, fee_sats: u64) -> Self {
        let title = Text::new(
            "Network Fee".to_string(),
            U8g2TextStyle::new(crate::FONT_MED, Gray4::new(7))
        );
        
        let fee_amount = BitcoinAmountDisplay::new(fee_sats).color_map(|c| match c {
            embedded_graphics::pixelcolor::BinaryColor::Off => Gray4::new(6), // Disabled emphasis (~38%) for non-significant
            embedded_graphics::pixelcolor::BinaryColor::On => Gray4::new(11), // Primary color for significant digits
        });
        
        let fee_sats_text = Text::new(
            format!("{} sats", fee_sats),
            U8g2TextStyle::new(crate::FONT_SMALL, Gray4::new(7))
        );
        
        let column = Column::new((
            title,
            fee_amount,
            fee_sats_text,
        )).with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
        
        Self {
            column,
        }
    }
}

impl DynWidget for FeePage {
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

impl Widget for FeePage {
    type Color = Gray4;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(&mut self, target: &mut D, current_time: Instant) -> Result<(), D::Error> {
        self.column.draw(target, current_time)
    }
}

/// Page widget for high fee warning
struct WarningPage {
    column: Column<(
        SizedBox<Gray4>,
        Text<U8g2TextStyle<Gray4>>,
        Text<U8g2TextStyle<Gray4>>,
        Text<U8g2TextStyle<Gray4>>,
    ), Gray4>,
}

impl WarningPage {
    fn new(_size: Size, fee_sats: u64, _total_sent: u64) -> Self {
        let spacer = SizedBox::<Gray4>::new(Size::new(1, 40));
        
        // TODO: Replace with warning icon bitmap
        let warning_icon = Text::new(
            "!".to_string(),
            U8g2TextStyle::new(crate::FONT_LARGE, Gray4::new(11))
        );
        
        let caution_text = Text::new(
            "Caution".to_string(),
            U8g2TextStyle::new(crate::FONT_MED, Gray4::new(11))
        );
        
        let warning_msg = if fee_sats > 100_000 {
            "Fee exceeds 0.001 BTC"
        } else {
            "Fee exceeds 5% of amount"
        };
        
        let warning_text = Text::new(
            warning_msg.to_string(),
            U8g2TextStyle::new(crate::FONT_SMALL, Gray4::new(14))
        );
        
        let column = Column::new((
            spacer,
            warning_icon,
            caution_text,
            warning_text,
        )).with_main_axis_alignment(MainAxisAlignment::Center);
        
        Self {
            column,
        }
    }
}

impl DynWidget for WarningPage {
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

impl Widget for WarningPage {
    type Color = Gray4;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(&mut self, target: &mut D, current_time: Instant) -> Result<(), D::Error> {
        self.column.draw(target, current_time)
    }
}

impl SignPromptDisplay {
    pub fn new(size: Size, prompt: PromptSignBitcoinTx) -> Self {
        // Create first widget
        let current_widget = Self::create_widget_for_page(0, &prompt, size);

        Self {
            prompt,
            current_page: 0,
            size,
            current_widget,
        }
    }
    
    /// Determine what type of page this is and create the appropriate widget
    fn create_widget_for_page(page_num: usize, prompt: &PromptSignBitcoinTx, size: Size) -> Box<dyn crate::AnyDynWidget> {
        let num_recipients = prompt.foreign_recipients.len();
        let recipient_pages = num_recipients * 2;
        
        if page_num < recipient_pages {
            // It's either an amount or address page for a recipient
            let recipient_idx = page_num / 2;
            let is_amount = page_num % 2 == 0;
            
            if is_amount {
                // Amount page
                let (_, amount) = &prompt.foreign_recipients[recipient_idx];
                Box::new(AmountPage::new(size, recipient_idx, amount.to_sat()))
            } else {
                // Address page
                let (address, _) = &prompt.foreign_recipients[recipient_idx];
                Box::new(AddressPage::new(recipient_idx, &address.to_string(), size))
            }
        } else if page_num == recipient_pages {
            // Fee page
            Box::new(FeePage::new(size, prompt.fee.to_sat()))
        } else {
            // Warning page
            let total_sent: u64 = prompt.foreign_recipients
                .iter()
                .map(|(_, amount)| amount.to_sat())
                .sum();
            Box::new(WarningPage::new(size, prompt.fee.to_sat(), total_sent))
        }
    }
    
    /// Check if the transaction has high fees that warrant a warning
    fn has_high_fee(prompt: &PromptSignBitcoinTx) -> bool {
        let fee_sats = prompt.fee.to_sat();
        
        // High fee if > 0.001 BTC (100,000 sats)
        if fee_sats > 100_000 {
            return true;
        }
        
        // High fee if > 5% of total amount being sent
        let total_sent: u64 = prompt.foreign_recipients
            .iter()
            .map(|(_, amount)| amount.to_sat())
            .sum();
        if total_sent > 0 && fee_sats > total_sent / 20 {
            return true;
        }
        
        false
    }
    
}

impl DynWidget for SignPromptDisplay {
    fn handle_touch(&mut self, _point: Point, _current_time: Instant, _is_release: bool) -> Option<crate::KeyTouch> {
        None
    }
    
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // Not used
    }
    
    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }
    
    fn force_full_redraw(&mut self) {
        self.current_widget.force_full_redraw();
    }
}

impl Widget for SignPromptDisplay {
    type Color = Gray4;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Clear background
        target.clear(Gray4::new(0))?;
        
        // Draw the current widget - still need to downcast for this
        try_downcast!(self.current_widget, draw(target, current_time))
    }
}

impl PageByPage for SignPromptDisplay {
    fn current_page(&self) -> usize {
        self.current_page
    }
    
    fn total_pages(&self) -> usize {
        let num_recipients = self.prompt.foreign_recipients.len();
        let has_warning = Self::has_high_fee(&self.prompt);
        
        // Each recipient has 2 pages (amount, address), plus fee page, plus optional warning
        num_recipients * 2 + 1 + if has_warning { 1 } else { 0 }
    }
    
    fn has_next_page(&self) -> bool {
        self.current_page < self.total_pages() - 1
    }
    
    fn has_prev_page(&self) -> bool {
        self.current_page > 0
    }
    
    fn next_page(&mut self) {
        if self.has_next_page() {
            self.current_page += 1;
            // Create new widget for the new page
            self.current_widget = Self::create_widget_for_page(
                self.current_page,
                &self.prompt,
                self.size
            );
        }
    }
    
    fn prev_page(&mut self) {
        if self.has_prev_page() {
            self.current_page -= 1;
            // Create new widget for the new page
            self.current_widget = Self::create_widget_for_page(
                self.current_page,
                &self.prompt,
                self.size
            );
        }
    }
}

/// High-level widget that manages the complete sign prompt flow
/// Handles paginator, scroll bar, hold to confirm, and color mapping
pub struct SignPrompt {
    widget: crate::PaginatorWithScrollBar<
        crate::color_map::ColorMap<
            crate::animation::VerticalPaginator<SignPromptDisplay, 240, 280, { embedded_graphics::framebuffer::buffer_size::<Gray4>(240, 280) }>,
            embedded_graphics::pixelcolor::Rgb565
        >,
        crate::HoldToConfirm<crate::color_map::ColorMap<Text<U8g2TextStyle<embedded_graphics::pixelcolor::BinaryColor>>, embedded_graphics::pixelcolor::Rgb565>>
    >,
}

impl SignPrompt {
    pub fn new(screen_size: Size, prompt: PromptSignBitcoinTx) -> Self {
        use crate::{palette::PALETTE, animation::VerticalPaginator, HoldToConfirm, PaginatorWithScrollBar};
        use embedded_graphics::{pixelcolor::BinaryColor, prelude::GrayColor};
        
        // Create the sign prompt display widget
        let sign_display = SignPromptDisplay::new(screen_size, prompt);
        
        // Wrap in vertical paginator
        const SCREEN_WIDTH: usize = 240;
        const SCREEN_HEIGHT: usize = 280;
        const BUFFER_SIZE: usize = embedded_graphics::framebuffer::buffer_size::<Gray4>(SCREEN_WIDTH, SCREEN_HEIGHT);
        let paginator = VerticalPaginator::<_, SCREEN_WIDTH, SCREEN_HEIGHT, BUFFER_SIZE>::new(sign_display);
        
        // Map Gray4 colors to Rgb565
        let paginator_mapped = paginator.color_map(|c| match c.luma() {
            0 => PALETTE.background,           // Black
            1..=3 => PALETTE.surface,          // Very dark grays
            4..=5 => PALETTE.surface_variant,  // Dark surface
            6 => PALETTE.text_disabled,        // Disabled emphasis (~38%)
            7..=9 => PALETTE.text_secondary,   // Medium emphasis (~60%)
            10..=12 => PALETTE.primary,        // Accent color
            13..=15 => PALETTE.on_surface,     // High emphasis (~87%)
            _ => PALETTE.background,
        });
        
        // Create hold to confirm widget
        let confirm_text = Text::new("Hold to Sign", U8g2TextStyle::new(crate::FONT_MED, BinaryColor::On));
        let confirm_widget = confirm_text.color_map(|c| match c {
            BinaryColor::On => PALETTE.on_surface,
            BinaryColor::Off => PALETTE.background,
        });
        let hold_confirm = HoldToConfirm::new(screen_size, 5000, confirm_widget);
        
        // Wrap in PaginatorWithScrollBar
        let widget = PaginatorWithScrollBar::new(paginator_mapped, hold_confirm, screen_size);
        
        Self { widget }
    }
}

impl crate::DynWidget for SignPrompt {
    fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) -> Option<crate::KeyTouch> {
        self.widget.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.widget.handle_vertical_drag(prev_y, new_y, is_release)
    }
    
    fn size_hint(&self) -> Option<Size> {
        self.widget.size_hint()
    }
    
    fn force_full_redraw(&mut self) {
        self.widget.force_full_redraw()
    }
}

impl Widget for SignPrompt {
    type Color = embedded_graphics::pixelcolor::Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        self.widget.draw(target, current_time)
    }
}
