//! Implementations of AssociatedArray and PushWidget for various collection types

use super::{AssociatedArray, PushWidget};
use alloc::{boxed::Box, vec, vec::Vec};

// The empty tuple needs special handling since it has no widgets
// We implement DynWidget for it to make everything work
impl crate::DynWidget for () {
    fn set_constraints(&mut self, _max_size: embedded_graphics::geometry::Size) {}
    fn sizing(&self) -> crate::Sizing {
        crate::Sizing {
            width: 0,
            height: 0,
        }
    }
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

// PushWidget implementation for empty tuple - can push any widget type
impl<W: crate::DynWidget> PushWidget<W> for () {
    type Output = (W,);

    fn push_widget(self, widget: W) -> Self::Output {
        (widget,)
    }
}

// Macro to implement both AssociatedArray and PushWidget for tuples
macro_rules! impl_widget_tuple {
    ($len:literal, $($t:ident),+) => {
        impl<$($t: crate::DynWidget),+> AssociatedArray for ($($t,)+) {
            type Array<T> = [T; $len];

            fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T> {
                [value; $len]
            }

            fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn crate::DynWidget> {
                #[allow(non_snake_case, unused_assignments, unused_mut)]
                let ($(ref mut $t,)+) = self;
                #[allow(unused_mut, unused_assignments)]
                let mut i = 0;
                $(
                    if i == index {
                        return Some($t as &mut dyn crate::DynWidget);
                    }
                    #[allow(unused_assignments)]
                    { i += 1; }
                )+
                None
            }

            fn len(&self) -> usize {
                $len
            }
        }

        // PushWidget implementation for tuples - can push any widget type
        impl<$($t: crate::DynWidget,)+ W: crate::DynWidget> PushWidget<W> for ($($t,)+) {
            type Output = ($($t,)+ W);

            fn push_widget(self, widget: W) -> Self::Output {
                #[allow(non_snake_case)]
                let ($($t,)+) = self;
                ($($t,)+ widget)
            }
        }

        // Box implementation for heap allocation
        impl<$($t: crate::DynWidget),+> AssociatedArray for Box<($($t,)+)> {
            type Array<T> = Box<[T]>;

            fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T> {
                vec![value; $len].into_boxed_slice()
            }

            fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn crate::DynWidget> {
                #[allow(non_snake_case, unused_assignments, unused_mut)]
                let ($(ref mut $t,)+) = &mut **self;
                #[allow(unused_mut, unused_assignments)]
                let mut i = 0;
                $(
                    if i == index {
                        return Some($t as &mut dyn crate::DynWidget);
                    }
                    #[allow(unused_assignments)]
                    { i += 1; }
                )+
                None
            }

            fn len(&self) -> usize {
                $len
            }
        }

        // PushWidget implementation for Box<tuple> - can push any widget type
        impl<$($t: crate::DynWidget,)+ W: crate::DynWidget> PushWidget<W> for Box<($($t,)+)> {
            type Output = Box<($($t,)+ W)>;

            fn push_widget(self, widget: W) -> Self::Output {
                #[allow(non_snake_case)]
                let ($($t,)+) = *self;
                Box::new(($($t,)+ widget))
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
                #[allow(non_snake_case, unused_assignments, unused_mut)]
                let ($(ref mut $t,)+) = self;
                #[allow(unused_mut, unused_assignments)]
                let mut i = 0;
                $(
                    if i == index {
                        return Some($t as &mut dyn crate::DynWidget);
                    }
                    #[allow(unused_assignments)]
                    { i += 1; }
                )+
                None
            }

            fn len(&self) -> usize {
                $len
            }
        }

        // Note: No PushWidget implementation for the last tuple (can't add more widgets)

        // Box implementation for heap allocation
        impl<$($t: crate::DynWidget),+> AssociatedArray for Box<($($t,)+)> {
            type Array<T> = Box<[T]>;

            fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T> {
                vec![value; $len].into_boxed_slice()
            }

            fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn crate::DynWidget> {
                #[allow(non_snake_case, unused_assignments, unused_mut)]
                let ($(ref mut $t,)+) = &mut **self;
                #[allow(unused_mut, unused_assignments)]
                let mut i = 0;
                $(
                    if i == index {
                        return Some($t as &mut dyn crate::DynWidget);
                    }
                    #[allow(unused_assignments)]
                    { i += 1; }
                )+
                None
            }

            fn len(&self) -> usize {
                $len
            }
        }

        // Note: No PushWidget implementation for Box<last_tuple> either
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
impl<W: crate::DynWidget> AssociatedArray for Vec<W> {
    type Array<T> = Vec<T>;

    fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T> {
        vec![value; self.len()]
    }

    fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn crate::DynWidget> {
        self.get_mut(index).map(|w| w as &mut dyn crate::DynWidget)
    }

    fn len(&self) -> usize {
        self.len()
    }
}

// PushWidget implementation for Vec - can only push the same type T
impl<T: crate::DynWidget> PushWidget<T> for Vec<T> {
    type Output = Vec<T>;

    fn push_widget(mut self, widget: T) -> Self::Output {
        self.push(widget);
        self
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
