mod column;
mod row;
mod stack;

pub use column::Column;
pub use row::Row;
pub use stack::{Positioned, Stack, StackAlignment};

/// Alignment options for the cross axis (horizontal for Column, vertical for Row)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossAxisAlignment {
    /// Align children to the start (left/top) of the cross axis
    Start,
    /// Center children along the cross axis
    Center,
    /// Align children to the end (right/bottom) of the cross axis
    End,
}

/// Defines how children are distributed along the main axis (vertical for Column, horizontal for Row)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainAxisAlignment {
    /// Place children at the start with no spacing between them
    Start,
    /// Center children with no spacing between them
    Center,
    /// Place children at the end with no spacing between them
    End,
    /// Place children with equal spacing between them, with no space before the first or after the last child
    /// Example with 3 children: [Child1]--space--[Child2]--space--[Child3]
    SpaceBetween,
    /// Place children with equal spacing around them, with half spacing before the first and after the last child
    /// Example with 3 children: -half-[Child1]-full-[Child2]-full-[Child3]-half-
    SpaceAround,
    /// Place children with equal spacing between and around them
    /// Example with 3 children: --space--[Child1]--space--[Child2]--space--[Child3]--space--
    SpaceEvenly,
}
