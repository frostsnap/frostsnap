[![Frostsnap](https://frostsnap.com/frostsnap_colour_shadow.png)](https://frostsnap.com)

_Control Your Keys - The Future of Bitcoin Security_

Read out post [Introducing Frostsnap](https://frostsnap.com/introducing-frostsnap.html).

![frostsnap daisy chain](https://frostsnap.com/frostsnap.png)

Frostsnap devices seamlessly connect together in a daisy-chain. A fun and easy way to create a Bitcoin wallet protected behind multiple devices (e.g. a `3-of-5`).

After key generation, you can geographically separate your Frostsnap devices or share them amongst individuals you trust.

You can then derive Bitcoin addresses from your FROST key, and receive bitcoin to your new wallet. To spend bitcoin, you can sign on each device one-at-a-time, keeping your Bitcoin secure at all times.

Excited? Join us on [frostsnap.com](https://frostsnap.com).

## What is FROST?

Take a listen to [Nick's podcast with Stephan Livera](https://stephanlivera.com/episode/476/) about FROST.

[FROST](https://eprint.iacr.org/2020/852.pdf) is a signature scheme that fixes many problems with today's multisigs. FROST achieves its threshold (`t-of-n`) nature through mathematics, rather than Bitcoin script.

FROST allows a major advancement in:

- User experience - A much more powerful and flexible multisig user experience, especially with Frostsnap devices. Unlike script multisig, it is possible to **add, remove, or recover signers** after key generation while keeping the public key the same.
- Spending policies - You will be able to **create custom signing policies based on your personal needs**. No need to do any risky or hacky methods like passphrases, splitting seeds, or shamir secret sharing. Secure your Bitcoin correctly.
- Privacy - **FROST leaves no multisig footprint onchain**, and is identical to single-signature taproot wallets. Unlike script multisigs which can be trivially identified,
- Fees - **Constant transaction sizes**, regardless of whether you are doing a `1-of-2`, `5-of-8`, or `50-of-100` the fees will be the same. FROST produces single signatures for the single public key, the Bitcoin script is always the same size regardless of the multisig.

Frostsnap uses our FROST implementation from [secp256kfun](https://docs.rs/schnorr_fun/latest/schnorr_fun/frost/index.html).

## Code

This repository is comprised of the following components:

- [device/](/device/) - The firmware which runs on esp-32 microprocessors, handles message IO, user interaction, and display.
- [coordinator-cli/](/coordinator-cli/) - A simple CLI Bitcoin wallet which instructs devices what to do over USB serial. It can also post to Nostr!
- [frostsnap_core/](/frostsnap_core/) - Software library for handling the state of Frostsnap coordinators and devices, and how they respond to messages of different kinds.
- [frostsnap_comms/](/frostsnap_comms/) - Software library for how the devices and coordinators serialize different types of bincode messages.

All of this code is completely free open source software under the XXX license.

## DIY: I want a frost-esp32 signing device right now!

It is possible to DIY build a frost-esp32 signing device. Though custom hardware is required for double-ended USBC devices which can form a daisy chain.

Currently we support the following ESP32C3 boards:

- "Blue boards": [ESP32C3-CORE](https://wiki.luatos.com/chips/esp32c3/board.html)
- "Purple boards": [ESP32-C3-0.42LCD](https://github.com/01Space/ESP32-C3-0.42LCD)

## Security & Disclaimer

We're somewhat moving away from the idea that each hardware signing devices needs to be insanely secure (though secure elements will likely return in future models).

Frostsnap multisigs provide distributed security, you can separate your signing devices geographically or share them amongst individuals you trust.

No longer is the individual security of each device so extremely paramount. Even if each device could be infiltrated and secrets extracted, you would have to physically attack a threshold number of devices in order to steal funds from the FROST key.

**This software is provided as is, and since it is under rapid development, we recommend against using it for securing your bitcoin just yet**. See the license for more info.

## Authors

This software has been primarily built by @musdom, @LLFOURN, and @nickfarrow as a part of the Frostsnap team.

## Contributors

This software is still under rapid development, and we have many many things planned which are not yet implemented. Too many things to list infact.

We would love to have you contribute to Frostsnap, however suggest that you discuss any significant additions/changes with us beforehand.

We suggest this because there's a good chance we've already considered what you have in mind or already have it planned, and we would love to share our insights with you!

---

We hope you're excited for the future of Bitcoin security.
