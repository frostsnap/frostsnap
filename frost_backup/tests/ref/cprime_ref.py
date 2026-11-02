#!/usr/bin/env python3
"""Independent reference encoder for card format C', from the spec text.

Written from the spec text (PK.1 layout, PK.2 derivations, PK.3 threshold word, in the team's
design notes) with the slot map of owner decision B6: the 5 extra random values sit at hidden
points x = 2019..2023, not on device words. It shares no code with frost_backup: its own
GF(2^11) (shift-and-add mod x^11 + x^2 + 1), its own bit strings, and Lagrange interpolation by
the product formula.

Value slots (61 values define f, deg f <= 60; word w = f(w), w = 1..64):
  slots 0..4   x = 2019..2023  slot 2 = (0b000 || t8) XOR mu, slots 0,1,3,4 = m1..m4
  slots 5..36  x = 33..64      m5..m36 = the phone half P
  slots 37..60 x = 2024..2047  h0..h23 = u8(i) || ser32(s), 11 bits each, MSB first
Masks: m1..m24 self-checking, B_b = TH("frostsnap/card/c1/mask", L || ser32(s) || u8 i || u8 ctr
|| u8 b), first 264 bits of B_0 || B_1; m25..m36 wallet-bound, TH("frostsnap/card/c1/mask-wallet",
L || ser32(s) || u8 i || ser(F) || u8 ctr), first 132 bits. ctr = smallest giving 64 distinct
words.

Usage: cprime_ref.py [s_hex i t serF_hex]. With no arguments it runs the spec's fixed inputs
(s = h2s(SHA256("frostsnap card tv1 s")), i = 3, t = 2, ser(F) as frost_backup prints it; F is
public input, so it isn't recomputed here), then the first s_k = h2s(SHA256(label || BE32(k)))
that needs counter 1 and counter 2, under the same i, t and F.
"""
import hashlib
import sys

N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141  # secp256k1 order
FMT_C = (b"frostsnap-card|packed-c1|gf2^11|poly=0x805|n=64|k=61|x=word|rand=2019..2023,33..64"
         b"|thr=2021|hidden=2024..2047|h=idx8+scalar256-be|self=2019,2020,2022,2023,33..52"
         b"|wallet=53..64|D=words1-32|phone=words33-64")
SPEC_SER_F = ("02c55cab9ac3dd3445a6248b63dd6f56cd6441c5837c23a6db64bb8a225b51ca4a"
              "03b581d4d32076486d9fdb69254ca4d1f5a78b3767d866c357afac399e9dfbf624")


def gmul(a, b):
    """GF(2^11) product mod x^11 + x^2 + 1, shift and add."""
    r = 0
    for i in range(11):
        if (b >> i) & 1:
            r ^= a << i
    for bit in range(20, 10, -1):
        if (r >> bit) & 1:
            r ^= 0x805 << (bit - 11)
    return r


def ginv(a):
    assert a
    r, e = 1, 2046  # a^(2^11 - 2)
    while e:
        if e & 1:
            r = gmul(r, a)
        a = gmul(a, a)
        e >>= 1
    return r


def th(tag, msg):
    t = hashlib.sha256(tag).digest()
    return hashlib.sha256(t + t + msg).digest()


def bits_of(data):
    return [(byte >> (7 - k)) & 1 for byte in data for k in range(8)]


def chunks11(bits, count):
    return [int("".join(map(str, bits[11 * q:11 * q + 11])), 2) for q in range(count)]


# PK.1 with B6: the point of each slot.
POINTS = [2019, 2020, 2021, 2022, 2023] + list(range(33, 65)) + list(range(2024, 2048))
assert len(POINTS) == 61 and len(set(POINTS)) == 61
THR_SLOT = 2
MASK_SLOTS = [0, 1, 3, 4] + list(range(5, 37))  # m1..m36
LAGRANGE = {}


def lagrange_row(x):
    """l_k(x) for the 61 slot points, by the product formula."""
    if x not in LAGRANGE:
        row = []
        for k, xk in enumerate(POINTS):
            num = den = 1
            for m, xm in enumerate(POINTS):
                if m != k:
                    num = gmul(num, x ^ xm)
                    den = gmul(den, xk ^ xm)
            row.append(gmul(num, ginv(den)))
        LAGRANGE[x] = row
    return LAGRANGE[x]


def words_of(values):
    out = []
    for w in range(1, 65):
        acc = 0
        for l, v in zip(lagrange_row(w), values):
            acc ^= gmul(l, v)
        out.append(acc)
    return out


def layout_hash():
    return th(b"frostsnap/card/c1/layout", bytes([len(FMT_C)]) + FMT_C)


def masks_at(s, i, ser_f, ctr):
    """m1..m36 at one counter (PK.2)."""
    s_bytes = s.to_bytes(32, "big")
    layout = layout_hash()
    b0 = th(b"frostsnap/card/c1/mask", layout + s_bytes + bytes([i, ctr, 0]))
    b1 = th(b"frostsnap/card/c1/mask", layout + s_bytes + bytes([i, ctr, 1]))
    own = chunks11(bits_of(b0 + b1)[:264], 24)
    bw = th(b"frostsnap/card/c1/mask-wallet", layout + s_bytes + bytes([i]) + ser_f + bytes([ctr]))
    wallet = chunks11(bits_of(bw)[:132], 12)
    return own + wallet


def encode(s, i, t, ser_f):
    s_bytes = s.to_bytes(32, "big")
    layout = layout_hash()
    mu = chunks11(bits_of(th(b"frostsnap/card/c1/thr", layout + s_bytes)), 1)[0]
    h = chunks11(bits_of(bytes([i]) + s_bytes), 24)
    for ctr in range(256):
        masks = masks_at(s, i, ser_f, ctr)
        values = [0] * 61
        values[THR_SLOT] = (t & 0xff) ^ mu
        for slot, m in zip(MASK_SLOTS, masks):
            values[slot] = m
        values[37:] = h
        words = words_of(values)
        if len(set(words)) == 64:
            return layout, mu, ctr, masks, values, words
    raise ValueError("no counter")


def show(s, i, t, ser_f):
    layout, mu, ctr, masks, values, words = encode(s, i, t, ser_f)
    print("L", layout.hex())
    print("mu", mu)
    print("ctr", ctr)
    print("masks", masks)
    print("masks at counter 0", masks_at(s, i, ser_f, 0))
    print("values", values)
    print("words", words)
    return ctr


def h2s(data):
    return int.from_bytes(hashlib.sha256(data).digest(), "big") % N


def main():
    if len(sys.argv) == 5:
        show(int(sys.argv[1], 16), int(sys.argv[2]), int(sys.argv[3]), bytes.fromhex(sys.argv[4]))
        return
    ser_f = bytes.fromhex(SPEC_SER_F)
    label = b"frostsnap card tv1 s"
    print("# spec inputs: s = h2s(SHA256(label)), i = 3, t = 2")
    show(h2s(label), 3, 2, ser_f)
    want = {1, 2}
    k = 1
    while want:
        s_k = h2s(label + k.to_bytes(4, "big"))
        ctr = encode(s_k, 3, 2, ser_f)[2]
        if ctr in want:
            want.discard(ctr)
            print("# k = %d needs counter %d" % (k, ctr))
            print("words", encode(s_k, 3, 2, ser_f)[5])
        k += 1


if __name__ == "__main__":
    main()
