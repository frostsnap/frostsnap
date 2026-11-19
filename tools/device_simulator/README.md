# Frostsnap Device Simulator

Desktop simulator process that emulates a Frostsnap hardware device over a pseudo-terminal (PTY) interface with an interactive window.

This tool is strictly for signet and testing purposes only. Simulator state files hold device private keys and secrets unencrypted on disk.

## Usage

### 1. Build the firmware image and the app

The app only accepts devices whose firmware matches the image it bundles, so build the dev image once and bundle it:

```sh
nix develop --command just env=dev build-firmware   # then save-image and sign-firmware, see build-firmware-signed
cd frostsnapp && FROSTSNAP_ENV=dev BUNDLE_FIRMWARE=1 flutter build macos --debug
```

### 2. Start simulated devices and app instances

Start each simulator with a new state file and no name, like a new device out of the box (the app names it during setup):

```sh
just sim-device ~/work/frost/dev1.state
just sim-device ~/work/frost/dev2.state
just sim-device ~/work/frost/dev3.state
```

Each prints `SIM PORT /dev/ttysNNN`. Start the app with all three ports, comma separated, so it sees them as plugged in:

```sh
APP=frostsnapp/build/macos/Build/Products/Debug/Frostsnap.app/Contents/MacOS/Frostsnap
FROSTSNAP_SIM_PORTS=/dev/ttysAAA,/dev/ttysBBB,/dev/ttysCCC $APP --data-dir=$HOME/work/frost/snap-1
```

Turn on developer mode (Settings, Advanced) to see signet.

### 3. Local 2-of-3 keygen and silent payment send

1. In the app, create a 2-of-3 signet wallet with the three simulated devices.
2. Confirm the keygen prompts on each device simulator window by clicking to confirm.
3. Review and record the backup on the simulator display.
4. In the send flow, choose **Add contact** and enter a name and a silent payment address.
5. Check the name and address on each simulator display and hold to trust it. Once two have, the contact is added.
6. Once funded on signet, pick the contact as the recipient.
7. Review the transaction prompt on the simulators, confirm the signing request, and broadcast the transaction.

## State Persistence

Device state (device keypair, device name, shares, and nonces) is persisted atomically to the specified state file with mode 0600 after every mutation and before any signature share or ECDH share message leaves the process. On restart, the simulator reloads its state and does not reuse nonces.
