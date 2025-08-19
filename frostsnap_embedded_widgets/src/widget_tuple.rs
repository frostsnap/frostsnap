/// Trait for types that have an associated array type for storing auxiliary data
pub trait AssociatedArray {
    /// Generic associated type for auxiliary arrays (for storing rectangles, spacing, etc)
    type Array<T>: AsRef<[T]> + AsMut<[T]>;

    /// Create an array filled with a specific value, sized according to self
    fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T>;
    
    /// Get a child widget as a dyn DynWidget reference by index
    fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn crate::DynWidget>;
    
    /// Get the number of children
    fn len(&self) -> usize;
}

/// Trait to get tuple length at compile time and support tuple building
pub trait WidgetTuple: AssociatedArray {
    const TUPLE_LEN: usize;

    /// Generic associated type for adding a widget to the tuple
    type Add<W: crate::DynWidget>: AssociatedArray;

    /// Add a widget to this tuple
    fn add<W: crate::DynWidget>(self, widget: W) -> Self::Add<W>;
}

// The empty tuple needs special handling since it has no widgets
// We implement DynWidget for it to make everything work
impl crate::DynWidget for () {
    fn set_constraints(&mut self, _max_size: embedded_graphics::geometry::Size) {}
    fn sizing(&self) -> crate::Sizing { crate::Sizing { width: 0, height: 0 } }
}

// Implementation of AssociatedArray for empty tuple
impl AssociatedArray for () {
    type Array<T> = [T; 0];

    fn create_array_with<T: Copy>(&self, _value: T) -> Self::Array<T> {
        []
    }
    
    fn get_dyn_child(&mut self, _index: usize) -> Option<&mut dyn crate::DynWidget> {
        None
    }
    
    fn len(&self) -> usize {
        0
    }
}

// Special implementation for empty tuple
impl WidgetTuple for () {
    const TUPLE_LEN: usize = 0;
    type Add<W: crate::DynWidget> = (W,);

    fn add<W: crate::DynWidget>(self, widget: W) -> Self::Add<W> {
        (widget,)
    }
}

// Macro to implement both AssociatedArray and WidgetTuple for tuples
macro_rules! impl_widget_tuple {
    ($len:literal, $($t:ident),+) => {
        impl<$($t: crate::DynWidget),+> AssociatedArray for ($($t,)+) {
            type Array<T> = [T; $len];

            fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T> {
                [value; $len]
            }
            
            fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn crate::DynWidget> {
                #[allow(non_snake_case)]
                let ($(ref mut $t,)+) = self;
                let mut i = 0;
                $(
                    if i == index {
                        return Some($t as &mut dyn crate::DynWidget);
                    }
                    i += 1;
                )+
                None
            }
            
            fn len(&self) -> usize {
                $len
            }
        }
        
        impl<$($t: crate::DynWidget),+> WidgetTuple for ($($t,)+) {
            const TUPLE_LEN: usize = $len;
            type Add<W: crate::DynWidget> = ($($t,)+ W);

            fn add<W: crate::DynWidget>(self, widget: W) -> Self::Add<W> {
                #[allow(non_snake_case)]
                let ($($t,)+) = self;
                ($($t,)+ widget)
            }
        }
    };
}

// Special macro for the last tuple (20) that can't add more widgets
macro_rules! impl_last_widget_tuple {
    ($len:literal, $($t:ident),+) => {
        impl<$($t: crate::DynWidget),+> AssociatedArray for ($($t,)+) {
            type Array<T> = [T; $len];

            fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T> {
                [value; $len]
            }
            
            fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn crate::DynWidget> {
                #[allow(non_snake_case)]
                let ($(ref mut $t,)+) = self;
                let mut i = 0;
                $(
                    if i == index {
                        return Some($t as &mut dyn crate::DynWidget);
                    }
                    i += 1;
                )+
                None
            }
            
            fn len(&self) -> usize {
                $len
            }
        }
        
        impl<$($t: crate::DynWidget),+> WidgetTuple for ($($t,)+) {
            const TUPLE_LEN: usize = $len;
            // Can't add more widgets, so Add type is Self
            type Add<W: crate::DynWidget> = Self;

            fn add<W: crate::DynWidget>(self, _widget: W) -> Self::Add<W> {
                panic!("Cannot add more than 20 widgets to a tuple");
            }
        }
    };
}

// Generate implementations for tuples up to 20 elements
impl_widget_tuple!(1, T1);
impl_widget_tuple!(2, T1, T2);
impl_widget_tuple!(3, T1, T2, T3);
impl_widget_tuple!(4, T1, T2, T3, T4);
impl_widget_tuple!(5, T1, T2, T3, T4, T5);
impl_widget_tuple!(6, T1, T2, T3, T4, T5, T6);
impl_widget_tuple!(7, T1, T2, T3, T4, T5, T6, T7);
impl_widget_tuple!(8, T1, T2, T3, T4, T5, T6, T7, T8);
impl_widget_tuple!(9, T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_widget_tuple!(10, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_widget_tuple!(11, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_widget_tuple!(12, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
impl_widget_tuple!(13, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13);
impl_widget_tuple!(14, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14);
impl_widget_tuple!(15, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15);
impl_widget_tuple!(16, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16);
impl_widget_tuple!(17, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17);
impl_widget_tuple!(
    18, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18
);
impl_widget_tuple!(
    19, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19
);
impl_last_widget_tuple!(
    20, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20
);

/// Implementation of AssociatedArray for Vec<W> to enable dynamic collections
impl<W: crate::DynWidget> AssociatedArray for alloc::vec::Vec<W> {
    type Array<T> = alloc::vec::Vec<T>;

    fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T> {
        alloc::vec![value; self.len()]
    }

    fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn crate::DynWidget> {
        self.get_mut(index).map(|w| w as &mut dyn crate::DynWidget)
    }

    fn len(&self) -> usize {
        self.len()
    }
}

/// Implementation of AssociatedArray for fixed-size arrays
impl<W: crate::DynWidget, const N: usize> AssociatedArray for [W; N] {
    type Array<T> = [T; N];

    fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T> {
        [value; N]
    }

    fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn crate::DynWidget> {
        self.get_mut(index).map(|w| w as &mut dyn crate::DynWidget)
    }

    fn len(&self) -> usize {
        N
    }
}
