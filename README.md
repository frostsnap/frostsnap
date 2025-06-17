# Frostsnap

<img alt="Frostsnap daisy chain" src="https://frostsnap.com/frostypede_landscape.png" width=360>

[<img alt="Frostsnap" src="https://frostsnap.com/assets/frostsnap-logo.svg" width=360>](https://frostsnap.com)

_Frostsnap is the ultimate bitcoin self-custody system using the latest advancements in cryptography on distributed multisignature devices._

Having your keys in a single location makes you an inviting target to criminals. Sophisticated physical and digital thefts against bitcoin owners are becoming more prevalent.

Frostsnap devices distribute security across multiple locations using advanced multisignature. With a 2-of-3 setup, any two devices must sign to access your bitcoin. You can choose your number of devices and quorum.

Frostsnap devices can seamlessly connect together in a daisy-chain, providing an easy way to create or upgrade a Bitcoin wallet protected behind multiple devices.

Frostsnap devices are not trusted to generate keys on their own. Your phone or laptop participates in sensitive operations, including verifiably contributing entropy during key generation.

Bitcoin's Taproot upgrade has enabled elegant and secure Schnorr threshold signatures; a single public key that pays the same fees as single signature wallets, has hidden multisig for privacy, and straightforward recovery requirements.

## Code

Frostsnap uses our [FROST](https://eprint.iacr.org/2020/852.pdf) implementation from [secp256kfun](https://docs.rs/schnorr_fun/latest/schnorr_fun/frost/index.html).

This repository contains:

- **[device/](device/)** - ESP32 Rust firmware for frostsnap devices
- **[frostsnap_core/](frostsnap_core/)** - Core Rust library for coordinator and signer state management
- **[frostsnap_comms/](frostsnap_comms/)** - Communication protocol and message serialization
- **[frostsnapp/](frostsnapp/)** - Cross-platform Flutter wallet application and FROST coordinator

All code is free and open source under the MIT license.

## Security & Disclaimer

Rather than worrying about access to a single hardware wallet or physical seed, Frostsnap distributes security across multiple devices which should be stored in several different locations.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND. See [LICENSE](LICENSE) for more info.

## Contributors

We welcome contributions or issues related to this software.

This software was originally built by @musdom, @LLFOURN, @nickfarrow, and @evanlinjin as part of the Frostsnap team.

Find us at [frostsnap.com](https://frostsnap.com) or [@FrostsnapTech](https://x.com/FrostsnapTech).

---

Stay Frosty.
