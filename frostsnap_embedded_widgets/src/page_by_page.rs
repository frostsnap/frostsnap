use crate::Widget;

/// Trait for widgets that can navigate between pages
pub trait PageByPage: Widget {
    /// Check if there is a next page available
    fn has_next_page(&self) -> bool;
    
    /// Check if there is a previous page available
    fn has_prev_page(&self) -> bool;
    
    /// Move to the next page
    fn next_page(&mut self);
    
    /// Move to the previous page  
    fn prev_page(&mut self);
    
    /// Get the current page number (0-indexed)
    fn current_page(&self) -> usize;
    
    /// Get the total number of pages
    fn total_pages(&self) -> usize;
    
    /// Check if the widget is currently transitioning between pages
    fn is_transitioning(&self) -> bool {
        false
    }
}