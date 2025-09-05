# frostsnap_cst816s

A no_std driver for the Hynan / Hynitron CST816S touchpad device with ESP32 interrupt support, used in the Frostsnap hardware signing device.

This driver is based on the original [cst816s](https://github.com/tstellanova/cst816s) crate by Todd Stellanova and [Felix Weidinger's fork](https://github.com/fxweidinger/cst816s).

## Features
- Blocking mode read of available touch events
- Reading slides and long press gestures  
- Hardware interrupt support for ESP32C3
- Event queue for buffering touch events

## Original Work

This crate is derived from:
- Original implementation: https://github.com/tstellanova/cst816s by Todd Stellanova
- Fork with improvements: https://github.com/fxweidinger/cst816s by Felix Weidinger

## Resources
- [Datasheet](https://github.com/tstellanova/cst816s/blob/main/CST816S_V1.1.en.pdf)
- [Reference Driver](https://github.com/tstellanova/hynitron_i2c_cst0xxse) in C, from Hynitron

## License

BSD-3-Clause, see `LICENSE` file.
