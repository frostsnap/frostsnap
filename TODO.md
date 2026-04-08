# TODO (esp-hal v1 migration)

Tracking shortcuts and stubs introduced while porting the `device` crate from
esp-hal v0.22 to v1.0.0. Each entry should be resolved before this branch ships.

## Stubbed out

- **`device/src/ds.rs` — `HardwareDs::sign` is `todo!()`.** Upstream esp-hal
  v1.0.0 does not expose a public driver for the DS peripheral (the frostsnap
  fork's `ds-eh1.0` branch has one, but we chose to stay on upstream for the
  migration). Any code path that ends up calling `HardwareDs::sign` — notably
  the challenge-response handler in `esp32_run.rs` — will panic at runtime.
  Either port the frostsnap DS driver in-tree or wait for an upstream driver
  before re-enabling this.

## v1 API quirks we should consider wrapping

- **`esp_hal::timer::timg::Timer` doesn't auto-start in v1.** In v0.22
  `Timer::new` called `set_counter_active(true)`; in v1 you have to call
  `Timer::start()` from the `esp_hal::timer::Timer` trait yourself or
  `timer.now()` stays frozen at 0. `DevicePeripherals::init` now does this
  explicitly, but it's a footgun — consider switching to
  `esp_hal::time::Instant::now()` (a free function backed by `SYSTIMER` that
  is initialised by `esp_hal::init` and always runs) and dropping the
  timg timer handles from `DevicePeripherals` / `Resources` / `FrostyUi` /
  `SerialInterface` entirely. That would also let us drop the
  `Box::leak(Box::new(timg0.timer0))` dance and the `T: timer::Timer` generic
  on `SerialInterface`.
