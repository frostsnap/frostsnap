use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use rand::{seq::IteratorRandom, Rng};

/// wget https://raw.githubusercontent.com/bitcoin/bips/master/bip-0039/english.txt
const WORDS_FILE: &str = include_str!("../device-names.txt");


pub fn gen_name39() -> String {
    let mut rng = rand::thread_rng();
    let words = WORDS_FILE.split("\n");
    words.choose_multiple(&mut rng, 2).into_iter().collect::<Vec<_>>().join("")
}
