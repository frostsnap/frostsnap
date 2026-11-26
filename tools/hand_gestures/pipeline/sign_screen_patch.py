"""Patches a checkout at 450fefa40 the way the 30 Oct sign-screen recording ran.

usage: sign_screen_patch.py <checkout>

Run it only on a disposable worktree: it edits tracked files in place.
"""

import sys
from pathlib import Path

root = Path(sys.argv[1])

simulator = root / "tools/widget_simulator/src/main.rs"
s = simulator.read_text()
# The simulator's touch marker would be recorded into the screens the hand is composited over.
marker = "touch_feedback.push((point, 30));"
assert s.count(marker) == 1, marker
simulator.write_text(s.replace(marker, "let _ = point;"))

demo = root / "frostsnap_widgets/src/demo_widget.rs"
s = demo.read_text()
start = "recipients: $crate::alloc::vec![\n                        recipient(p2tr_address.clone(), 100_000),"
end = "fee_rate_sats_per_vbyte: Some(12.5)"
assert s.count(start) == 1 and s.count(end) == 1
a = s.index(start)
b = s.index(end) + len(end)
s = (
    s[:a]
    + "recipients: $crate::alloc::vec![recipient(p2tr_address.clone(), 250_284)],\n"
    + "                    fee: bitcoin::Amount::from_sat(284),\n"
    + "                    fee_rate_sats_per_vbyte: Some(2.3)"
    + s[b:]
)
demo.write_text(s)
