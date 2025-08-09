/// Trait to get tuple length at compile time
pub trait WidgetTuple {
    const TUPLE_LEN: usize;
}

// Macro to implement WidgetTuple for tuples
macro_rules! impl_widget_tuple {
    ($len:literal, $($t:ident),+) => {
        impl<$($t),+> WidgetTuple for ($($t,)+) {
            const TUPLE_LEN: usize = $len;
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
impl_widget_tuple!(18, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18);
impl_widget_tuple!(19, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19);
impl_widget_tuple!(20, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20);