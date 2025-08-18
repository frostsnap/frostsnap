use super::Widget;
use crate::{super_draw_target::SuperDrawTarget, DynWidget};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::{
    draw_target::DrawTarget,
    prelude::*,
    primitives::{Rectangle, RoundedRectangle, StrokeAlignment},
};

/// A container that can optionally draw a border around its child
#[derive(PartialEq)]
pub struct Container<W>
where
    W: Widget,
{
    pub child: W,
    width: Option<u32>, // None = shrink-wrap, Some(width) = explicit width (including MAX for fill)
    height: Option<u32>, // None = shrink-wrap, Some(height) = explicit height (including MAX for fill)
    border_color: Option<W::Color>,
    border_width: u32,
    fill_color: Option<W::Color>,
    corner_radius: Option<Size>,
    border_needs_redraw: bool,
    constraints: Option<Size>,
}

impl<W: Widget> Container<W> {
    /// Create a container that inherits size from its child
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
            constraints: None,
        }
    }

    /// Create a container with an explicit size
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
            constraints: None,
        }
    }

    /// Set the width of the container
    pub fn with_width(mut self, width: u32) -> Self {
        self.width = Some(width);
        self
    }

    /// Set the height of the container
    pub fn with_height(mut self, height: u32) -> Self {
        self.height = Some(height);
        self
    }

    /// Set the border with a color and width
    pub fn with_border(mut self, color: W::Color, width: u32) -> Self {
        self.border_color = Some(color);
        self.border_width = width;
        self
    }

    /// Set the fill color
    pub fn with_fill(mut self, color: W::Color) -> Self {
        self.fill_color = Some(color);
        self
    }

    /// Set the corner radius for rounded borders
    pub fn with_corner_radius(mut self, corner_radius: Size) -> Self {
        self.corner_radius = Some(corner_radius);
        self
    }

    /// Set the container to expanded mode - it will fill the available space
    pub fn with_expanded(mut self) -> Self {
        // Expanded means requesting u32::MAX size
        self.width = Some(u32::MAX);
        self.height = Some(u32::MAX);
        self
    }
}

impl<W: Widget> crate::DynWidget for Container<W> {
    fn set_constraints(&mut self, max_size: Size) {
        self.constraints = Some(max_size);

        // Calculate child constraints based on our width and height settings
        let container_width = self.width.unwrap_or(max_size.width).min(max_size.width);
        let container_height = self.height.unwrap_or(max_size.height).min(max_size.height);

        let child_max_size = Size::new(
            container_width.saturating_sub(2 * self.border_width),
            container_height.saturating_sub(2 * self.border_width),
        );
        self.child.set_constraints(child_max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        let constraints = self
            .constraints
            .expect("set_constraints must be called before sizing");

        let child_sizing = self.child.sizing();

        // Calculate width: use explicit width if set, otherwise shrink-wrap
        let width = if let Some(requested_width) = self.width {
            requested_width.min(constraints.width)
        } else {
            child_sizing.width + 2 * self.border_width
        };

        // Calculate height: use explicit height if set, otherwise shrink-wrap
        let height = if let Some(requested_height) = self.height {
            requested_height.min(constraints.height)
        } else {
            child_sizing.height + 2 * self.border_width
        };

        crate::Sizing { width, height }
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Offset point by border width when passing to child
        let child_point = Point::new(
            point.x - self.border_width as i32,
            point.y - self.border_width as i32,
        );
        self.child
            .handle_touch(child_point, current_time, is_release)
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
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.constraints.expect("constraints must be set");

        // Get our actual size
        let container_size = Size::from(self.sizing());

        // Draw border if needed
        if self.border_needs_redraw && (self.border_color.is_some() || self.fill_color.is_some()) {
            let border_rect = Rectangle::new(Point::zero(), container_size);

            // Build the primitive style with inside stroke alignment
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

            if let Some(corner_radius) = self.corner_radius {
                RoundedRectangle::with_equal_corners(border_rect, corner_radius)
                    .into_styled(style)
                    .draw(target)?;
            } else {
                border_rect.into_styled(style).draw(target)?;
            }

            self.border_needs_redraw = false;
        }

        // Draw child with proper offset and cropping
        let child_area = Rectangle::new(
            Point::new(self.border_width as i32, self.border_width as i32),
            Size::new(
                container_size.width.saturating_sub(2 * self.border_width),
                container_size.height.saturating_sub(2 * self.border_width),
            ),
        );

        self.child
            .draw(&mut target.clone().crop(child_area), current_time)?;

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
