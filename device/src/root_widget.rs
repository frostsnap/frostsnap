use crate::widget_tree::WidgetTree;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
};
use frostsnap_widgets::{DynWidget, FadeSwitcher, Widget};

/// Root widget that contains the main widget tree
pub struct RootWidget {
    pub page_switcher: FadeSwitcher<WidgetTree>,
}

impl RootWidget {
    pub fn new(
        initial_widget: WidgetTree,
        fade_duration_ms: u32,
        background_color: Rgb565,
    ) -> Self {
        let page_switcher =
            FadeSwitcher::new(initial_widget, fade_duration_ms, 30, background_color);

        Self { page_switcher }
    }

    /// Forward switch_to calls to the FadeSwitcher
    pub fn switch_to(&mut self, new_widget: WidgetTree) {
        self.page_switcher.switch_to(new_widget);
    }

    /// Get a mutable reference to the current widget
    pub fn current_mut(&mut self) -> &mut WidgetTree {
        self.page_switcher.current_mut()
    }
}

impl DynWidget for RootWidget {
    fn set_constraints(&mut self, max_size: Size) {
        self.page_switcher.set_constraints(max_size);
    }

    fn sizing(&self) -> frostsnap_widgets::Sizing {
        self.page_switcher.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: frostsnap_widgets::Instant,
        is_release: bool,
    ) -> Option<frostsnap_widgets::KeyTouch> {
        self.page_switcher
            .handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.page_switcher
            .handle_vertical_drag(prev_y, new_y, is_release)
    }

    fn force_full_redraw(&mut self) {
        self.page_switcher.force_full_redraw();
    }
}

impl Widget for RootWidget {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut frostsnap_widgets::SuperDrawTarget<D, Self::Color>,
        current_time: frostsnap_widgets::Instant,
    ) -> Result<(), D::Error> {
        self.page_switcher.draw(target, current_time)
    }
}
