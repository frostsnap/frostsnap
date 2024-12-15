# Frostsnap Permission Design

Frostsnap devices are used in conjunction with coordinators that request actions which are confirmed on the device. Frostsnap devices should not reveal sensitive information to, or perform sensitive actions with, unknown coordinators.

For example, if someone finds a Frostsnap device, they should not be able to immediately initiate signing or learn any public keys.

We design permissions around coordinators being either AWARE or UNAWARE of keys held on the device:

1. If a coordinator is AWARE of a key, that coordinator can perform operations including keygen, device renaming, updating, and signing with that key.
2. If the device has no keys for a coordinator to be AWARE of, the coordinator can perform basic operations (new keygen, renaming, etc.).
3. For a device to become useful with a new coordinator, the coordinator must become AWARE of a key of which the device holds a share.

## Potential Annoyance/Pitfall

One potential limitation with this design is that to create a new key across a set of devices with a new coordinator, the coordinator must first learn of a key belonging to **each** device.

Example scenario: You have 5 Frostsnap devices and set up:

- A 2-of-3 threshold with three devices
- A 2-of-2 threshold with the other two devices

To make a new 3-of-5 with a new coordinator, you would need to share knowledge of both the 2-of-3 and 2-of-2 keys to the new coordinator.

## Coordinator Awareness of Keys

**Access**: Coordinators should disclose to devices which keys they are AWARE of, using a public key.

Devices will register the coordinator as AWARE of these keys for permission purposes.

Key points:

- Under trusted firmware, devices are less likely to collect information than phone coordinators
- Keygen results in a new key which the coordinator is AWARE of
- Once the coordinator reveals its awareness of keys, devices can disclose information about held shares:
  - For AWARE keys: disclose index and share image
  - For UNAWARE keys: provide a t-of-n key share and key fingerprint that progresses the coordinator towards becoming AWARE
    - Coordinator must visit `t` devices to become AWARE
    - Key names may be provided alongside shares

**Confirmation**: Device _could_ REQUIRE CONFIRMATION before informing the coordinator of its held key fingerprints & t-of-n recovery shares.

### Actions

| Action                 | Access Requirement for Coordinator  | On-Device Confirmation |
| ---------------------- | ----------------------------------- | ---------------------- |
| KeyGen Start           | AWARE of any key (or no keys exist) | No                     |
| KeyGen Finish          | AWARE (or no keys exist^^)          | Yes                    |
| Share Public Nonces    | AWARE or UNAWARE (limited)          | No                     |
| Signing                | AWARE of relevant key               | Yes                    |
| Display Backup         | AWARE of relevant key               | Yes                    |
| Check Backup           | AWARE                               | Yes (keyboard input)   |
| Restore Backup         | AWARE or UNAWARE                    | Yes                    |
| Device Data Wipe       | UNAWARE                             | Yes (CPU countdown)    |
| Factory Firmware Reset | AWARE of ALL keys                   | Yes (CPU countdown?)   |
| Delete Shares          | AWARE of relevant key               | Yes (long hold)        |
| Update Firmware        | AWARE of any key                    | Yes                    |
| Rename Device          | AWARE of any key                    | Yes                    |
| Device Configuration   | AWARE of ALL keys                   | Yes                    |

## Action Details

### Nonces

Sharing nonces during coordinator recovery enables two-round signing.
Though we do not want to allow unknown coordinators to request heaps of nonces under different identities to crash the devices.
Is this limit on nonces during key restoration enough to prevent storage overflow?
Perhaps the device could store any pending new-coordinator nonces in a separate location, only storing one at a time.

### Device Data Wipe

If someone evil finds your device, they could destroy or steal it. Allowing someone UNAWARE to wipe the data off the device doesn't give them much besides hard to notice deletion. For this reason the data wipe should delete all device data such that it is noticed.

This should require a long CPU countdown.
Perhaps the timer to delete could scale with the number of shares held.

## Device Configuration

In the future, we might want a way to semi-permanently apply settings to a device, such as:

- Disabled Actions
- Speak to anyone mode
- CPU timer multiplier
-

### Share Deletion

Idea: we could force users to write down & check backups if they want to do a deletion without a CPU countdown.

## Lost access to coordinator, don't have enough shares to recover any keys, but want to use the device again?!

If the user has any coordinator that knows about they key, or has enough devices/backups to recover a key, then they can use those methods to access the device.

Without these, the user will have to delete everything off the device via a device data wipe.

## Remote Coordinator Handling

- Coordinators do not reveal their known keys to peers.
- The coordinator making a reqeuest must demonstrate AWARENESS of a key.
- Lost coordinator recovery is possible through peers sharing AWARENESS.
- Copy/paste functionality for coordinator AWARENESS data would be desirable.

# Other Ideas

## Time Based Access

If the devices have a concept of time (powered / unpowered) we could render a new coordinator trusted after N hours.

## Sharable Coordinator Certificates

Devices could sign certificates which get saved _elsewhere_ (cloud?), to be loaded into a new authenticator to make it trusted.

Though it might be a simpler design to just share the awareness of keys, as above.
