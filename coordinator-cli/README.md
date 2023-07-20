# Coordinator-cli

The coordinator can be run with devices on multiple USB ports, or with multiple devices daisy-chained on a single USB port.

Currently Frostsnap's `coordinator-cli` can:

- Orchestrate a `t-of-n` keygen
- Request devices to sign messages
- Print key information
- Derive taproot bitcoin addresses, receive bitcoin, sync wallet
- Spend bitcoin by requesting devices to sign transaction witnesses
- Request signatures for a Nostr message, and broadcast

## Install and Run the coordinator

```
cd coordinator-cli/
cargo install --path .
coordinator-cli --help
```
