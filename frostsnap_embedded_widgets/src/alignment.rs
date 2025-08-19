use super::Widget;
use crate::{super_draw_target::SuperDrawTarget, Instant};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    primitives::Rectangle,
};

/// Alignment for positioning widgets
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Alignment {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Default for Alignment {
    fn default() -> Self {
        Self::TopLeft
    }
}

impl Alignment {
    /// Calculate the x offset for the given alignment
    pub fn x_offset(&self, container_width: u32, child_width: u32) -> i32 {
        match self {
            Alignment::TopLeft | Alignment::CenterLeft | Alignment::BottomLeft => 0,
            Alignment::TopCenter | Alignment::Center | Alignment::BottomCenter => {
                ((container_width.saturating_sub(child_width)) / 2) as i32
            }
            Alignment::TopRight | Alignment::CenterRight | Alignment::BottomRight => {
                (container_width.saturating_sub(child_width)) as i32
            }
        }
    }

    /// Calculate the y offset for the given alignment
    pub fn y_offset(&self, container_height: u32, child_height: u32) -> i32 {
        match self {
            Alignment::TopLeft | Alignment::TopCenter | Alignment::TopRight => 0,
            Alignment::CenterLeft | Alignment::Center | Alignment::CenterRight => {
                ((container_height.saturating_sub(child_height)) / 2) as i32
            }
            Alignment::BottomLeft | Alignment::BottomCenter | Alignment::BottomRight => {
                (container_height.saturating_sub(child_height)) as i32
            }
        }
    }

    /// Calculate both x and y offsets
    pub fn offset(&self, container_size: Size, child_size: Size) -> Point {
        Point::new(
            self.x_offset(container_size.width, child_size.width),
            self.y_offset(container_size.height, child_size.height),
        )
    }
}

/// Horizontal alignment options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HorizontalAlignment {
    Left,
    Center,
    Right,
}

impl HorizontalAlignment {
    pub fn x_offset(&self, container_width: u32, child_width: u32) -> i32 {
        match self {
            HorizontalAlignment::Left => 0,
            HorizontalAlignment::Center => {
                ((container_width.saturating_sub(child_width)) / 2) as i32
            }
            HorizontalAlignment::Right => (container_width.saturating_sub(child_width)) as i32,
        }
    }
}

/// Vertical alignment options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

impl VerticalAlignment {
    pub fn y_offset(&self, container_height: u32, child_height: u32) -> i32 {
        match self {
            VerticalAlignment::Top => 0,
            VerticalAlignment::Center => {
                ((container_height.saturating_sub(child_height)) / 2) as i32
            }
            VerticalAlignment::Bottom => (container_height.saturating_sub(child_height)) as i32,
        }
    }
}

/// A widget that aligns its child with customizable horizontal and vertical alignment
#[derive(PartialEq)]
pub struct Align<W> {
    pub child: W,
    pub horizontal: HorizontalAlignment,
    pub vertical: VerticalAlignment,
    /// If true, the widget expands to fill available space on the horizontal axis
    pub expand_horizontal: bool,
    /// If true, the widget expands to fill available space on the vertical axis
    pub expand_vertical: bool,
    constraints: Option<Size>,
    child_rect: Rectangle,
}

impl<W> Align<W> {
    /// Create an Align widget with default top-left alignment that shrink-wraps
    pub fn new(child: W) -> Self {
        Self {
            child,
            horizontal: HorizontalAlignment::Left,
            vertical: VerticalAlignment::Top,
            expand_horizontal: false,
            expand_vertical: false,
            constraints: None,
            child_rect: Rectangle::zero(),
        }
    }

    /// Set horizontal alignment (consumes self for chaining)
    pub fn horizontal(mut self, alignment: HorizontalAlignment) -> Self {
        self.horizontal = alignment;
        self.expand_horizontal = true;
        self
    }

    /// Set vertical alignment (consumes self for chaining)
    pub fn vertical(mut self, alignment: VerticalAlignment) -> Self {
        self.vertical = alignment;
        self.expand_vertical = true;
        self
    }

    /// Create an Align widget with a combined alignment
    pub fn align(child: W, alignment: Alignment) -> Self {
        let (horizontal, vertical) = match alignment {
            Alignment::TopLeft => (HorizontalAlignment::Left, VerticalAlignment::Top),
            Alignment::TopCenter => (HorizontalAlignment::Center, VerticalAlignment::Top),
            Alignment::TopRight => (HorizontalAlignment::Right, VerticalAlignment::Top),
            Alignment::CenterLeft => (HorizontalAlignment::Left, VerticalAlignment::Center),
            Alignment::Center => (HorizontalAlignment::Center, VerticalAlignment::Center),
            Alignment::CenterRight => (HorizontalAlignment::Right, VerticalAlignment::Center),
            Alignment::BottomLeft => (HorizontalAlignment::Left, VerticalAlignment::Bottom),
            Alignment::BottomCenter => (HorizontalAlignment::Center, VerticalAlignment::Bottom),
            Alignment::BottomRight => (HorizontalAlignment::Right, VerticalAlignment::Bottom),
        };
        Self {
            child,
            horizontal,
            vertical,
            expand_horizontal: true,
            expand_vertical: true,
            constraints: None,
            child_rect: Rectangle::zero(),
        }
    }
}

impl<W: Widget> crate::DynWidget for Align<W> {
    fn set_constraints(&mut self, max_size: Size) {
        self.constraints = Some(max_size);
        self.child.set_constraints(max_size);

        let child_size: Size = self.child.sizing().into();

        // Calculate position based on alignment settings
        let x_offset = if self.expand_horizontal {
            self.horizontal.x_offset(max_size.width, child_size.width)
        } else {
            // Not expanding - position at left (shrink-wrap behavior)
            0
        };

        let y_offset = if self.expand_vertical {
            self.vertical.y_offset(max_size.height, child_size.height)
        } else {
            // Not expanding - position at top (shrink-wrap behavior)
            0
        };

        self.child_rect = Rectangle::new(Point::new(x_offset, y_offset), child_size);
    }

    fn sizing(&self) -> crate::Sizing {
        let constraints = self.constraints.unwrap();
        let child_sizing = self.child.sizing();

        // If expanding on an axis, use full available space on that axis
        // Otherwise, shrink-wrap to child size
        crate::Sizing {
            width: if self.expand_horizontal {
                constraints.width
            } else {
                child_sizing.width
            },
            height: if self.expand_vertical {
                constraints.height
            } else {
                child_sizing.height
            },
        }
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Check if the touch is within the child's bounds
        if self.child_rect.contains(point) || is_release {
            // Translate the touch point to the child's coordinate system
            let translated_point = Point::new(
                point.x - self.child_rect.top_left.x,
                point.y - self.child_rect.top_left.y,
            );
            if let Some(mut key_touch) = self.child
                .handle_touch(translated_point, current_time, is_release) {
                // Translate the KeyTouch rectangle back to parent coordinates
                key_touch.translate(self.child_rect.top_left);
                Some(key_touch)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, is_release: bool) {
        self.child
            .handle_vertical_drag(start_y, current_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw()
    }
}

impl<W: Widget> Widget for Align<W> {
    type Color = W::Color;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.constraints.unwrap();

        self.child
            .draw(&mut target.clone().crop(self.child_rect), current_time)?;

        Ok(())
    }
}
