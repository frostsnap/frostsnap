/// Trait to get tuple length at compile time
pub trait WidgetTuple {
    const TUPLE_LEN: usize;

    /// Generic associated type for adding a widget to the tuple
    type Add<W>: WidgetTuple;

    /// Generic associated type for fixed-size arrays
    type Array<T>: AsRef<[T]> + AsMut<[T]>;

    /// Add a widget to this tuple
    fn add<W>(self, widget: W) -> Self::Add<W>;

    /// Create an array filled with default values
    fn create_array<T: Default + Copy>() -> Self::Array<T>;

    /// Create an array filled with a specific value
    fn create_array_with<T: Copy>(value: T) -> Self::Array<T>;
}

// Special implementation for empty tuple
impl WidgetTuple for () {
    const TUPLE_LEN: usize = 0;
    type Add<W> = (W,);
    type Array<T> = [T; 0];

    fn add<W>(self, widget: W) -> Self::Add<W> {
        (widget,)
    }

    fn create_array<T: Default + Copy>() -> Self::Array<T> {
        []
    }

    fn create_array_with<T: Copy>(_value: T) -> Self::Array<T> {
        []
    }
}

// Macro to implement WidgetTuple for tuples
macro_rules! impl_widget_tuple {
    ($len:literal, $($t:ident),+) => {
        impl<$($t),+> WidgetTuple for ($($t,)+) {
            const TUPLE_LEN: usize = $len;
            type Add<W> = ($($t,)+ W);
            type Array<T> = [T; $len];

            fn add<W>(self, widget: W) -> Self::Add<W> {
                #[allow(non_snake_case)]
                let ($($t,)+) = self;
                ($($t,)+ widget)
            }

            fn create_array<T: Default + Copy>() -> Self::Array<T> {
                [T::default(); $len]
            }

            fn create_array_with<T: Copy>(value: T) -> Self::Array<T> {
                [value; $len]
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
// Special case for 20-tuple - can't add more
impl<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20>
    WidgetTuple
    for (
        T1,
        T2,
        T3,
        T4,
        T5,
        T6,
        T7,
        T8,
        T9,
        T10,
        T11,
        T12,
        T13,
        T14,
        T15,
        T16,
        T17,
        T18,
        T19,
        T20,
    )
{
    const TUPLE_LEN: usize = 20;
    type Add<W> = Self; // Can't add more, return self
    type Array<T> = [T; 20];

    fn add<W>(self, _widget: W) -> Self::Add<W> {
        panic!("Cannot add more than 20 widgets to a tuple");
    }

    fn create_array<T: Default + Copy>() -> Self::Array<T> {
        [T::default(); 20]
    }

    fn create_array_with<T: Copy>(value: T) -> Self::Array<T> {
        [value; 20]
    }
}
