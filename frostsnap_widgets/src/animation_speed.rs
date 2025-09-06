use crate::Frac;

/// Animation speed curve for controlling how animations progress over time
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationSpeed {
    /// Linear interpolation - constant speed throughout
    Linear,
    /// Cubic bezier ease-out curve (0.0, 0.0, 0.58, 1.0)
    /// Starts fast and slows down at the end
    EaseOut,
}

impl AnimationSpeed {
    /// Apply the animation speed curve to a progress value (0.0 to 1.0)
    /// Returns the eased progress value
    pub fn apply(&self, progress: Frac) -> Frac {
        match self {
            AnimationSpeed::Linear => progress,
            AnimationSpeed::EaseOut => {
                // Ease-out cubic bezier approximation using fixed point math
                // This approximates the curve (0.0, 0.0, 0.58, 1.0)
                // Formula: 1 - (1 - t)³
                let one_minus_t = Frac::ONE - progress;
                let one_minus_t_squared = one_minus_t * one_minus_t;
                let one_minus_t_cubed = one_minus_t_squared * one_minus_t;
                Frac::ONE - one_minus_t_cubed
            }
        }
    }
}
