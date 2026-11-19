//! The repair search restore runs when a card has more wrong words than the decoder can
//! fix directly: guess which `m` words are wrong and rebuild them.
//!
//! When `m` is at most the parity, the other words rebuild the polynomial and any spare
//! words cross-check it. Past the parity, the first `m - parity` erased words have no
//! redundancy left, so every one of their 2048 values is tried. Each surviving candidate
//! goes to the caller's check (the polynomial checksum, then the share image).

use super::card::{eval, point, Basis, Design, KEY_WORDS};
use super::gf;
use alloc::vec::Vec;

/// Most words whose values the search will brute-force at once.
pub const MAX_GUESSED: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchStats {
    /// Sets of erased positions tried.
    pub subsets: u64,
    /// Candidate keys passed to the check.
    pub candidates: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The key words of the candidate the check accepted.
    Found([u16; KEY_WORDS]),
    /// Every set of `m` positions was tried.
    Exhausted,
    /// `stop` asked the search to end early.
    Stopped,
}

/// Tries every set of `m` card positions, in lexicographic order, as the wrong words.
///
/// `check` decides whether candidate key words are the share being restored. `stop` is
/// polled after each set of positions (and every 2048 candidates when guessing more than
/// one word) and ends the search when it returns true.
pub fn search(
    design: Design,
    words: &[u16],
    m: usize,
    check: &mut impl FnMut(&[u16; KEY_WORDS]) -> bool,
    stats: &mut SearchStats,
    stop: &mut impl FnMut(&SearchStats) -> bool,
) -> Outcome {
    let (n, k, pad) = (design.n(), design.k(), design.pad);
    assert_eq!(words.len(), n);
    assert!(m <= n);
    let guessed = m.saturating_sub(design.parity);
    assert!(guessed <= MAX_GUESSED, "guessing {guessed} words at once");

    let mut erased: Vec<usize> = (0..m).collect();
    let mut known: Vec<usize> = Vec::with_capacity(n);
    let mut xs: Vec<u16> = Vec::with_capacity(k);
    let mut basis = Basis::default();
    let mut coeffs = vec![0u16; k];
    let mut guess_logs = [[gf::NO_LOG; KEY_WORDS]; MAX_GUESSED];

    loop {
        stats.subsets += 1;
        known.clear();
        let mut next_erased = 0;
        for j in 0..n {
            if next_erased < m && erased[next_erased] == j {
                next_erased += 1;
            } else {
                known.push(j);
            }
        }

        if guessed == 0 {
            // Rebuild from the first k known words; the rest must agree.
            xs.clear();
            xs.extend(known[..k].iter().map(|&j| point(j)));
            basis.reset(&xs);
            coeffs.fill(0);
            for (i, &j) in known[..k].iter().enumerate() {
                basis.add_scaled(i, words[j], &mut coeffs);
            }
            if known[k..]
                .iter()
                .all(|&j| eval(&coeffs, point(j)) == words[j])
            {
                stats.candidates += 1;
                let key: [u16; KEY_WORDS] = coeffs[pad..].try_into().expect("k = pad + 24");
                if check(&key) {
                    return Outcome::Found(key);
                }
            }
            if stop(stats) {
                return Outcome::Stopped;
            }
        } else {
            // The known words plus the guessed ones fix the polynomial. Its key words are
            // linear in the guesses: base + Σ value_t · basis_t.
            xs.clear();
            xs.extend(known.iter().map(|&j| point(j)));
            xs.extend(erased[..guessed].iter().map(|&j| point(j)));
            debug_assert_eq!(xs.len(), k);
            basis.reset(&xs);
            coeffs.fill(0);
            for (i, &j) in known.iter().enumerate() {
                basis.add_scaled(i, words[j], &mut coeffs);
            }
            let base: [u16; KEY_WORDS] = coeffs[pad..].try_into().expect("k = pad + 24");
            for (t, logs) in guess_logs[..guessed].iter_mut().enumerate() {
                coeffs.fill(0);
                basis.add_scaled(known.len() + t, 1, &mut coeffs);
                for (log, &c) in logs.iter_mut().zip(&coeffs[pad..]) {
                    *log = gf::log_or_none(c);
                }
            }
            if let Some(outcome) = try_values(&base, &guess_logs[..guessed], check, stats, stop) {
                return outcome;
            }
        }

        if !next_combination(&mut erased, n) {
            return Outcome::Exhausted;
        }
    }
}

