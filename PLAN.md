# Frostsnap + Nostr

We want a way to sign transactions without plugging the devices sequentially into the same coordinator. 
Nostr relays should be ideal for this:

1. We don't need the messages between participants to last forever, just long enough to get a job done like sign a transaction
2. Nostr events can refer to previous events so we can let events in a chain refer to each other and the servers will index these references so we can look them up.


## Nostr channel set up

Each key -- actually I think each "access structure" can have its own chat room. 
This channel will be encrypted. The shared secret will be the `key_id`.
This will determine the `channel_id` as well (hash the `key_id` with `NOSTR_CHANNEL_ID`).
Each event related to this will be tagged with `h` for the `channel_id`.

To send a message to the channel, you'll publish an event under `h`, and generate a shared secret (hash the `key_id` with `NOSTR_CHANNEL_SHARED_SECRET`) and do a NIP44 style encryption using the shared secret (instead of using ECDH for the shared secret). The outer key for the event will be ephemeral. The inner encrypted one will be a proper signed event.

Note: that we'll come up with a different way to do an actual key generation using nostr -- first we're going to assume the key has been set up already.

## Basic messages

The inner events will form a NIP28 unencrypted channel.

## Signing protocol

Interspersed with the normal NIP28 messages will be FROST protocol messages.

The signing messages are as follows:

### `SignRequest`

This message includes a `SignTask` (see `/frostsnap_core/src/sign_task.rs`). It indicates the author would like carry out this sign task.

### `SignOffer`

This message is sent from a user who is willing to sign the message. It includes the `binonce`(s) the user will use to complete this sign task and which share index they will sign with (usually a single one). This is computed by their frostsnap app (by calling down to `frostsnap_coordinator` -> `frostsnap_core::FrostCoordinator`).

This event references the `SignRequest` with an `e` (reply) tag OR another `SignOffer` message (which in turn will reference the `SignRequest`). When you have `t` `SignOffer` messages (at different share indices) in a chain we call the final `SignOffer` message the *sealing* offer. Note that clients can respond to multiple chains of `SignOffer` messages but they MUST use different nonces in each chain. We don't have to implement multiple `SignOffer` chains right now -- just choose one -- but we can leave a TODO for this.

Note that so far no frostsnap device has needed to be engaged. The frostsnap coordinator (e.g. the user's phone or laptop) will have nonces for that 

### `SignPartial`

This is made by one of the `SignOffer` parties in reply to the `SignTakeOffer` message. It provides the partial signature(s) under the nonces they replied with. This references the *sealing* `SignOffer` message in a chain. 

Given all the `SignPartial` events anyone can construct the full signatures and complete the sign task.

## UI

The UI for this chat room + signing protocol coordination should be added to the flutter app. For now there will be a wallet setting called "remote signing" which takes you through the workflow to enable it. Note a wallet can either be in remote signing mode or in "personal" mode. Right now the default is personal mode but we need a way to change it to "remote signing". For now you can only take a personal wallet and convert it to a remote signing wallet. In remote signing mode there is a chat button between the send and receive buttons. It will have a badge with the number of unread messages in the chat.

The user has not already set their nsec they will be encouraged to set one or generate one randomly. Before that they will be prompted about whether they want to enable "remote signing" for this wallet. 

After they have enabled it their app will subscribe to the nostr channel id with tag `h`. If they find that there are no events they will initialize the chat channel with the first NIP28 channel creation event.

From the chat/remote signing page they have a chat room interface where they can see who's in there (using typical nostr account info to populate the list).

They also have access to the "send" button too which brings up the typical send flow but you *don't* choose which devices are going to sign. This will create the `SignRequest` message in the channel which will be styled with emphasis. This message is interactive. Pressing it brings up a dialog where you can choose which device you will sign with. 

When you submit to sign with a device it will send a `SignOffer` event. This will progress the status of the `SignRequest` message with a small circular progress this with like "2/3" which means we've received 2 out of a necessary 3 `SignOffer`. A small de-emphasised text message indicating an event will also appear in the chat "<the user> has offered to sign the transaction". This progress is based on the longest chain of `SignOffer` messages.

Once there's `t` `SignOffer` messages in a chain, the clients for the users in that chain have a "sign tx" button appear in the chat which looks like a "reply" to a previous message. There should be a button in the message and probably somewhere fixed in the ui. This brings up a dialog that prompt to plug in the correct device to produce the partial signature (entering the usual fullscreen dialog). Then once it's signed, it posts the `SignPartial` replying to the sealing offer. The `SignPartial` messages progress the circular progress indicator in signing prompt message.

Once `t` valid `SignPartial`s are collected referring to the sealing `SignOffer`, a button appears in the chat in reply to the message with the sign button with a prompt to view and the transaction details and broadcast it.


## Implementation notes


- Each user has a frostsnap app which has the `frostsnap_core::coordinator::FrostCoordinator` running in it.
- There isn't an API yet for doing this sort of signing so you'll need to build those into `FrostCoordinator`. To start the signing all we need is a `SignTask` and to say which `access_structure_id` we want to use. We don't really need an API to develop those.
- The issue will be an API to get the `binonce`s for a particular device and reserve it to carry out the signing task. This will need a new API to get the nonce stream and lock it. Right now nonce streams are locked by the existance of signing sessions. This will need some kind of explicit lock on a stream which returns some nonces too. You won't be able to create a sign session at this point and so we need another mechanism. This will need new mutations in the coordinator signing module.
- Then to finally tell the device to sign the task with the particular nonces we will need a coordinator API to generate the message to send. Once you get back the partial signature the coordinator can remove the lock from the device's nonce stream. 

## Implementation plan

1. Make a crate frostsnap_nostr where all the nostr code will live including the nostr client that understands the channel logic and encryption. Also modelling the chat state will be done here.
2. Make a simple binary in there to start up a nostr relay for testing. You can use `nostr_relay_builder` to do this. 
3. let's work on the nostr chat room interface and turning the wallet into a "remote signing" mode. It won't be able to do signing but every user should be to convert their wallet to a remote signing wallet and join the chat. This involves:
  - Saving the config using the existing app config system. The config logic should probably live in `frostsnap_nostr` but I don't think from the app the API should distinct -- the api calls from flutter can all go on the existing context things like the `coord` coordinator.
  - getting the encryption and deterministic channel creation working where everything is derived from the `key_id`
  - being able to send and receive chat messages.
4. Then we'll work on `frostsnap_core` and the APIs and the actual signing feature. 
