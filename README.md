[<img alt="Frostsnap" src="https://frostsnap.com/frostsnap-logo-boxed.png" width=400>](https://frostsnap.com)

_Easy, Personalized, Secure Bitcoin Self-Custody_

<img alt="Frostsnap daisy chain" src="https://frostsnap.com/frostypede_landscape.png" width=400>

Frostsnap is building the ultimate bitcoin self-custody experience using the latest advancements in cryptography.

With a single Frostsnap device used in conjunction with a mobile phone you gain the power to create an **accessible and highly secure** bitcoin wallet.

Frostsnap devices can seamlessly connect together in a daisy-chain. Providing an easy way to create **or upgrade** a Bitcoin wallet to be protected behind multiple devices (e.g. a `3-of-5` multisignature wallet).

In the future, Frostsnap aims to encompass every aspect of your Bitcoin journey, offering customized security policies ranging from daily spending access to inheritance planning.

For an introduction on what this is all about, read our post [Introducing Frostsnap](https://frostsnap.com/introducing-frostsnap.html).

While Frostsnap devices are not yet available for purchase, stay connected with us to ensure you don't miss out!

Find us on [frostsnap.com](https://frostsnap.com) or [@FrostsnapTech](https://twitter.com/FrostsnapTech).

## Code

Frostsnap uses our [FROST](https://eprint.iacr.org/2020/852.pdf) implementation from [secp256kfun](https://docs.rs/schnorr_fun/latest/schnorr_fun/frost/index.html).

This repository is comprised of the following components:

- [device/](/device/) - The firmware which runs on ESP-32 microprocessors, handling message IO, user interaction, and display.
- [frostsnap_core/](/frostsnap_core/) - Software library for handling the state of Frostsnap coordinators and signers, and how to respond to messages of different kinds.
- [frostsnap_comms/](/frostsnap_comms/) - Software library for how the devices and coordinators serialize different types of bincode messages.
- [frostsnapp/](/frostsnapp/) - Desktop and mobile wallet app.

All of this code is completely free open source software under the MIT license.

## Security & Disclaimer

Rather than worrying about access to a single hardware wallet or a physical seed, Frostsnap distributes security across a number of devices.

You can separate your signing devices geographically or share them amongst individuals you trust.

No longer is the security of each individual device so paramount. To compromise the key you need to compromise a threshold number of devices.

Since Frostsnap is under rapid development, **we recommend against using it to secure your bitcoin just yet**.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND. See LICENSE for more info.

## Contributors

This software is under rapid development, and we have many many things planned which are not yet implemented. Too many things to list in fact.

We would love your contributions to Frostsnap. For significant changes we suggest discussing or posting issues beforehand.

This software was initially built by @musdom, @LLFOURN, and @nickfarrow as a part of the Frostsnap team.

---

Stay Frosty.
