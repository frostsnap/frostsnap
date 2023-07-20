[<img alt="Frostsnap" src="https://frostsnap.com/assets/logo.svg" width=400>](https://frostsnap.com)

_Easy, Personalized, Secure Bitcoin Multisig_

<img alt="Frostsnap daisy chain" src="https://frostsnap.com/frostsnap.png" width=600>

Frostsnap devices seamlessly connect together in a daisy-chain. An easy way to create a Bitcoin wallet protected behind multiple devices (e.g. a `3-of-5`).

For an introduction on what this is all about, read our post [Introducing Frostsnap](https://frostsnap.com/introducing-frostsnap.html).

Find us on [frostsnap.com](https://frostsnap.com) or [@FrostsnapTech](https://twitter.com/FrostsnapTech).

## Code

Frostsnap uses our **experimental** [FROST](https://eprint.iacr.org/2020/852.pdf) implementation from [secp256kfun](https://docs.rs/schnorr_fun/latest/schnorr_fun/frost/index.html).

This repository is comprised of the following components:

- [device/](/device/) - The firmware which runs on ESP-32 microprocessors, handles message IO, user interaction, and display.
- [coordinator-cli/](/coordinator-cli/) - A simple CLI Bitcoin wallet which instructs devices what to do over USB serial. It can also post to Nostr!
- [frostsnap_core/](/frostsnap_core/) - Software library for handling the state of Frostsnap coordinators and devices, and how they respond to messages of different kinds.
- [frostsnap_comms/](/frostsnap_comms/) - Software library for how the devices and coordinators serialize different types of bincode messages.

All of this code is completely free open source software under the MIT license.

## Security & Disclaimer

Rather than worrying about access to a single hardware wallet or a physical seed, Frostsnap distributes security across a number of devices.

You can separate your signing devices geographically or share them amongst individuals you trust.

No longer is the security of each individual device so paramount. To compromise the key you need to compromise a threshold number of devices.

Since Frostsnap is under rapid development, we recommend against using it to secure your bitcoin just yet.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND. See LICENSE for more info.

## Contributors

This software is under rapid development, and we have many many things planned which are not yet implemented. Too many things to list in fact.

We would love your contributes in Frostsnap. For significant changes we suggest discussing or posting issues beforehand.

This software was initially built by @musdom, @LLFOURN, and @nickfarrow as a part of Frostsnap.

---

Check out the code!
