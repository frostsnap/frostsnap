// Air101 5 way button driver

use esp32c3_hal::{
    gpio::{
        BankGpioRegisterAccess, Gpio13Signals, Gpio4Signals, Gpio5Signals, Gpio8Signals,
        Gpio9Signals, GpioPin, Input, InputOutputAnalogPinType, InputOutputPinType,
        InteruptStatusRegisterAccess, PullUp, Unknown
    },
    prelude::{_embedded_hal_digital_v2_InputPin, _esp_hal_timer_Instance}, timer::{Timer, Timer0},
    peripherals::TIMG0
};
use esp_println::println;

pub enum ButtonDirection {
    Center,
    Up,
    Down,
    Right,
    Left,
}

pub struct Buttons<RA, IRA>
where
    RA: BankGpioRegisterAccess,
    IRA: InteruptStatusRegisterAccess,
{
    last_press: u64,
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
            // timer,
            last_press: 0u64,
            center: center.into_pull_up_input(),
            up: up.into_pull_up_input(),
            down: down.into_pull_up_input(),
            right: right.into_pull_up_input(),
            left: left.into_pull_up_input(),
        }
    }

    pub fn wait_for_press(&mut self) -> ButtonDirection {
        let mut pressed = false;
        let mut position: ButtonDirection = ButtonDirection::Center;
        loop {
            while self.center.is_low().unwrap() {
                if !pressed {
                    position = ButtonDirection::Center;
                    pressed = true;
                }
            }

            while self.up.is_low().unwrap() {
                if !pressed {
                    position = ButtonDirection::Up;
                    pressed = true;
                }
            }

            while self.down.is_low().unwrap() {
                if !pressed {
                    position = ButtonDirection::Down;
                    pressed = true;
                }
            }

            while self.right.is_low().unwrap() {
                if !pressed {
                    position = ButtonDirection::Right;
                    pressed = true;
                }
            }

            while self.left.is_low().unwrap() {
                if !pressed {
                    position = ButtonDirection::Left;
                    pressed = true;
                }
            }

            if pressed {
                return position;
            }
        }
    }
}