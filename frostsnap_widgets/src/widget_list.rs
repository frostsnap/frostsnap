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

    /// Returns true if navigation to the next page is allowed from the current page
    fn can_go_next(&self, from_index: usize, _current_widget: &T) -> bool {
        from_index + 1 < self.len()
    }

    /// Returns true if navigation to the previous page is allowed from the current page
    fn can_go_prev(&self, from_index: usize, _current_widget: &T) -> bool {
        from_index > 0
    }
}

// Implementation for Vec<T> where T is Clone
impl<T> WidgetList<T> for alloc::vec::Vec<T> 
where
    T: Clone,
{
    fn len(&self) -> usize {
        self.len()
    }

    fn get(&self, index: usize) -> Option<T> {
        <[T]>::get(self, index).cloned()
    }
}

// Factory for creating pages/widgets on demand
pub struct PageFactory<F, T> {
    len: usize,
    factory: F,
    _phantom: core::marker::PhantomData<T>,
}

impl<F, T> PageFactory<F, T>
where
    F: Fn(usize) -> Option<T>,
{
    pub fn new(len: usize, factory: F) -> Self {
        Self {
            len,
            factory,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<F, T> WidgetList<T> for PageFactory<F, T>
where
    F: Fn(usize) -> Option<T>,
{
    fn len(&self) -> usize {
        self.len
    }

    fn get(&self, index: usize) -> Option<T> {
        if index < self.len {
            (self.factory)(index)
        } else {
            None
        }
    }
}