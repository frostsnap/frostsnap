use crate::super_draw_target::SuperDrawTarget;
use crate::{AnyDynWidget, DynWidget, Instant, Widget};
use alloc::boxed::Box;
use core::any::{Any, TypeId};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
};

/// A widget that can be any one of the types in the tuple T
/// This provides type-safe dynamic dispatch for widgets
pub struct AnyOf<T> {
    inner: Box<dyn AnyDynWidget>,
    _phantom: core::marker::PhantomData<T>,
}

impl<T> AnyOf<T> {
    /// Create a new AnyOf from a widget that must be one of the types in T
    pub fn new<W: AnyDynWidget + 'static>(widget: W) -> Self {
        Self {
            inner: Box::new(widget),
            _phantom: core::marker::PhantomData,
        }
    }

    /// Try to get a mutable reference to the inner widget as a specific type
    pub fn downcast_mut<W: Any>(&mut self) -> Option<&mut W> {
        let widget = self.inner.as_mut() as &mut dyn Any;
        widget.downcast_mut::<W>()
    }

    /// Try to get a reference to the inner widget as a specific type
    pub fn downcast_ref<W: Any>(&self) -> Option<&W> {
        let widget = self.inner.as_ref() as &dyn Any;
        widget.downcast_ref::<W>()
    }
}

// Helper macro to implement DynWidget for AnyOf
macro_rules! impl_any_of {
    ($($t:ident),+) => {
        impl<$($t: AnyDynWidget + PartialEq + 'static),+> PartialEq for AnyOf<($($t,)+)> {
            fn eq(&self, other: &Self) -> bool {
                let self_type_id = self.inner.as_ref().type_id();
                let other_type_id = other.inner.as_ref().type_id();

                // First check if they're the same type
                if self_type_id != other_type_id {
                    return false;
                }

                // Try to downcast to each possible type and compare
                $(
                    if self_type_id == TypeId::of::<$t>() {
                        let self_widget = self.inner.as_ref() as &dyn Any;
                        let other_widget = other.inner.as_ref() as &dyn Any;

                        let self_widget = self_widget.downcast_ref::<$t>().unwrap();
                        let other_widget = other_widget.downcast_ref::<$t>().unwrap();

                        return self_widget == other_widget;
                    }
                )+

                // This should never happen if AnyOf is constructed properly
                false
            }
        }

        impl<$($t: AnyDynWidget + 'static),+> DynWidget for AnyOf<($($t,)+)>
        {
            fn set_constraints(&mut self, max_size: Size) {
                self.inner.set_constraints(max_size)
            }

            fn sizing(&self) -> crate::Sizing {
                self.inner.sizing()
            }

            fn flex(&self) -> bool {
                self.inner.flex()
            }

            fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) -> Option<crate::KeyTouch> {
                self.inner.handle_touch(point, current_time, is_release)
            }

            fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
                self.inner.handle_vertical_drag(prev_y, new_y, is_release)
            }

            fn force_full_redraw(&mut self) {
                self.inner.force_full_redraw()
            }
        }

        impl<$($t: Widget<Color = COLOR> + 'static),+, COLOR: crate::WidgetColor> Widget for AnyOf<($($t,)+)>
        {
            type Color = COLOR;

            fn draw<DT>(&mut self, target: &mut crate::super_draw_target::SuperDrawTarget<DT, COLOR>, current_time: Instant) -> Result<(), DT::Error>
            where
                DT: DrawTarget<Color = COLOR>,
            {
                let type_id = self.inner.as_ref().type_id();

                $(
                    if type_id == TypeId::of::<$t>() {
                        let widget = self.inner.as_mut() as &mut dyn Any;
                        let widget = widget.downcast_mut::<$t>().unwrap();
                        return widget.draw(target, current_time);
                    }
                )+

                // This should never happen if AnyOf is constructed properly
                panic!("AnyOf inner widget type not in tuple");
            }
        }
    };
}

// Generate implementations for tuples of different sizes
impl_any_of!(A);
impl_any_of!(A, B);
impl_any_of!(A, B, C);
impl_any_of!(A, B, C, D);
impl_any_of!(A, B, C, D, E);
impl_any_of!(A, B, C, D, E, F);
impl_any_of!(A, B, C, D, E, F, G);
impl_any_of!(A, B, C, D, E, F, G, H);
impl_any_of!(A, B, C, D, E, F, G, H, I);
impl_any_of!(A, B, C, D, E, F, G, H, I, J);
impl_any_of!(A, B, C, D, E, F, G, H, I, J, K);
impl_any_of!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_any_of!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_any_of!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_any_of!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
