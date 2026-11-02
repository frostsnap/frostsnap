//! How long restore takes to repair a lie, and how often a check catches one.
//!
//! This is the cost model from the design notes, with each kind of try weighted by its
//! measured time instead of counted as one unit. It prices two search orders, because
//! they differ whenever restore erases more words than the lie.

use super::card::Design;
use super::gf::ORDER;

/// Measured time of one try of each kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    /// Seconds to rebuild from parity once (erasing at most `parity` words).
    pub rebuild_s: f64,
    /// Seconds per candidate when brute-forcing words past the parity.
    pub guess_s: f64,
}

impl Rates {
    /// Every try costs one second, so times read as try counts (the notes' model).
    pub const UNIT: Rates = Rates {
        rebuild_s: 1.0,
        guess_s: 1.0,
    };
}

/// The order restore tries sets of erased positions in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// The design notes' count, C(n, m) / C(n − e, m − e) sets. When `m` equals the lie
    /// this is every set; when `m` is bigger it is the covering bound, which a covering
    /// design reaches and a random order reaches on average.
    Ideal,
    /// [`search`](super::search::search)'s lexicographic order, against a lie on the last
    /// words: C(n − m + e, e) sets.
    Lexicographic,
}

/// The cheapest way to repair a given number of wrong words.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    /// Positions to erase per try.
    pub m: usize,
    /// Tries before the lie is found, in the worst case for the order.
    pub tries: f64,
    /// Seconds for those tries.
    pub seconds: f64,
    /// The decoder fixes this many wrong words directly.
    pub instant: bool,
}

pub fn choose(n: usize, k: usize) -> f64 {
    if k > n {
        return 0.0;
    }
    (0..k).fold(1.0, |acc, i| acc * (n - i) as f64 / (i + 1) as f64)
}

/// The cheapest search for `wrong` wrong words at unknown positions.
pub fn plan(design: Design, wrong: usize, rates: Rates, order: Order) -> Plan {
    let (n, p) = (design.n(), design.parity);
    if 2 * wrong <= p {
        return Plan {
            m: wrong,
            tries: 1.0,
            seconds: rates.rebuild_s,
            instant: true,
        };
    }
    let mut best: Option<Plan> = None;
    for m in wrong..=n {
        let guessed = m.saturating_sub(p);
        let mut tries = match order {
            Order::Ideal => choose(n, m) / choose(n - wrong, m - wrong),
            Order::Lexicographic => choose(n - m + wrong, wrong),
        };
        let per_try = if guessed == 0 {
            rates.rebuild_s
        } else {
            tries = (0..guessed).fold(tries, |t, _| t * ORDER as f64);
            rates.guess_s
        };
        let seconds = tries * per_try;
        if best.is_none_or(|b| seconds < b.seconds) {
            best = Some(Plan {
                m,
                tries,
                seconds,
                instant: false,
            });
        }
    }
    best.expect("wrong <= n")
}

/// The smallest lie whose repair takes longer than `budget_s`.
pub fn smallest_lie(design: Design, rates: Rates, budget_s: f64, order: Order) -> usize {
    (1..=design.n())
        .find(|&e| plan(design, e, rates, order).seconds > budget_s)
        .unwrap_or(design.n() + 1)
}

/// Chance that `openings` random positions hit at least one of `lie` wrong words.
pub fn catch_rate(n: usize, lie: usize, openings: usize) -> f64 {
    if openings == 0 {
        return 0.0;
    }
    if lie > n || n - lie < openings {
        return 1.0;
    }
    let miss = (0..openings).fold(1.0, |acc, i| acc * (n - lie - i) as f64 / (n - i) as f64);
    1.0 - miss
}

#[cfg(test)]
mod test {
    use super::*;

    const DEVICE: f64 = 16_777_216.0; // 2^24
    const TOOL: f64 = 1_125_899_906_842_624.0; // 2^50

    #[test]
    fn matches_the_design_notes() {
        // (pad, parity, lie on device, lie with the tool, catch per card on device)
        for (pad, parity, r_dev, r_tool, catch) in [
            (2, 6, 7, 9, 0.395),
            (3, 5, 6, 8, 0.476),
            (4, 4, 5, 7, 0.512),
            (5, 3, 4, 6, 0.512),
            (6, 2, 4, 5, 0.584),
            (6, 6, 7, 9, 0.756),
        ] {
            let design = Design::new(pad, parity);
            assert_eq!(
                smallest_lie(design, Rates::UNIT, DEVICE, Order::Ideal),
                r_dev,
                "{design}"
            );
            assert_eq!(
                smallest_lie(design, Rates::UNIT, TOOL, Order::Ideal),
                r_tool,
                "{design}"
            );
            let c = catch_rate(design.n(), r_dev, pad);
            assert!((c - catch).abs() < 0.0005, "{design}: {c}");
        }
    }

    #[test]
    fn plan_guesses_only_past_the_parity() {
        let design = Design::new(6, 2);
        assert!(plan(design, 1, Rates::UNIT, Order::Ideal).instant);
        let p = plan(design, 3, Rates::UNIT, Order::Ideal);
        assert_eq!((p.m, p.tries), (3, choose(32, 3) * 2048.0));
        // Erasing more words than the lie can be cheaper: 4/4 repairs 3 by erasing 4.
        let p = plan(Design::new(4, 4), 3, Rates::UNIT, Order::Ideal);
        assert_eq!(p.m, 4);
        // The lexicographic search needs C(31, 3) sets for the same lie.
        let p = plan(Design::new(4, 4), 3, Rates::UNIT, Order::Lexicographic);
        assert_eq!((p.m, p.tries), (4, choose(31, 3)));
        // When the plan erases exactly the lie, both orders try every set.
        let design = Design::new(6, 2);
        assert_eq!(
            plan(design, 3, Rates::UNIT, Order::Lexicographic),
            plan(design, 3, Rates::UNIT, Order::Ideal)
        );
    }
}
