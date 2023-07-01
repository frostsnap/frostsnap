# Coordinator-cli

The coordinator can be run with devices on multiple USB ports, or with multiple devices daisy-chained on a single USB port.

## Install and Run the coordinator

```
cd coordinator-cli/
cargo install --path .
coordinator-cli --help
```

Create a FROST key, here we'll make a 2-of-3:

```
coordinator-cli keygen -t 2 -n 3
```

Plug three devices in and follow the coordinator prompts to create a FROST key.

View the FROST key:

```
coordinator-cli key
```

Sign a message:

```
coordinator-cli sign "hello world"
```

Now select a threshold number of devices to sign with, and plug them in one at a time. Confirm the signature request on each device.

Once the threshold number of devices have each signed, the coordinator will combine the signature shares into a single Schnorr signature.

We can also get a bitcoin address belonging to this FROST key:

```
coordinator-cli address
```

Receive money to it and sync the wallet with

```
coordinator-cli sync
```

Then send bitcoin with

```
coordinator-cli send <ADDRESS> <VALUE>
```

Again, choosing and signing with a threshold number of devices.

## Help

```
Usage: coordinator-cli [OPTIONS] <COMMAND>

Commands:
  keygen   Create a new Frostsnap key (t-of-n)
  key      View the existing Frostsnap key
  sign     Sign a message, Bitcoin transaction, or Nostr post
  balance
  address
  sync
  send
  help     Print this message or the help of the given subcommand(s)

Options:
  -d, --db <FILE>  Database file (default: ~/.frostsnap)
  -v               Increase verbosity
  -h, --help       Print help
  -V, --version    Print version
```
