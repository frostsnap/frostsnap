use crate::widget_tree::WidgetTree;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
};
use frostsnap_widgets::{DynWidget, Widget};

/// Root widget that contains the main widget tree
pub struct RootWidget {
    current: WidgetTree,
    constraints: Option<Size>,
}

impl RootWidget {
    pub fn new(initial_widget: WidgetTree) -> Self {
        Self {
            current: initial_widget,
            constraints: None,
        }
    }

    pub fn switch_to(&mut self, new_widget: WidgetTree) {
        self.current = new_widget;
        if let Some(max_size) = self.constraints {
            self.current.set_constraints(max_size);
        }
    }

    pub fn current_mut(&mut self) -> &mut WidgetTree {
        &mut self.current
    }
}

impl DynWidget for RootWidget {
    fn set_constraints(&mut self, max_size: Size) {
        self.constraints = Some(max_size);
        self.current.set_constraints(max_size);
    }

    fn sizing(&self) -> frostsnap_widgets::Sizing {
        self.current.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: frostsnap_widgets::Instant,
        is_release: bool,
    ) -> Option<frostsnap_widgets::KeyTouch> {
        self.current.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.current.handle_vertical_drag(prev_y, new_y, is_release)
    }

    fn force_full_redraw(&mut self) {
        self.current.force_full_redraw();
    }
}

impl Widget for RootWidget {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut frostsnap_widgets::SuperDrawTarget<D, Self::Color>,
        current_time: frostsnap_widgets::Instant,
    ) -> Result<(), D::Error> {
        self.current.draw(target, current_time)
    }
}
