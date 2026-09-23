# Genuine check

The app tells the user whether each connected device is genuine Frostsnap hardware, and shows its
case colour. The check is cosmetic: it runs in the background and never blocks or delays anything a
user does with a device.

## What a genuine device has

- A **factory certificate**: a factory Schnorr signature over the device's DS (RSA-3072) public
  key, case colour, revision, serial and timestamp (`genuine_certificate::Certificate`).
- A **DS key** held by the ESP32 Digital Signature peripheral. It can sign, but it cannot be read
  out.
- A **device-id key**: the secret behind its `DeviceId`, protected by eFuse.

## Assumption and limit

**Assumption:** a device-id key cannot be extracted or cloned. #522 relies on this too.

The DS hardware proves once that it vouches for a device id (the attestation below). After that, a
device proves it is genuine in each session by signing with its id key. The DS key is **not**
re-proven every session: holding the id key stands in for it.

**Limit:** if an id key were extracted, the attacker would also need that device's attestation,
which is a public blob. Anyone who has ever talked to the device can capture it, because PKCS#1 v1.5
signatures are deterministic. With both, any hardware would pass as that device from then on. #522
re-ran the DS key every session, so a clone also needed the DS peripheral. We accept this because
of the assumption.

Someone could claim a genuine device's id and relay our challenge to the real device. The check
then proves the real device's key is present, and that key is exactly what signing trusts.

## State machine

Each device is in one of three states:

| State | Meaning | Leaves on |
|---|---|---|
| 1. `Unattested` | No certificate on file for this id | a verified attestation → 2 |
| 2. `Attested(cert)` | Certificate on file; nothing proven this session | a verified identity proof → 3 |
| 3. `Genuine(cert)` | Proven this session | disconnect → 2 |

- A failed attestation or identity proof is **ignored**. The device stays where it was, nothing
  persisted changes, and nothing is flagged. There is no Failed state: #522's `Failed` and
  `FirmwareTooOld` are removed.
- A device with a certificate on file goes 2 → 3 with one query and no DS signature. A device with
  none goes 1 → 2 → 3 with two queries: after the attestation verifies, the challenge is sent
  straight away.
- Firmware without `FirmwareFeatures::genuine_check` is sent neither request, so it can't advance:
  it stays in state 1, or in state 2 if a certificate is already on file. A device can have one and
  still lack the feature, for example after a downgrade. The status the app sees carries whether
  the firmware supports the check (see below), so the app can say why the device isn't advancing.
- A build with no factory key compiled in (`cfg(genuine_cert_key)` unset) sends no requests at all.

### Module

`frostsnap_coordinator/src/genuine_check.rs` holds `GenuineCheck`. It is not a `UiProtocol`:
`UiProtocol`s are flows a user starts, and they take a device over. It owns:

- the certificates on file (`GenuineCerts`, persisted, described below);
- each connected device's session state, plus the one request outstanding to it (`Attestation`, or
  `Identity(challenge)`);
- the factory key;
- the firmware-feature gate;
- verification, through the `frostsnap_comms::genuine_certificate` functions.

Its interface has no USB in it:

```rust
fn connected(&mut self, id: DeviceId, firmware: FirmwareVersion) -> Vec<CoordinatorSendMessage>;
fn disconnected(&mut self, id: DeviceId);
fn recv(&mut self, from: DeviceId, msg: GenuineResponse) -> Vec<CoordinatorSendMessage>;
fn status(&self, id: DeviceId) -> GenuineStatus;  // + a drained change list
```

`usb_serial_manager` calls `connected` on every `Announce` and `disconnected` on every path that
removes a device (port loss and downstream disconnect). It hands the two response variants to
`recv`, sends whatever comes back, and turns status changes into `DeviceChange::Genuine { id,
status }`. It holds no genuine-check state and does no crypto.

#522 has `challenges` and verification in `usb_serial_manager`, and record-keeping in the app's
`coordinator.rs`. All of that moves into this module.

Two rules keep the outstanding request honest:

- **A response is accepted only while its request is outstanding.** Accepting it clears the
  request whether or not it verifies. An unsolicited or second answer is dropped, so a device can't
  replace a verdict with an extra answer.
- **A re-announce while a request is outstanding does not issue a new one.** This is #522's rule: a
  device re-announces when its connection re-handshakes, and it is still answering the first
  challenge.

A failed identity proof leaves the device in state 2 until the next announce or reconnect, which
sends a fresh challenge.

## Wire

All new variants are **appended**, so no existing discriminant moves.

Coordinator → device (`CoordinatorSendBody`, which is `EncapsV0`-encoded):

- `RequestGenuineAttestation`: no payload.
- `GenuineIdentityChallenge(Box<GenuineChallenge>)`: 32 fresh random bytes per connection.

Device → coordinator (`DeviceSendBody`):

- `GenuineAttestation { certificate: Box<Certificate>, ds_signature: Box<[u8; 384]> }`. The DS key
  signs `SHA256(ATTESTATION_TAG ‖ device_id)` with a new tag, `b"frostsnap-genuine-attest-v1"`.
  This is a new message, not #522's bound proof: that RSA signature covers a challenge, so it
  can't be persisted and re-checked later.
- `GenuineIdentityProof { signature: Signature }`, from `sign_identity_challenge`: a Schnorr
  signature under the id key over the challenge, tagged `GENUINE_IDENTITY_MESSAGE_TAG`.

