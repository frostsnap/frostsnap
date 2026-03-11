use crate::super_draw_target::SuperDrawTarget;
use crate::Widget;
use embedded_graphics::{
    draw_target::DrawTarget,
    prelude::*,
    primitives::{Rectangle, StrokeAlignment},
};

#[derive(Clone, PartialEq)]
pub struct Container<W>
where
    W: Widget,
{
    pub child: W,
    width: Option<u32>,
    height: Option<u32>,
    border_color: Option<W::Color>,
    border_width: u32,
    fill_color: Option<W::Color>,
    corner_radius: Option<Size>,
    border_needs_redraw: bool,
    computed_sizing: Option<crate::Sizing>,
    child_rect: Option<Rectangle>,
}

impl<W: Widget> Container<W> {
    pub fn new(child: W) -> Self {
        Self {
            child,
            width: None,
            height: None,
            border_color: None,
            border_width: 0,
            fill_color: None,
            corner_radius: None,
            border_needs_redraw: true,
            computed_sizing: None,
            child_rect: None,
        }
    }

    pub fn with_size(child: W, size: Size) -> Self {
        Self {
            child,
            width: Some(size.width),
            height: Some(size.height),
            border_color: None,
            border_width: 0,
            fill_color: None,
            corner_radius: None,
            border_needs_redraw: true,
            computed_sizing: None,
            child_rect: None,
        }
    }

    pub fn with_width(mut self, width: u32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn with_height(mut self, height: u32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn with_border(mut self, color: W::Color, width: u32) -> Self {
        self.border_color = Some(color);
        self.border_width = width;
        self
    }

    pub fn with_fill(mut self, color: W::Color) -> Self {
        self.fill_color = Some(color);
        self
    }

    pub fn fill_color(&self) -> Option<W::Color> {
        self.fill_color
    }

    pub fn set_fill(&mut self, color: W::Color) {
        self.fill_color = Some(color);
        self.border_needs_redraw = true;
        self.child.force_full_redraw();
    }

    pub fn with_corner_radius(mut self, corner_radius: Size) -> Self {
        self.corner_radius = Some(corner_radius);
        self
    }

    pub fn with_expanded(mut self) -> Self {
        self.width = Some(u32::MAX);
        self.height = Some(u32::MAX);
        self
    }
}

impl<W: Widget> crate::DynWidget for Container<W> {
    fn set_constraints(&mut self, max_size: Size) {
        let container_width = self.width.unwrap_or(max_size.width).min(max_size.width);
        let container_height = self.height.unwrap_or(max_size.height).min(max_size.height);

        let child_max_size = Size::new(
            container_width.saturating_sub(2 * self.border_width),
            container_height.saturating_sub(2 * self.border_width),
        );
        self.child.set_constraints(child_max_size);

        let child_sizing = self.child.sizing();

        let width = if let Some(requested_width) = self.width {
            requested_width.min(max_size.width)
        } else {
            child_sizing.width + 2 * self.border_width
        };

        let height = if let Some(requested_height) = self.height {
            requested_height.min(max_size.height)
        } else {
            child_sizing.height + 2 * self.border_width
        };

        let dirty_rect = if self.border_width == 0 {
            self.child.sizing().dirty_rect
        } else {
            None
        };

        self.computed_sizing = Some(crate::Sizing {
            width,
            height,
            dirty_rect,
        });

        self.child_rect = Some(Rectangle::new(
            Point::new(self.border_width as i32, self.border_width as i32),
            Size::new(
                width.saturating_sub(2 * self.border_width),
                height.saturating_sub(2 * self.border_width),
            ),
        ));
    }

    fn sizing(&self) -> crate::Sizing {
        self.computed_sizing
            .expect("set_constraints must be called before sizing")
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        let child_point = Point::new(
            point.x - self.border_width as i32,
            point.y - self.border_width as i32,
        );
        if let Some(mut key_touch) = self
            .child
            .handle_touch(child_point, current_time, is_release)
        {
            key_touch.translate(Point::new(
                self.border_width as i32,
                self.border_width as i32,
            ));
            Some(key_touch)
        } else {
            None
        }
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.child.handle_vertical_drag(prev_y, new_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        self.border_needs_redraw = true;
        self.child.force_full_redraw();
    }
}

impl<W: Widget> Widget for Container<W> {
    type Color = W::Color;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, W::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = W::Color>,
    {
        let child_area = self
            .child_rect
            .expect("set_constraints must be called before draw");

        if self.border_needs_redraw && (self.border_color.is_some() || self.fill_color.is_some()) {
            let container_size = Size::from(
                self.computed_sizing
                    .expect("set_constraints must be called before draw"),
            );

            let border_rect = Rectangle::new(Point::zero(), container_size);

            if let Some(corner_radius) = self.corner_radius {
                let aa_rect = crate::aa_rounded_rect::AARoundedRectangle::new(
                    border_rect,
                    target.background_color(),
                )
                .with_corner_radius(corner_radius.width);

                let aa_rect = if let Some(fill_color) = self.fill_color {
                    aa_rect.with_fill(fill_color)
                } else {
                    aa_rect
                };

                let aa_rect = if let Some(border_color) = self.border_color {
                    aa_rect.with_border(border_color, self.border_width)
                } else {
                    aa_rect
                };

                aa_rect.draw(target)?;
            } else {
                let mut style_builder = embedded_graphics::primitives::PrimitiveStyleBuilder::new();

                if let Some(fill_color) = self.fill_color {
                    style_builder = style_builder.fill_color(fill_color);
                }

                if let Some(border_color) = self.border_color {
                    style_builder = style_builder
                        .stroke_color(border_color)
                        .stroke_width(self.border_width)
                        .stroke_alignment(StrokeAlignment::Inside);
                }

                let style = style_builder.build();
                border_rect.into_styled(style).draw(target)?;
            }

            self.border_needs_redraw = false;
        }

        let mut child_target = target.clone().crop(child_area);
        if let Some(fill_color) = self.fill_color {
            child_target = child_target.with_background_color(fill_color);
        }
        self.child.draw(&mut child_target, current_time)?;

        Ok(())
    }
}

impl<W: Widget> core::fmt::Debug for Container<W>
where
    W: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Container")
            .field("child", &self.child)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("has_border", &self.border_color.is_some())
            .field("border_width", &self.border_width)
            .finish()
    }
}
