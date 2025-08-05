use crate::Widget;
use alloc::vec::Vec;
use embedded_graphics::geometry::Size;

/// Trait to get tuple length at compile time and handle initialization
pub trait WidgetTuple {
    const TUPLE_LEN: usize;
    
    fn extract_sizes(&self) -> Vec<Size>;
}

// Macro to implement WidgetTuple for tuples
macro_rules! impl_widget_tuple {
    ($len:literal, $($t:ident),+) => {
        impl<$($t: Widget),+> WidgetTuple for ($($t,)+) {
            const TUPLE_LEN: usize = $len;
            
            fn extract_sizes(&self) -> Vec<Size> {
                #[allow(non_snake_case)]
                let ($(ref $t,)+) = self;
                
                let mut sizes = Vec::with_capacity($len);
                $(
                    sizes.push($t.size_hint().unwrap_or_default());
                )+
                sizes
            }
        }
    };
}

// Generate implementations for tuples up to 12 elements
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