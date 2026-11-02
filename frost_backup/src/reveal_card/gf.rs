//! GF(2^11) arithmetic, one field element per BIP39 word.
//!
//! The field is built from the primitive polynomial x^11 + x^2 + 1, so `2` generates
//! all 2047 nonzero elements. Addition is XOR.

/// Number of field elements (and BIP39 words).
pub const ORDER: usize = 2048;

const POLY: u16 = 0x805;

/// Marks a zero coefficient in a table of logarithms.
pub const NO_LOG: u16 = u16::MAX;

const fn build_tables() -> ([u16; 4096], [u16; ORDER]) {
    let mut exp = [0u16; 4096];
    let mut log = [0u16; ORDER];
    let mut x: u16 = 1;
    let mut i = 0;
    while i < ORDER - 1 {
        exp[i] = x;
        log[x as usize] = i as u16;
        x <<= 1;
        if x & 0x800 != 0 {
            x ^= POLY;
        }
        i += 1;
    }
    // Doubled so the sum of two logarithms never needs reducing.
    while i < 4096 {
        exp[i] = exp[i - (ORDER - 1)];
        i += 1;
    }
    (exp, log)
}

const TABLES: ([u16; 4096], [u16; ORDER]) = build_tables();
static EXP: [u16; 4096] = TABLES.0;
static LOG: [u16; ORDER] = TABLES.1;

#[inline(always)]
pub fn mul(a: u16, b: u16) -> u16 {
    if a == 0 || b == 0 {
        0
    } else {
        EXP[(LOG[a as usize] + LOG[b as usize]) as usize]
    }
}

/// [`mul`] with no branch on either value: both tables are read whatever the operands, and a
/// zero operand masks the product (LOG[0] is 0, so it reads EXP at the other's logarithm). For
/// values that are public but depend on which words a decode leaves out: the reads are the
/// same in number and kind for every operand (review B15 secrecy S-1). Their addresses still
/// follow the operands; see [`crate::verifiable_card::codec`]'s prefilter on why that's
/// accepted.
#[inline(always)]
pub fn mul_ct(a: u16, b: u16) -> u16 {
    let nonzero = ((a != 0) & (b != 0)) as u16;
    EXP[(LOG[a as usize] + LOG[b as usize]) as usize] & nonzero.wrapping_neg()
}

/// [`inv`] with no branch: zero gives zero rather than a panic.
#[inline(always)]
pub fn inv_ct(a: u16) -> u16 {
    EXP[ORDER - 1 - LOG[a as usize] as usize] & ((a != 0) as u16).wrapping_neg()
}

/// Multiplicative inverse. Panics on zero.
pub fn inv(a: u16) -> u16 {
    assert!(a != 0, "zero has no inverse");
    EXP[(ORDER - 1 - LOG[a as usize] as usize) % (ORDER - 1)]
}

/// The logarithm of `a`, or [`NO_LOG`] for zero.
#[inline(always)]
pub fn log_or_none(a: u16) -> u16 {
    if a == 0 {
        NO_LOG
    } else {
        LOG[a as usize]
    }
}

/// Multiplies two elements given as logarithms from [`log_or_none`].
#[inline(always)]
pub fn mul_logs(log_a: u16, log_b: u16) -> u16 {
    if log_a == NO_LOG || log_b == NO_LOG {
        0
    } else {
        EXP[(log_a + log_b) as usize]
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn two_generates_every_nonzero_element() {
        let mut seen = [false; ORDER];
        for &x in &EXP[..ORDER - 1] {
            assert!(x != 0 && !seen[x as usize]);
            seen[x as usize] = true;
        }
    }

    #[test]
    fn field_laws() {
        for a in [1u16, 2, 3, 0x7ff, 0x400, 1234] {
            assert_eq!(mul(a, inv(a)), 1);
            assert_eq!(mul(a, 0), 0);
            for b in [5u16, 0x7fe, 999] {
                assert_eq!(mul(a, b), mul(b, a));
                for c in [7u16, 0x123] {
                    assert_eq!(mul(a, b ^ c), mul(a, b) ^ mul(a, c));
                    assert_eq!(mul(mul(a, b), c), mul(a, mul(b, c)));
                }
                assert_eq!(mul_logs(log_or_none(a), log_or_none(b)), mul(a, b));
            }
        }
        assert_eq!(inv(1), 1);
    }

    #[test]
    fn the_branchless_ops_agree_with_the_tables() {
        for a in 0..ORDER as u16 {
            for b in 0..ORDER as u16 {
                assert_eq!(mul_ct(a, b), mul(a, b), "{a}·{b}");
            }
            let expected = if a == 0 { 0 } else { inv(a) };
            assert_eq!(inv_ct(a), expected, "1/{a}");
        }
    }
}