The coordinator verifies against the connection's `from`, never against an id in the message body:

- attestation: `verify_certificate_detailed`, then `verify_attestation(body, from, ds_signature)`;
- identity proof: `verify_identity(from, challenge, signature)`.

A certificate whose DS signature covers a different id fails.

### Retiring the old challenge

- **Released firmware (≤ 0.4.0)** answers `Challenge` with `SignedChallenge`: an unbound RSA
  signature over the bare challenge, which can be relayed. `Challenge` becomes `_LegacyChallenge`
  and `SignedChallenge` becomes `_LegacySignedChallenge`. Both keep their positions, and the
  coordinator never sends the former. New firmware ignores `_LegacyChallenge`, so it no longer
  signs coordinator-chosen bytes with the DS key.
- **#522's `GenuineProof`** and `genuine_challenge_message` are dropped. They never shipped, so no
  discriminant needs preserving.
- **Old firmware sent a new request:** `WireCoordinatorSendBody::decode` fails on the unknown
  `EncapsV0` variant and returns `None`, and the device ignores it. The feature gate means we never
  rely on that.
- **The factory genuine check** (`frostsnap_factory/src/genuine_check.rs`) switches to the two new
  requests. It verifies both, which exercises the DS peripheral on the freshly flashed firmware.

### Feature gate

In `frostsnap_comms/src/firmware_version.rs`:

```rust
genuine_check: *self > V0_4_0
```

v0.4.0 is already tagged and registered in `KNOWN_FIRMWARE_VERSIONS`, and it predates these
requests. The first release after it is the first to ship them. Tests pin the boundary: v0.4.0 and
earlier are false, and an unregistered digest is true.
`FirmwareFeatures::all()` sets it, so a dev build with an unknown digest supports it.
`GenuineCheck::connected` checks `firmware.features().genuine_check`, as `check_backup.rs:46` does.

## Persistence

`GenuineCerts` replaces #522's `GenuineDeviceInfo`/`fs_genuine_devices` (colour, serial and revision
as text). #522 never shipped, so nothing needs migrating. `GenuineCerts` is a new table in
`frostsnap_persist.rs` next to `DeviceNames`:

```sql
CREATE TABLE fs_genuine_certs (id BLOB PRIMARY KEY, certificate BLOB NOT NULL, ds_signature BLOB NOT NULL)
```

- **What is stored.** A row holds the whole attestation, not facts taken from it, so the row stays
  evidence. On load, each row is decoded and **re-verified** against this build's factory key: one
  RSA verify per device, once per launch. A row that fails to decode or verify is skipped, and
  that device is back in state 1. This covers a dev build reading a prod database, or the reverse.
  A corrupt row can't stop the app starting. When the device next attests, `INSERT OR REPLACE`
  overwrites the row.
- **Only a verified attestation writes a row.** Nothing deletes one: a failed proof changes
  nothing.
- **How it is written.** The app keeps the db lock off `poll_ports`, so `GenuineCheck` stages
  inserts (`TakeStaged`). The app's poll loop takes the db lock afterwards and persists what was
  staged, as it does for other stores. If the app dies before that write, the device attests again
  next time. That costs one extra DS signature and is harmless.
- **How state 2 is found on connect.** `connected(id, …)` looks `id` up in the in-memory
  `GenuineCerts`. A hit is state 2 and sends the challenge. A miss is state 1 and requests the
  attestation, if the feature gate allows.
- **Disconnect** drops the session state and any outstanding request. The certificate stays, so the
  device is in state 2 when it comes back.

## App

`ConnectedDevice.genuine: GenuineStatus` replaces #522's `Unknown / Genuine / Failed /
FirmwareTooOld`:

```rust
enum GenuineStatus {
    Unattested { firmware_supports_check: bool },  // state 1
    Attested { firmware_supports_check: bool },    // state 2
    Genuine,                                        // state 3
}
```

`Genuine` needs no flag, because a device only reaches it by answering the new request.

- **Case colour comes from the certificate on file**, which the factory has signed. It is shown in
  states 2 and 3, and for disconnected devices through `get_device_case_color`.
- **#522's colour from unverified certificates is gone**, and with it the `DeviceChange::CaseColor`
  event. As a result, a device on firmware without the feature shows no colour until it is upgraded,
  unless a certificate is already on file for it.
- **Badges:**
  - state 3: "Genuine";
  - state 1 or 2 with `firmware_supports_check == false`: "Update firmware to check" (state 2
    still shows the colour);
  - state 2 otherwise: neutral (colour shown, check in progress);
  - state 1 otherwise: neutral.
- Nothing reads as a warning, because a failure is never shown.
- `genuine_check_enabled()` stays. It is false when no factory key is compiled in, and the UI then
  shows no badges.

## Cost and non-blocking

- **No waiting.** Both requests go through the normal outbox, and nothing waits for their answers.
  No `UiProtocol`, device-list entry or signing flow is gated on `GenuineStatus`.
- **DS signing is rare.** The device makes one DS signature per coordinator, the first time that
  coordinator sees it. After that, each session costs one Schnorr signature. #522 made a DS
  signature on every connection.
- **Frontier stack.** The frontier stack budget is tight (about 24.5% of 25%). The handlers box
  their 384-byte buffers, as #522 does, and `just stack-check` is part of done.
