// Air101 5 way button driver

use esp32c3_hal::{
    gpio::{
        BankGpioRegisterAccess, Gpio13Signals, Gpio4Signals, Gpio5Signals, Gpio8Signals,
        Gpio9Signals, GpioPin, Input, InputOutputAnalogPinType, InputOutputPinType,
        InteruptStatusRegisterAccess, PullUp, Unknown,
    },
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

pub struct Buttons<RA, IRA>
where
    RA: BankGpioRegisterAccess,
    IRA: InteruptStatusRegisterAccess,
{
    center: GpioPin<Input<PullUp>, RA, IRA, InputOutputAnalogPinType, Gpio4Signals, 4>,
    up: GpioPin<Input<PullUp>, RA, IRA, InputOutputPinType, Gpio8Signals, 8>,
    down: GpioPin<Input<PullUp>, RA, IRA, InputOutputPinType, Gpio13Signals, 13>,
    right: GpioPin<Input<PullUp>, RA, IRA, InputOutputPinType, Gpio9Signals, 9>,
    left: GpioPin<Input<PullUp>, RA, IRA, InputOutputAnalogPinType, Gpio5Signals, 5>,
}

impl<'d, RA, IRA> Buttons<RA, IRA>
where
    RA: BankGpioRegisterAccess,
    IRA: InteruptStatusRegisterAccess,
{
    pub fn new(
        center: GpioPin<Unknown, RA, IRA, InputOutputAnalogPinType, Gpio4Signals, 4>,
        up: GpioPin<Unknown, RA, IRA, InputOutputPinType, Gpio8Signals, 8>,
        down: GpioPin<Unknown, RA, IRA, InputOutputPinType, Gpio13Signals, 13>,
        right: GpioPin<Unknown, RA, IRA, InputOutputPinType, Gpio9Signals, 9>,
        left: GpioPin<Unknown, RA, IRA, InputOutputAnalogPinType, Gpio5Signals, 5>,
    ) -> Self {
        Self {
            center: center.into_pull_up_input(),
            up: up.into_pull_up_input(),
            down: down.into_pull_up_input(),
            right: right.into_pull_up_input(),
            left: left.into_pull_up_input(),
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
