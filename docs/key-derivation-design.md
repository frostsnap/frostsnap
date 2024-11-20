# Key derivation and recovery

## Design

### Requirements

Frostsnap devices do not handle authentication of the user directly unlike other devices with on-device pin codes.
Coordinating devices (e.g., the user's phone) are responsible for authenticating the user, and the devices authenticate this coordinator.
The purpose of this document is to walk through the implication of this design for key derivation and recovery.

Naturally two questions immediately follow:

1. How can devices authenticate a coordinator?
2. How does one authorise a coordinator in the first place?

Consider the following requirements. A user must be able to:

1. Fully restore their signing capabilities from physical share backups alone.
2. Authorise a new coordinating device if they have a threshold of devices with valid shares for a key.

This implies that we cannot require a coordinator generated authentication key for a new coordinator to gain access. The state of a valid coordinator must be restorable by connecting devices with valid shares on them. Specifically, no more or less than `t` valid shares should be required to restore a coordinator.

### Approach

Since `t` valid shares are required to authorise a coordinator there is a natural credential that a coordinator can present to demonstrate they've talked to devices with `t` valid shares: the root polynomial image.
This is the polynomial the coordinator uses to verify signature shares under that access structure and can be reconstructed from `t` valid share images.

Therefore we make each device present its share image(s) when it connects to a coordinator. Once a coordinator has `t` of them it can reconstruct the root polynomial image and from then on it can present some kind of credential derived from this to authorise itself as a coordinator to request signatures under that access structure.

It's important to understand that we are not making requesting signatures more restrictive than it needs to be. One could ask, "Why not just let any old coordinator request signatures? If they don't have access to enough devices they won't be able to finish the signature anyway".
The problem with this (very natural) approach is the coordinator would still need to be told the root public key so it can create and verify the signing session anyway.
Having a single device provide the coordinator all this information at this stage would mean in the context of Bitcoin, you would have revealed the wallet's XPUB and thus transaction history to anyone who happens upon a single frostsnap device (without some very awkward designs to keep the XPUB secret).
Furthermore it's unlikely they could start a signing session yet since the coordinator presumably doesn't have nonces from the other devices yet. If they had nonces they could just as easily have acquired share images from the other devices too so we're back at our original design.

Since the root polynomial is a universal credential that only authenticated coordinators have and is independent of any specific coordinator we can also use it to encrypt shares on each device.
Devices can forget about the root polynomial after key generation but before they do, encrypt their new key share with a key derived from it.

Now an attacker who finds a device learns nothing from it other than a share image for each share the device has. This tells them only how many keys that devices was involved with and their threshold `t` (and by the index a little bit more info about `n`). The attacker can only start requesting signatures after they discover `t` devices which satisfies our security assumption.

### Concrete key derivations

Here's how keys are derived. Remember `rootkey` is the first coefficient of the root polynomial generated during key generation.

```
rootkey (m) // rootkey includes a chain code of 32 zeros
└── appkey (m/0)
    | // Descriptors/PSBT will treat this as the root for everything below
    ├── Bitcoin (m/0/0)
    │   └── Segwitv1 (m/0/0/0) // the account kind
    │       └── Account Index (m/0/0/0/n) // n is the index for the account
    │           ├── External keychain (m/0/0/0/n/0)
    │           └── Internal keychain (m/0/0/0/n/1)
    ├── TestMessage (m/0/1)
    └── Nostr (m/0/2)
```

Recall that the devices delete `rootkey` soon after they receive it during keygen and don't store any public keys at all.
The coordinator will provide the `rootkey` along with any signing request along with something derived from the root polynomial to decrypt their signature shares. The `rootkey` itself doesn't help decrypt by itself since it doesn't demonstrate knowledge of the entire access structure. The devices can verify the `rootkey` is correct since they have a hash of it.
With the `rootkey` the devices can derive all the public keys involved in the signing request and verify that everything matches expectations (e.g. input public keys are at the right derivation paths).

### Coordinator encryption

Coordinators should encrypt the `rootkey` on their side such that the user must authenticate themselves to the app in order to explicitly decrypt and use it. Remember the root _polynomial_ is what gives you the ability to request signatures but with the `rootkey` and the rest of the polynomial coefficients you can construct it. The "`app_poly`" is on the coordinator unencrypted -- this is the root polynomial with the `rootkey` tweaked to become the `appkey`. Once you have the `rootkey` you can restore the `root_poly` by replacing it.

Keep in mind with the `appkey` (an XPUB) the coordinator can derive any public key in the tree below it which means all the wallet addresses. Coordinator applications may want to keep `appkey` as secret as possible and require authentication before using it but the right approach is probably just to encrypt the whole application database. With the `app_poly` the coordinator can always verify signatures without decrypting the root key.

[^1]:
    The reader may have been thinking "a signature!" since `t` valid shares can produce a signature. But then the coordinator would have to collect nonces from the devices first before she begins asking the devices to sign her own access credential.
    If we were using BLS signatures (no nonces) then signing a credential would make more sense for authentication.
