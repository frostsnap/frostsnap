# simplify_channel_shutdown
# Simplify channel shutdown

Undo the "lift shutdown signal to outer `ChannelHandle`" refactor.
It was defense-in-depth against a memory-leak class we don't actually
need to defend against.

## Motivation

The `per_consumer_channel_runner` plan lifted the runner's
`tokio::sync::watch::Sender<bool>` up to the outer
`frostsnapp::api::nostr::ChannelHandle` so that `close()` and `Drop`
would fire the signal regardless of whether `start()` had finished
plumbing the receiver into the runner.

That fixed two theoretical leak paths:

1. Dart disposes the page mid-`start()`; `close()` finds `inner=None`
   and no-ops; the runner spins up seconds later and lives forever.
2. Handle drops without an explicit `close()` (GC path); no signal
   ever fires.

Both are real, but the blast radius is one leaked tokio task per
missed close per session. In exchange, the refactor forced two other
users of `ChannelRunner::run` (keygen `ProtocolHandle` and
`LobbyHandle`) to grow a `_shutdown_keepalive:
Arc<watch::Sender<bool>>` field whose sole purpose is "keep the watch
channel from closing while any handle clone lives." That's implicit
lifecycle logic encoded in a `_`-prefixed field and a comment — a
smell.

The contract we actually want is the plain one Dart already models
with `State.dispose`: **the caller owns the handle; they call
`close()` when done; forgetting is a leak, oh well.**

## Concretely

Revert commits `34e9b15` + `89a66c0` (behaviorally, not literally —
some of the changes are wound through subsequent commits, so this is
a fresh edit pass).

Keep everything the `per_consumer_channel_runner` plan established
for lazy-start (`403137c`) — that fixed a real correctness bug
(lost startup events), not a leak.

### frostsnap_nostr

- `frostsnap_nostr/src/channel_runner.rs`:
  - `ChannelRunner::run` creates its own `(shutdown_tx, shutdown_rx)
    = tokio::sync::watch::channel(false)` internally again. Drop the
    `shutdown_rx: watch::Receiver<bool>` parameter.
  - `ChannelRunnerHandle` gets `shutdown_tx:
    tokio::sync::watch::Sender<bool>` back as a field.
  - Restore `ChannelRunnerHandle::shutdown()` that calls
    `self.shutdown_tx.send(true)`. Don't re-add `is_shutdown()` —
    it was already dead before the lift.
- `frostsnap_nostr/src/signing/mod.rs`:
  - `ChannelClient::run` drops the `shutdown_rx` parameter; the
    inner `ChannelRunner::run` call reverts to two-arg form.
- `frostsnap_nostr/src/keygen/protocol.rs`:
  - Remove the `_shutdown_keepalive:
    Arc<tokio::sync::watch::Sender<bool>>` field on `ProtocolHandle`.
  - Drop the local `watch::channel(false)` creation; call
    `runner.run(client)` two-arg again.
- `frostsnap_nostr/src/keygen/lobby.rs`:
  - Same on `LobbyHandle`: drop `_shutdown_keepalive`, drop the local
    channel creation, revert to `runner.run(client)`.
- `frostsnap_nostr/tests/signing_live.rs`:
  - Drop `_shutdown_tx` from `NostrSide`; revert the call to
    `channel_client.run(client, sink)` two-arg.

### frostsnapp

- `frostsnapp/rust/src/api/nostr/mod.rs`:
  - Remove `shutdown_tx: tokio::sync::watch::Sender<bool>` from
    `ChannelHandle`.
  - Remove `impl Drop for ChannelHandle`.
  - `close()` reverts to:

    ```rust
    #[frb(sync)]
    pub fn close(&self) {
        if let Some(inner) = self.inner.get() {
            inner.runner_handle.shutdown();
        }
    }
    ```

  - `NostrClient::connect_to_channel` no longer creates a
    `watch::channel`; the handle struct literal loses the
    `shutdown_tx` field.
  - `ChannelHandle::start` drops the `self.shutdown_tx.subscribe()`
    argument in the `cc.run(...)` call.

## What we explicitly accept

- Close-during-start race: if Dart disposes while
  `handle.start().await` is in flight, `close()` returns a no-op
  because `inner` isn't set yet. When `start()` finishes, the runner
  is live and unowned. It runs until process exit. That's one leaked
  task per race. Acceptable — we're not running in a long-lived
  server where the accumulation matters, and the race window is
  short.
- No `Drop` safety net: if the Flutter side forgets to call
  `handle.close()` in `dispose`, same leak. The idiom is well-known;
  we trust the caller. Reviewers should flag missing `close()` in
  new code the same way they flag missing `subscription.cancel()`.

## Drop-of-outer still cleans up the runner (happy path)

To pre-empt "does Dart-GC of the outer handle actually tear down the
runner?": yes, via a reference-count cascade, not via an explicit
`send(true)`.

Outer `ChannelHandle` drops → `inner: OnceCell<InnerChannelHandle>`
drops → its `ChannelRunnerHandle` drops → the last clone of
`shutdown_tx: watch::Sender<bool>` drops → the runner task's
`shutdown_rx.changed().await` returns `Err(_)` (the "channel closed"
path in the runner's `select!`) → runner breaks its loop, unsubscribes,
exits. Same shape as pre-`per_consumer_channel_runner`.

The only case that leaks is the "runner spawned after `close()` was
called" race in the previous section — in that race no `close()` is
coming (already returned no-op) and the outer isn't dropped yet
either (caller is still holding the handle waiting for `start()` to
return), so neither cleanup path fires.

## Verification

- `cargo check -p frostsnap_nostr` and `-p rust_lib_frostsnapp` clean.
- `cargo test -p frostsnap_nostr --tests --no-run` clean (was a real
  breakage codex caught last round).
- `flutter analyze lib` clean.
- Grep confirms no remaining `_shutdown_keepalive` references.
- The plan's original per-consumer semantics still hold: each
  `connect_to_channel` gets its own runner + relay subscription;
  `close()` tears them down.

## Out of scope

- Revisiting `is_shutdown()` — it was dead before, stays deleted.
- Fixing the lazy-start ergonomics (forget `await handle.start()` →
  silent stall). That's a real footgun but a separate concern; would
  need FRB support for returning `(handle, stream)` atomically.
- Explicit `close()` semantics for keygen `ProtocolHandle` /
  `LobbyHandle`. Left riding on drop-of-last-clone → last-Sender-
  dropped → channel-closed → runner exits (the same behavior that
  existed before the lift). If keygen ever needs deterministic
  teardown, that's its own plan.