/// Tries all 2048 values of the first guessed word, recursing into the rest.
fn try_values(
    base: &[u16; KEY_WORDS],
    logs: &[[u16; KEY_WORDS]],
    check: &mut impl FnMut(&[u16; KEY_WORDS]) -> bool,
    stats: &mut SearchStats,
    stop: &mut impl FnMut(&SearchStats) -> bool,
) -> Option<Outcome> {
    let (logs_here, rest) = logs.split_first().expect("at least one guessed word");
    let mut key = *base;
    for value in 0..gf::ORDER as u16 {
        let log_v = gf::log_or_none(value);
        for i in 0..KEY_WORDS {
            key[i] = base[i] ^ gf::mul_logs(log_v, logs_here[i]);
        }
        if rest.is_empty() {
            stats.candidates += 1;
            if check(&key) {
                return Some(Outcome::Found(key));
            }
        } else if let Some(outcome) = try_values(&key, rest, check, stats, stop) {
            return Some(outcome);
        }
    }
    if rest.is_empty() && stop(stats) {
        return Some(Outcome::Stopped);
    }
    None
}

/// Advances `c` to the next `c.len()`-subset of `0..n` in lexicographic order.
fn next_combination(c: &mut [usize], n: usize) -> bool {
    let m = c.len();
    for i in (0..m).rev() {
        if c[i] < n - m + i {
            c[i] += 1;
            for j in i + 1..m {
                c[j] = c[j - 1] + 1;
            }
            return true;
        }
    }
    false
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::reveal_card::card::Card;
    use rand::Rng;

    fn card(design: Design, rng: &mut impl Rng) -> Card {
        let pad: Vec<u16> = (0..design.pad).map(|_| rng.gen_range(0..2048)).collect();
        let key: [u16; KEY_WORDS] = core::array::from_fn(|_| rng.gen_range(0..2048));
        Card::new(design, &pad, &key)
    }

    fn corrupt(words: &mut [u16], positions: &[usize], rng: &mut impl Rng) {
        for &j in positions {
            let old = words[j];
            while words[j] == old {
                words[j] = rng.gen_range(0..2048);
            }
        }
    }

    fn run(design: Design, wrong: &[usize], m: usize) -> (Outcome, SearchStats, Card) {
        let mut rng = rand::thread_rng();
        let card = card(design, &mut rng);
        let mut words = card.words.clone();
        corrupt(&mut words, wrong, &mut rng);
        let truth: [u16; KEY_WORDS] = card.key().try_into().unwrap();
        let mut stats = SearchStats::default();
        let outcome = search(
            design,
            &words,
            m,
            &mut |key| *key == truth,
            &mut stats,
            &mut |_| false,
        );
        (outcome, stats, card)
    }

    #[test]
    fn rebuilds_within_the_parity() {
        let (outcome, stats, card) = run(Design::new(4, 4), &[0, 1, 2], 3);
        assert_eq!(outcome, Outcome::Found(card.key().try_into().unwrap()));
        assert_eq!(stats.subsets, 1);
        // Anywhere on the card, erasing up to the parity.
        let (outcome, _, card) = run(Design::new(4, 4), &[5, 30], 4);
        assert_eq!(outcome, Outcome::Found(card.key().try_into().unwrap()));
    }

    #[test]
    fn guesses_past_the_parity() {
        for design in [Design::new(4, 4), Design::new(6, 2), Design::new(7, 1)] {
            let wrong: Vec<usize> = (0..design.parity + 1).collect();
            let (outcome, stats, card) = run(design, &wrong, wrong.len());
            assert_eq!(outcome, Outcome::Found(card.key().try_into().unwrap()));
            assert_eq!(stats.subsets, 1);
            assert!(stats.candidates <= 2048);
        }
    }

    #[test]
    fn misses_when_too_few_words_are_erased() {
        let (outcome, stats, _) = run(Design::new(4, 4), &[3, 9, 17], 2);
        assert_eq!(outcome, Outcome::Exhausted);
        assert_eq!(stats.subsets, 32 * 31 / 2);
    }

    #[test]
    fn stop_ends_the_search() {
        let design = Design::new(6, 2);
        let mut rng = rand::thread_rng();
        let card = card(design, &mut rng);
        let mut stats = SearchStats::default();
        let outcome = search(
            design,
            &card.words,
            3,
            &mut |_| false,
            &mut stats,
            &mut |s| s.subsets >= 3,
        );
        assert_eq!(outcome, Outcome::Stopped);
        assert_eq!((stats.subsets, stats.candidates), (3, 3 * 2048));
    }

    #[test]
    fn lexicographic_worst_case_matches_the_model() {
        use crate::reveal_card::model::{plan, Order, Rates};
        for (design, wrong) in [(Design::new(4, 4), 3), (Design::new(3, 5), 4)] {
            let n = design.n();
            let last: Vec<usize> = (n - wrong..n).collect();
            let p = plan(design, wrong, Rates::UNIT, Order::Lexicographic);
            let (outcome, stats, _) = run(design, &last, p.m);
            assert!(matches!(outcome, Outcome::Found(_)), "{design}");
            assert_eq!(stats.subsets as f64, p.tries, "{design}");
        }
    }

    #[test]
    fn next_combination_visits_every_subset() {
        let mut c = vec![0, 1, 2];
        let mut count = 1;
        while next_combination(&mut c, 6) {
            count += 1;
        }
        assert_eq!(count, 20);
        assert_eq!(c, vec![3, 4, 5]);
    }
}
