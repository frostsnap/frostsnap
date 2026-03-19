use frost_backup::bip39_words::BIP39_WORDS;

fn levenshtein(a: &[u8], b: &[u8]) -> usize {
    const MAX_LEN: usize = 9;
    let mut prev = [0usize; MAX_LEN];
    let mut curr = [0usize; MAX_LEN];

    let b_len = b.len();
    for j in 0..=b_len {
        prev[j] = j;
    }

    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b_len {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        let tmp = prev;
        prev = curr;
        curr = tmp;
    }
    prev[b_len]
}

fn shared_suffix_len(a: &[u8], b: &[u8]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
}

fn distractor_score(target: &[u8], candidate: &[u8]) -> i32 {
    let edit_dist = levenshtein(target, candidate) as i32;
    let suffix = shared_suffix_len(target, candidate) as i32;
    // 🎯 Weight edit distance heavily, but reward shared suffixes
    edit_dist * 3 - suffix * 2
}

pub(super) fn find_closest_distractors(correct_index: u16) -> [u16; 2] {
    let target = BIP39_WORDS[correct_index as usize].as_bytes();

    let mut best = [(i32::MAX, 0u16); 2];

    for (i, &word) in BIP39_WORDS.iter().enumerate() {
        if i as u16 == correct_index {
            continue;
        }
        let score = distractor_score(target, word.as_bytes());

        if score < best[0].0 {
            best[1] = best[0];
            best[0] = (score, i as u16);
        } else if score < best[1].0 {
            best[1] = (score, i as u16);
        }
    }

    [best[0].1, best[1].1]
}
