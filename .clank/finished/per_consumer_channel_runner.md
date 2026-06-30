# per_consumer_channel_runner
# Per-consumer channel runner: drop the shared `channels` map

## The bug

Chat messages stop appearing in the chat UI after navigating
around. Restart fixes it. Relay-pool log shows the same
encrypted outer event arriving on multiple subscription ids —
multiple runners are alive for the same channel.

## The bug class, mechanically

`NostrClient::connect_to_channel(params, sink)` returns
nothing. The created `ChannelHandle` is stored only in
`self.channels[asid]`. The caller never receives it.

When a second `connect_to_channel` for the same asid runs, the
new handle is inserted into the same slot, overwriting the
first. The first handle's reference count goes to zero. It
drops. Drop tears down the runner. The first caller's sink
stops getting events. The Dart consumer's stream goes silent
without any error or close signal.

There's no API surface for the caller to even hold the handle
themselves and prevent this.

## The fix

Each `connect_to_channel` call gets its own complete vertical
stack: its own runner, its own subscription, its own state,
its own sink. Nothing shared between consumers above the
shared `nostr_sdk::Client` (which manages the WebSocket
connection pool — that's fine to share).

### FRB constraint

`flutter_rust_bridge` cannot have one function that BOTH
returns a value AND takes a `StreamSink` parameter (the latter
becomes the Dart `Stream` return). So we can't have
`connect_to_channel(params, sink) -> ChannelHandle` directly.

Use the existing `broadcast_handle!` macro pattern for the
event-stream surface. The macro only accepts a single-field
tuple struct wrapping `Broadcast<T>` / `BehaviorBroadcast<T>`
and generates methods that operate on `self.0` as that
broadcast — it CAN'T generate `watch()` for a struct that
also contains the runner + send state. So we compose:

- A macro-generated leaf for the stream surface.
- An opaque "channel handle" that holds the leaf AND the
  runner-handle, exposes both `events()` (returns the leaf)
  and `sendMessage(...)` / etc.

### Concretely

1. `connect_to_channel(params) -> Result<ChannelHandle>` —
   creates a fresh runner. **No `StreamSink` param.** Returns
   the opaque handle.

2. **Leaf**: macro-generated, holds the event broadcast.
   ```rust
   broadcast_handle! { pub struct ChannelEventBcast(pub Broadcast<ChannelEvent>); }
   ```
   The runner pushes events into this broadcast. Dart-side gets
   `Stream<ChannelEvent> watch()` for free.

3. **Handle**: a manual opaque struct that composes leaf +
   runner + key_context.
   ```rust
   #[frb(opaque)]
   pub struct ChannelHandle {
       events: ChannelEventBcast,            // macro leaf
       runner: frostsnap_nostr::signing::ChannelHandle,
                                             // upstream handle for sends
       key_context: KeyContext,              // for seal_round_confirmed
   }

   impl ChannelHandle {
       #[frb(sync)]
       pub fn events(&self) -> ChannelEventBcast {
           self.events.clone()
       }

       // ALL the per-asid APIs that today route through
       // NostrClient.channels[asid] move here. Each call routes
       // through `self.runner` (no more get_handle lookup).
       pub async fn send_message(...) -> Result<EventId> { ... }
       pub async fn send_receive_address(...) -> Result<EventId> { ... }
       pub async fn send_sign_request(...) -> Result<EventId> { ... }
       pub async fn send_test_sign_request(...) -> Result<EventId> { ... }
       pub async fn send_sign_offer(...) -> Result<EventId> { ... }
       pub async fn send_sign_partial(...) -> Result<EventId> { ... }
       pub async fn send_sign_cancel(...) -> Result<EventId> { ... }
       pub async fn notify_tx_observed(...) -> Result<()> { ... }
       pub async fn publish_profile(...) -> Result<Option<EventId>> { ... }
       #[frb(sync)]
       pub fn seal_round_confirmed(...) -> Result<SealedSigningData> {
           // uses self.key_context, no longer NostrClient.key_contexts
       }
   }
   ```

4. **Delete `NostrClient.channels` AND `NostrClient.key_contexts`.**
   Every API that read from those maps moves to `ChannelHandle`.
   `NostrClient.disconnect_channel(asid)` also goes.

5. **Explicit teardown via `handle.close()`.** FRB opaque
   finalization (Drop-on-GC) is non-deterministic — relying on
   it for "stop the relay subscription when chat_page disposes"
   would leak subscriptions for unknown durations. Add an
   explicit
   ```rust
   impl ChannelHandle {
       /// Begin shutdown immediately and return. The actual
       /// teardown (signal shutdown → wrapper/runner loops exit
       /// → relay unsubscribe) runs in a spawned tokio task so
       /// callers don't need to await. Idempotent: calling
       /// close() twice is harmless.
       pub fn close(&self);
   }
   ```
   Fire-and-forget (synchronous, returns `()` immediately) so
   Flutter `State.dispose()` — which cannot be `async` — can
   call it directly.

   **Shutdown signal MUST NOT be the normal bounded command
   channel.** That channel can be full (chat under load), in
   which case `try_send(Shutdown)` fails silently and the
   runner never exits — recreating the leak this plan is
   meant to fix. Use a dedicated reliable signal:
   `tokio_util::sync::CancellationToken` (or
   `tokio::sync::watch<bool>`, or `tokio::sync::Notify`) held
   on the handle. `close()` calls `token.cancel()` —
   infallible, idempotent, doesn't block. Both the wrapper
   and runner loops `select!` on the token alongside their
   normal channels and break as soon as it fires.

   `close()` then spawns a tokio task that awaits the
   wrapper+runner exit (e.g. via a "done" channel they fire
   on the way out) with a short timeout, then calls
   `client.unsubscribe(channel_sub_id)` on the relay pool.

   Drop remains as a safety net: it also calls
   `token.cancel()` (infallible, same code path), and logs
   `tracing::warn!` if `close()` wasn't called first. Drop
   doesn't spawn the unsubscribe task — that's the
   `close()`-only guarantee — but the runner still exits, so
   the subscription is at least closed on the local-tokio
   side.

6. **NostrClient surface still owns**: the `Client` (relay
   pool), identity / publish credentials, lobby APIs,
   `fetch_profile_for_import`, etc. None of those are
   per-channel; they stay.

### Dart usage shape

```dart
final handle = await client.connectToChannel(params: params);
final stream = handle.events().watch();
_subscription = stream.listen(_handleEvent);
// sends:
await handle.sendMessage(content: '...');
// dispose (Flutter State.dispose, NOT async):
_subscription?.cancel();   // detach the sink (synchronous via SinkRegistrationId)
handle.close();            // fire-and-forget: shutdown runner + unsubscribe relay
// Drop is a safety net for the GC path; close() is the deterministic one.
```

## What's shared, what isn't

| Layer | Shared? |
|---|---|
| WebSocket pool (`nostr_sdk::Client`) | Yes (one per app) |
| `ChannelRunner` | Per consumer |
| Subscription on relay | Per consumer |
| `ChannelState` (members, creation event) | Per consumer |
| `SigningEventTree`, `ActivityState`, timers | Per consumer |
| `ChannelHandle` | Per consumer (caller owns it) |

For typical usage (one chat_page + brief post-keygen verify),
this means at most 2 runners briefly, settling to 1.

## What it costs

- N subscriptions per channel on the relay (relay sends N
  copies of each event).
- N copies of channel state in memory (kilobytes scale).
- N decrypts per peer event (NIP-44 is fast).

All bounded by the small number of concurrent consumers. Not
a real cost in practice.

## What goes away

- The clobbering bug class entirely.
- The shared `channels` map and its lifecycle management.
- All the "idempotency" discussion: each connect IS
  independent; nothing to dedupe.
- The "what does a late attacher see" discussion: there are
  no late attachers; each consumer cold-starts and replays
  from lmdb.
- The "multi-sink fanout vs single consumer" discussion: each
  consumer has their own runner with one sink.
- The post-keygen "verify" needing a stream-less helper: it
  just gets its own runner+stream like anyone else, drops it
  when done.

## Files

### Rust

- `frostsnapp/rust/src/api/nostr/mod.rs`:
  - `NostrClient::connect_to_channel` now returns
    `Result<ChannelHandle>` (FRB-opaque). The caller gets the
    handle.
  - Delete `self.channels` field, `get_handle`, and the
    `send_*` methods that route through it. Move sends onto
    `ChannelHandle`.
  - `disconnect_channel(asid)` goes away — `handle.close()`
    is the deterministic teardown path; Drop is the safety
    net.
- `frostsnap_nostr/src/signing/mod.rs`:
  - No FRB annotations here — `frostsnap_nostr` doesn't
    depend on FRB. The FRB-facing wrapper methods are on the
    new opaque `ChannelHandle` in
    `frostsnapp/rust/src/api/nostr/mod.rs` (see the
    "Concretely" section above); they delegate into
    `frostsnap_nostr::signing::ChannelHandle`'s existing send
    methods.
  - Fix the wrapper-task `runner_handle_for_task` clone so
    explicit `close()` (and Drop) trigger teardown reliably.
    Use the `CancellationToken` (or equivalent) described
    above as the shutdown signal — both wrapper and runner
    `select!` on it alongside their normal channels and break
    as soon as it fires. The clone chain stops mattering
    because shutdown is no longer reference-count-driven; the
    token is the source of truth.
    Either also remove the wrapper's `runner_handle_for_task`
    clone (read `state.members()` via a shared
    `Arc<Mutex<ChannelState>>` instead), or just leave it —
    once the token fires, both tasks break regardless of
    `cmd_tx` reference counts.
- `frostsnap_nostr/src/channel_runner.rs`:
  - On exit, `client.unsubscribe(channel_sub_id)` so relays
    drop the subscription.

### Dart

- `frostsnapp/lib/nostr_chat/chat_page.dart`:
  - Capture the handle: `_handle = await
    client.connectToChannel(params)`. Use `_handle.sendMessage(...)`
    everywhere instead of `_client.sendMessage(asid, ...)`.
  - `dispose()` calls `_subscription?.cancel()` then
    `_handle?.close()` (fire-and-forget, synchronous so it
    works inside Flutter's non-async `State.dispose`). Drop
    of the opaque handle reference is the safety net.
- `frostsnapp/lib/org_keygen_page.dart:1012`:
  - Same shape — get the handle, do the `firstWhere(ChannelState)`
    verify on its stream, drop the handle when done.
- `frostsnapp/lib/wallet_add.dart`: any other callers update
  to the new shape.

## Verification

- `cargo check -p frostsnap_nostr` and `-p rust_lib_frostsnapp`
  clean.
- `flutter analyze lib` clean.
- Manual:
  1. Two instances. Open chat on both. Send messages back
     and forth — they appear immediately on the peer.
  2. Trigger post-keygen flow → open chat → send messages.
     No regression.
  3. Navigate away from chat and back several times → still
     receives messages from peer each time. No accumulating
     relay subscriptions (check log for at most a brief blip
     of 2 subs as runners overlap during dispose+remount).
  4. Restart → fresh subscription, works as before.

## Out of scope

- Multi-consumer-of-one-runner (broadcast fanout, `broadcast_handle!`-style).
  Not needed — each consumer gets their own runner.
- Idempotent get-or-create. Not needed — every connect is a
  fresh runner.
- Cache-replay contract for re-attach. Not needed — there are
  no re-attaches; new consumers cold-start.
