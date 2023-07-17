// Air101 5 way button driver

use esp32c3_hal::{
    gpio::{AnyPin, Input, PullUp},
    prelude::_embedded_hal_digital_v2_InputPin,
};

pub enum ButtonDirection {
    Center,
    Up,
    Down,
    Right,
    Left,
    Unpressed,
}

pub struct Buttons {
    center: AnyPin<Input<PullUp>>,
    up: AnyPin<Input<PullUp>>,
    down: AnyPin<Input<PullUp>>,
    right: AnyPin<Input<PullUp>>,
    left: AnyPin<Input<PullUp>>,
}

impl Buttons {
    pub fn new(
        center: AnyPin<Input<PullUp>>,
        up: AnyPin<Input<PullUp>>,
        down: AnyPin<Input<PullUp>>,
        right: AnyPin<Input<PullUp>>,
        left: AnyPin<Input<PullUp>>,
    ) -> Self {
        Self {
            center,
            up,
            down,
            right,
            left,
        }
    }

    pub fn sample_buttons(&mut self) -> ButtonDirection {
        if self.center.is_low().unwrap() {
            ButtonDirection::Center
        } else if self.up.is_low().unwrap() {
            ButtonDirection::Up
        } else if self.down.is_low().unwrap() {
            ButtonDirection::Down
        } else if self.right.is_low().unwrap() {
            ButtonDirection::Right
        } else if self.left.is_low().unwrap() {
            ButtonDirection::Left
        } else {
            ButtonDirection::Unpressed
        }
    }
}
