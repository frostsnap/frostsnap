# The 64-word verifiable backup card (split64, format C′)

Status: experimental. No released firmware has it (`FirmwareFeatures::verifiable_card` is
`false` for every release digest), the encoding is a draft (see [What's draft](#whats-draft)),
and the phone-to-device link isn't encrypted yet, which the app says on its card intro pages.

This is a short design note for reviewers. The normative spec is Part 4 (PK.0 to PK.12) of
`two-screen-backup/spec_v3.md` in the team's design notes, outside this repo, and the owner's
decisions are logged in `split64-demo/DECISIONS.md` there. Labels like **B7** below are entries
in that log; **PK-On** are the spec's open items. Code comments also cite other sections of
those notes (`design §n`, spec `L1.x`, `L2.x` and `PK.x`, `build plan`, review finding ids,
`addendum`): each comment states its rule itself, and the label says where its reasoning was
recorded.

## 1. What the card is

Each key share gets a paper card of 64 BIP39 words instead of a 25-word backup. The device shows
words 1-32, the phone shows words 33-64, and neither screen ever shows more than 37 words of
the card. Before the card counts as checked, the phone verifies 5 of the device's words against
cryptographic commitments to the share, and the device quizzes the user on 5 of the phone's
words. So a lying device chip is caught with good probability, and a lying phone never sees
enough words to learn anything about the share.

**The code.** The card is a Reed-Solomon codeword RS[64, 61] over GF(2^11) (reduction polynomial
0x805): 2^11 = 2048 field elements, one per BIP39 word. Word w (1..64) is y_w = f(w) for a
polynomial f of degree at most 60. Three words are parity, so restore can correct one wrong
word, or fill unreadable ones, within 2e + f ≤ 3. With a target (the wallet's polynomial
F, or its share indices) the device also repairs two wrong words, with or without one
unreadable word (`MAX_REPAIR_WRONG`); the offline tool (§7) repairs up to three.

**The packed layout (C′).** f is defined by its 61 *values* at chosen points, not by its
coefficients:

| Points | Count | What they hold | Who sees them |
|---|---|---|---|
| x = 2024..2047 (hidden) | 24 | the share index i (8 bits) and the scalar s (256 bits): 264 bits = 24 × 11 | nobody; never a word |
| x = 2019..2023 (hidden) | 5 | random values: 4 masks, and at x = 2021 the threshold XOR a hash of s | nobody; never a word |
| words 33-64 (the phone half P) | 32 | random values (masks) | the phone |
| words 1-32 (the device half) | 32 | **computed**: f evaluated at w = 1..32 | the device |

The 37 random values (32 phone words + 5 hidden) are hashes of the share (the masks, §2), so the
card is deterministic. Any 37 words are a bijective image of the 37 random values for every
fixed secret, so **any 37 words reveal nothing** about the share; each screen sees exactly 37
(its own 32 plus 5 from the other half during the check). Hidden points sit above 2018, so no
u8 word index can ever evaluate f at one; the code asserts this.

The index is public and proof-bound (the phone knows it from its wallet record). The threshold
is warn-only (not bound by the proofs; restore never refuses on it). There are no version bits:
a layout descriptor string FMT_C is hashed into the seal and every mask, and restore finds the
format by trial decode confirmed by the masks.

## 2. Masks: 24 self-checking, 12 wallet-bound (PK-O1, decided)

The 36 masks (the 4 hidden random values other than the threshold, and the 32 phone words) are
derived from the share:

- **24 self-checking masks** (x = 2019, 2020, 2022, 2023 and words 33-52): hash of the layout
  descriptor L, s, i and a counter. Any restore can recompute them from the card alone. This
  264-bit check is what confirms a decode.
- **12 wallet-bound masks** (words 53-64): additionally hash the wallet's polynomial F, so an
  honest card mismatches a substituted F′ except with probability 2^-132.

The counter is the smallest value in 0..255 for which all 64 words are distinct. The first 50
are tried in constant time; a share that needs a later one (about 2^-33 of shares) is encoded in
variable time.

The proofs can't bind hash outputs, so a lying chip can write non-derived masks. That doesn't
change s or i (which are bound), but see B7 below for what it does to restore.

## 3. The seal and the bit proofs

Before any word is shown, the device computes the **seal**:

- 311 Pedersen commitments C_v = x_v·G + r_v·H, one per committed bit: the 256 scalar bits
  and the 55 bits of the 5 hidden random values. H is a fixed NUMS point.
- The blindings are derived deterministically from s and a context ctx (key id, access
  structure, index, S_i, threshold, FMT_C). One blinding (r_300) is **solved** so that the
  weighted scalar-bit commitments sum exactly to the share image: Σ_{t<256} 2^t·C_{v(t)} = S_i.
  The phone checks this group equation itself.
- A coin and its hash commitment coin_commit (for the coin toss, §4).
- **h_C** = TH("…/seal", ctx ‖ coin_commit ‖ hP ‖ C_0 ‖ … ‖ C_310), where hP is the hash of
  the phone half P. The seal is a deterministic function of (s, ctx).

The phone receives the commitments and P (44 bytes; P needs no proofs, it's checked against hP),
computes h_C itself from its own ctx, and **pins** it (persisted, never replaced) before the
first word is shown. Any later seal, commitment or P for that card that doesn't match the pin
is a hard failure, and the device is marked suspect (**B10**: an honest device can't produce a
second seal).

Then the device proves each commitment is to a bit: 311 stock CDS OR proofs (sigma_fun, pinned
at `=0.9.0` because it fixes the proof format), with h_C as the domain. "Pinned is not proven":
the phone treats nothing as verified until all 311 proofs verify, because the link equation
alone doesn't stop non-bit witnesses.

## 4. Openings, D and the coin toss

**Opening a device word.** Every device word is a public linear function of the 61 values, so
each of its 11 output bits is the XOR of a public subset T of the committed bits and a public
subset of the public bits (index and P). The device opens word j with value y by committing,
per output bit β, to the integer carry q_β = (Σ_{v∈T} x_v − y′_β)/2 in 7 bit commitments
Q_{β,l} with bit proofs, such that Σ_{v∈T} C_v − y′_β·G = Σ_l 2^{l+1}·Q_{β,l}. The phone checks
that group equation against the pinned seal. The largest parity set is 184, so carries fit in 7
bits: 77 commitments per opened word.

**D, the 5 device words the phone checks.** D = derive_boxes(h_C, N, coin) ⊂ words 1-32, where
coin is committed in the seal before word 1 and N is drawn by the phone (OS CSPRNG) once the
device half is final. So neither side can steer D.

- **N with the device-half confirm (B8).** The phone draws N when the user holds "I've written
  words 1-32" (offered only after the device's own quiz on words 1-32 passed), persists it,
  and sends it. The device computes the 311 proofs and then the 5 openings while the user
  writes words 33-64 on the phone, so the phone round starts without a wait.
- **Never redraw (B9, B12, R2-2).** The phone draws N once per card and never replaces it.
  After N, neither screen offers Stop (**B12**). A check that ends, ends one of two ways: both
  rounds pass, or it aborts with the device marked suspect (a failed round, a refusal, a missed
  deadline, a device that won't resume). A disconnect, a device that lost its session, the app
  closing or the user leaving the flow doesn't end the check: it pauses it, and the phone sends
  nothing until the user says whether they caused it. "No" is an abort with the device
  suspect. "Yes" resumes on the same N, at most 3 times per check (**R2-2**; the question before
  the last resume warns), and a 4th "Yes" is an abort with the device suspect too. Two gaps
  remain: a paused check the user never comes back to stays unchecked for good, with no
  suspect mark; and the device round has no deadline (PK-O9), since the user paces it, so a
  chip that freezes it is ended only by the user unplugging and then by the cap. Every other
  item the device owes after N has a deadline.
- **One phone per check (B9, B15).** Only the install that ran the wallet's keygen runs the
  verified check (**B15**; its marker is the key's `KeyFormat` record, which only keygen
  writes). It pins the seal while the device is still SEALED and is the only phone that draws
  N, takes proofs and openings, or records the card as checked. A phone without the pin that
  meets a device past SEALED gets "started on another phone" and records nothing (**B9**). Any
  other install with the wallet (a restored wallet, another phone) may still write the card
  *unverified*, once the user chooses to: words 1-32 on the device and 33-64 on the phone, for
  the device's own seal, with both quizzes, and nothing else sent (no proofs, no N). Its record
  is "written, not verified", never checked, and the app says the card must be verified on a
  trusted device that learns all 64 words. So a chip that tells a second phone its card is
  fresh gets no second D. Together this keeps D uniform and drawn once per card, which the
  catch rates need.

**The phone round.** The phone verifies each of the 5 openings against the proven seal and
quizzes the user on those words against the paper.

## 5. The device round E

The device draws E, 5 words of the phone half (33-64), and quizzes the user on them on the
device, then judges the round itself. It never tells the phone E. This is what protects the
backup against a malicious phone (the phone can't fake the user's paper), while D protects it
against a malicious chip. The verdict is two-key: PASSED only if both rounds pass. After N, a
failed device round is an abort with the device suspect, so a chip that has seen D can't fail
round E to get a fresh D.

Catch rates depend on the smallest lie in the device half that nothing can repair, since a
smaller one still restores. A lie of k wrong words is caught when D hits one of them,
1 − C(32−k, 5)/C(32, 5) per card; a wallet is lost only if enough cards lie (2 of 3, 3 of 5).

| Smallest unrepairable lie | per card | 2-of-3 | 3-of-5 |
|---|---|---|---|
| 6 words: against a computer repair tool fixing up to 5 (the owner's working assumption, about one GPU-day; not part of this PR) | 67.3% | 89.3% | 96.5% |
| 4 words: against this PR's offline tool, which repairs up to 3 | 51.2% | 76.2% | 88.4% |
| 3 words: against the device's own repair alone, which fixes up to 2 | 41.0% | 65.2% | 79.5% |

## 6. Flow, in short

1. Seal (~15 s, "Preparing your card"); the phone pulls commitments and P, checks the link and
   hP, pins h_C. No word is shown before this.
2. Device half: words 1-32 on the device, then the device's quiz.
3. The user confirms on the phone; N is drawn and sent.
4. The device proves (~63 s) and opens D (~96 s) behind "Use your phone" (no word is shown or
   re-shown on the device meanwhile, so proof timing can't follow the user's touches on device
   words), while the user writes words 33-64 on the phone and does the phone's quiz.
5. [Continue]: the device round (E), then the phone round (D). Two-key verdict.

Costs are estimates from measured units on the ESP32-C3 (one bit proof 202 ms, one opening
19.2 s); see the open items for what still needs a hardware bench.

## 7. Restore

Words 1-32 are entered on the device, words 33-64 in the app; the device decodes.

- **Masks gate everything (B7).** The device loads nothing and releases nothing derived from a
  decode unless the 24 self-checking masks pass (and, with F as the target, the 12
  wallet-bound masks too). Every failure is the same `CouldNotRead`, whatever the reason. A
  candidate is taken on the self masks alone, and only the one they confirm is checked against
  F. Reason: at
  restore the phone supplies words 33-64 and the target, and without the gate a malicious phone
  could read s a window at a time.
- **Constant work (R2-1).** The decode's timing doesn't depend on the words either. It runs a
  fixed 256 search slices (`RESTORE_SEARCH_SLICES`), each doing the same table work (64
  prefilter checks and one read) with no early exit at a hit, then one finish (the only s·G
  against F), then stand-in slices until T_dec (`RESTORE_DECODE_MS`, 10 s); the result goes
  out at T_dec. Table work on public points has no zero shortcut. The search is a count of
  slices, not a time, so a phone that starves the decode only delays the outcome. On the host,
  slice medians agree within 0.4%; the device estimate is about 25 ms a slice (about 6.4 s for
  the search), still to confirm with the `card-timing` bench (PK-O26). The slowest repair (2
  wrong words at the last positions in search order, plus 1 unreadable) needs a slice count
  that varies by card, about 1953·k/256 for a k-share wallet: 256 slices cover it for every
  card up to about 28 shares, and past that some such cards read as `CouldNotRead` (about a
  quarter of them at 32 shares). PK-O26 retunes the count from a high percentile once the slice
  time is measured. Table addresses follow public points, so a search check and a stand-in
  check read different table entries; each slice's rebuild reads across the whole tables
  anyway, and the bench's shortest and longest slice is the check.
- **WrongWallet (B11).** One exception: a unique decode whose self masks pass but which fails
  s·G = F(i) (or the wallet-bound masks) is reported as a valid card for a different wallet. It
  isn't counted as a failure.
- **Threshold is warn-only.** The device shows the card's threshold and warns, with a hold to
  confirm, when it differs from the wallet's; it never refuses (PK-O4, PK-O6).
- **Consequence of B7:** a card a lying chip wrote with non-derived masks passes "Backup
  checked" and then restores through no app. That's intended (PK-O5): such a card goes to the
  offline tool.

**Restore-failure escalation (B14).** The app counts failed reads per install (persisted):
(1) check the words you entered; (2) try the Frostsnap app on a different phone or laptop; (3)
try a different Frostsnap device; (4) the offline recovery tool. Step 4 needs failures on two
Frostsnap devices (or "I don't have another Frostsnap device"), because one restoring device
can fake a failure on its own.

**Offline recovery tool (B13).** `cargo run -p frost_backup --bin card_offline_recover`: all 64
words on one trusted machine, decode with repair, the masks reported but allowed to be ignored
(the masks-ignoring decode sits behind the `offline-repair` feature, which firmware never
enables). It prints the recovered share (`index:`, `secret:`) on stdout and its report on
stderr; `--poly` and `--backups` optionally check it against the wallet. It validates the escape
hatch; its final form (rebuild the key from a threshold of backups and sweep) is later work.

## 8. What's draft

**PK-O3 (owner, 2 Nov): stays draft until the format freeze before the first release.** The tag
strings, FMT_C and the hash input orders are as built:

- FMT_C = `frostsnap-card|packed-c1|gf2^11|poly=0x805|n=64|k=61|x=word|rand=2019..2023,33..64|thr=2021|hidden=2024..2047|h=idx8+scalar256-be|self=2019,2020,2022,2023,33..52|wallet=53..64|D=words1-32|phone=words33-64`;
- `frostsnap/card/c1/{layout,thr,mask,mask-wallet,phone-half,seal,words}`;
- `frostsnap/card/proof-v3/{bit,open,n-tag,carry-blind,boxes}`;
- `frostsnap/card/commit-v1/{H,ctx,blind-key,blind,box-coin,box-coin-commit}`;
- `frostsnap/card/phone-words-v1` and the `frostsnap/card/rng/…` streams;
- `FORMAT_ID = 3`, `PROOF_VERSION = 3`.

Any of these may still change; nothing reads a card written before the freeze.

## 9. Open (owner decisions not yet taken)

The spec's open items as of 3 Nov (PK-O10, PK-O12, PK-O13 and PK-O15 are closed by R2-2, B15,
R2-1 and the device's WrongWallet page):

- The check: a deadline on the device round (PK-O9); the one-proof drip (PK-O11); whether app
  restarts count toward the cap of 3 resumes, and whether a 4th interruption should end the
  check without asking (PK-O25); a checklist entry for a pending disconnect question (PK-O16);
  merging the changed-P and changed-seal alarms (PK-O19).
- B15's edges: copies of the keygen install's data (an OS migration or backup restore) are
  still keygen installs (PK-O21); whether an unverified card counts as backed up (PK-O22, as
  built it doesn't); the keygen install's lock once a card was written on another install
  (PK-O23); 25-word wallets made on this phone show a card [Backup] the device refuses
  (PK-O24); full verification of an unverified card on a trusted device (PK-O27).
- Restore and the offline tool: the constant-work decode on a real device (PK-O26); the offline
  tool with no F on a unique decode whose masks failed (PK-O14); the offline tool's place in
  the product (PK-O17).
- Copy written by the builders, awaiting the owner's wording (PK-O18); a release guard for the
  `card-timing` bench feature (PK-O20).
- Out of scope: batching the D check and other speed work, the encrypted phone-device link
  (the "Experimental" label), a VRF-derived phone half, a device/phone split other than 32/32.
