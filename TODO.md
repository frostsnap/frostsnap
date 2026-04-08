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

