/// A trait for providing a list of widgets by index
pub trait WidgetList<T> {
    /// Returns the number of widgets in the list
    fn len(&self) -> usize;
    
    /// Returns the widget at the given index, or None if out of bounds
    fn get(&self, index: usize) -> Option<T>;
    
    /// Returns true if the list is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}