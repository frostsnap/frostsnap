[<img alt="Frostsnap" src="https://frostsnap.com/assets/logo.svg" width=400>](https://frostsnap.com)

_Easy, Personalized, Secure Bitcoin Multisig_

Read out post [Introducing Frostsnap](https://frostsnap.com/introducing-frostsnap.html).

<img alt="Frostsnap daisy chain" src="https://frostsnap.com/frostsnap.png" width=600>

Frostsnap devices seamlessly connect together in a daisy-chain. A fun and easy way to create a Bitcoin wallet protected behind multiple devices (e.g. a `3-of-5`).

After key generation, you can geographically separate your Frostsnap devices or share them amongst individuals you trust.

You can then derive Bitcoin addresses from your FROST key, and receive bitcoin to your new wallet. To spend bitcoin, you can sign on each device one-at-a-time, keeping your Bitcoin secure at all times.

Find us on [frostsnap.com](https://frostsnap.com) or [@FrostsnapTech](https://twitter.com/FrostsnapTech).

## What is FROST?

[FROST](https://eprint.iacr.org/2020/852.pdf) is a signature scheme that fixes many problems with today's multisigs. FROST achieves its threshold (`t-of-n`) nature through mathematics, rather than Bitcoin script.

FROST allows major advancements in:

- User experience - A much more powerful and flexible multisig user experience, especially with Frostsnap devices. Unlike script multisig, it is possible to **add, remove, or recover signers** after key generation while keeping the public key the same.
- Spending policies - You will be able to **create custom signing policies based on your personal needs**. No need to do any risky or hacky methods like passphrases, splitting seeds, or shamir secret sharing. Secure your Bitcoin correctly.
- Privacy - **FROST leaves no multisig footprint onchain**, and is identical to single-signature taproot wallets. Unlike script multisigs which can be trivially identified,
- Fees - **Constant transaction sizes**, regardless of whether you are doing a `1-of-2`, `5-of-8`, or `50-of-100` the fees will be the same. FROST produces single signatures for the single public key, the Bitcoin script is always the same size regardless of the multisig.

Take a listen to [Nick's podcast with Stephan Livera](https://stephanlivera.com/episode/476/) about FROST.

Frostsnap uses our FROST implementation from [secp256kfun](https://docs.rs/schnorr_fun/latest/schnorr_fun/frost/index.html).

## Code

This repository is comprised of the following components:

- [device/](/device/) - The firmware which runs on ESP-32 microprocessors, handles message IO, user interaction, and display.
- [coordinator-cli/](/coordinator-cli/) - A simple CLI Bitcoin wallet which instructs devices what to do over USB serial. It can also post to Nostr!
- [frostsnap_core/](/frostsnap_core/) - Software library for handling the state of Frostsnap coordinators and devices, and how they respond to messages of different kinds.
- [frostsnap_comms/](/frostsnap_comms/) - Software library for how the devices and coordinators serialize different types of bincode messages.

All of this code is completely free open source software under the MIT license.

## Flashing a Dev Board

It is possible to DIY flash a frost signing device for development, but will require custom hardware modifications.

Currently we support the following base ESP32C3 boards:

- "Purple boards": [ESP32-C3-0.42LCD](https://github.com/01Space/ESP32-C3-0.42LCD) - Optional requirement of soldering an additional USBC port for daisy chaining devices. Currently lacks button interaction.
- "Blue boards": [ESP32C3-CORE](https://wiki.luatos.com/chips/esp32c3/board.html) - Requires additional hardware for a screen and buttons. Optional requirement of soldering an additional USBC port for daisy chaining devices.

## Security & Disclaimer

Rather than worrying about hardware wallet or a physical seed, Frostsnap distributes security across a number of devices.

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
