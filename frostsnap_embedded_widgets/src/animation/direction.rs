#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransitionDirection {
    Left,
    Right,
    Up,
    Down,
}

impl TransitionDirection {
    pub fn is_horizontal(&self) -> bool {
        matches!(self, TransitionDirection::Left | TransitionDirection::Right)
    }

    pub fn is_vertical(&self) -> bool {
        matches!(self, TransitionDirection::Up | TransitionDirection::Down)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Up,
    Down,
}
