## Nonce Generation

Each device sends N devices to the coordinator,
Allow any device to be a signing device.

If you get a new phone, you can load back the key from a device.
But then you need to collect nonces from each device. (Bearable & natural)

If we provide each device all the other device's nonces then you can reload off a single device.

Generate nonces during keygen.
Instead of putting secret shares around, also put nonce shares around to different devices.

Need to remember used vs unused. If we share them around, so long as one device refuses to reuse a nonce then it will be secure. Don't need to choose the devices upfront.
Coordinator chooses a nonce that hasnt been used before.
Goes around to each device and recreate a device

Protect joint key instead of their own share from nonce reuse.

## Review

Signing parties are not an option
