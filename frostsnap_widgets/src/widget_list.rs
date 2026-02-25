/// A trait for providing a list of widgets by index
pub trait WidgetList {
    type Widget;

    /// Returns the number of widgets in the list
    fn len(&self) -> usize;

    /// Returns the widget at the given index, or None if out of bounds
    fn get(&self, index: usize) -> Option<Self::Widget>;

    /// Returns true if the list is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if navigation to the next page is allowed from the current page
    fn can_go_next(&self, from_index: usize, _current_widget: &Self::Widget) -> bool {
        from_index + 1 < self.len()
    }

    /// Returns true if navigation to the previous page is allowed from the current page
    fn can_go_prev(&self, from_index: usize, _current_widget: &Self::Widget) -> bool {
        from_index > 0
    }
}

impl<T: Clone> WidgetList for alloc::vec::Vec<T> {
    type Widget = T;

    fn len(&self) -> usize {
        self.len()
    }

    fn get(&self, index: usize) -> Option<T> {
        <[T]>::get(self, index).cloned()
    }
}

macro_rules! impl_widget_list_for_tuple {
    ($($idx:tt => $W:ident),+ ; $len:expr) => {
        impl<$($W: crate::AnyDynWidget + Clone + 'static),+> WidgetList for ($($W,)+) {
            type Widget = crate::any_of::AnyOf<($($W,)+)>;

            fn len(&self) -> usize {
                $len
            }

            fn get(&self, index: usize) -> Option<Self::Widget> {
                match index {
                    $($idx => Some(crate::any_of::AnyOf::new(self.$idx.clone())),)+
                    _ => None,
                }
            }
        }
    };
}

impl_widget_list_for_tuple!(0 => W0; 1);
impl_widget_list_for_tuple!(0 => W0, 1 => W1; 2);
impl_widget_list_for_tuple!(0 => W0, 1 => W1, 2 => W2; 3);
impl_widget_list_for_tuple!(0 => W0, 1 => W1, 2 => W2, 3 => W3; 4);
impl_widget_list_for_tuple!(0 => W0, 1 => W1, 2 => W2, 3 => W3, 4 => W4; 5);
impl_widget_list_for_tuple!(0 => W0, 1 => W1, 2 => W2, 3 => W3, 4 => W4, 5 => W5; 6);
impl_widget_list_for_tuple!(0 => W0, 1 => W1, 2 => W2, 3 => W3, 4 => W4, 5 => W5, 6 => W6; 7);
impl_widget_list_for_tuple!(0 => W0, 1 => W1, 2 => W2, 3 => W3, 4 => W4, 5 => W5, 6 => W6, 7 => W7; 8);
impl_widget_list_for_tuple!(0 => W0, 1 => W1, 2 => W2, 3 => W3, 4 => W4, 5 => W5, 6 => W6, 7 => W7, 8 => W8; 9);
impl_widget_list_for_tuple!(0 => W0, 1 => W1, 2 => W2, 3 => W3, 4 => W4, 5 => W5, 6 => W6, 7 => W7, 8 => W8, 9 => W9; 10);
