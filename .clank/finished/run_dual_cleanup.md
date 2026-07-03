# run_dual_cleanup
# Ctrl-C on `just run-dual` tears down both app instances

`just run-dual` (justfile ~189) launches instance B directly as a
background binary (`"$BG_BINARY" --data-dir=~/tmp/frostsnap-b &`)
and then `exec`s `flutter run` for instance A in the foreground.
Ctrl-C stops `flutter run` (which tears down A), but B is orphaned:
the `exec` replaced the recipe shell, so no trap can ever run and
nothing owns B's PID anymore. Every dual session leaks a Frostsnap
instance; repeated runs accumulate them and stale instances hold
the `frostsnap-b` data dir.

## Fix

Restructure the dual branch of the recipe around a trap:

- Drop the `exec` on the dual path (a trap needs the shell to
  survive `flutter run`). Keep `exec` on the single-instance
  `a`/`b` paths — nothing to clean there.
- Record B's PID; `trap cleanup EXIT INT TERM` where `cleanup`:
  1. kills `$BG_PID` if still alive;
  2. belt-and-braces sweeps strays from previous leaked sessions:
     `pkill -f -- "--data-dir=$HOME/tmp/frostsnap-a"` and `-b`
     equivalents — matching ONLY on the data-dir argument so
     nothing unrelated can be hit (this also reaps an instance A
     that a killed `flutter run` orphaned).
- Trap on EXIT (not just INT) so quitting `flutter run` normally
  (`q`) also reaps B.
- Cleanup is process-only: the `~/tmp/frostsnap-{a,b}` data dirs
  are deliberately persistent state (the wallets under test) and
  are NOT deleted.

## Acceptance

- `just run-dual`, wait for both windows, Ctrl-C → `pgrep -f
  "data-dir=$HOME/tmp/frostsnap"` finds nothing.
- Same after quitting `flutter run` with `q`.
- `just run-dual a` / `b` single-instance behavior unchanged.
- A pre-existing leaked instance from before this fix is reaped by
  the next `run-dual` session's cleanup sweep.
