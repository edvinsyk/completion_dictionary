mod common;

use std::collections::HashMap;

use common::fixture;
use completion_dictionary::frequency::{casefold, load_frequencies, sort_by_frequency, top_n};

#[test]
fn casefold_matches_python_for_latin_and_greek() {
    // Python: "µgSTRAẞEςſÅ".casefold() == "μgstrasseσså"
    assert_eq!(casefold("µgSTRAẞEςſÅ"), "μgstrasseσså");
}

#[test]
fn sorts_by_frequency_then_casefold_lowercase_first() {
    let frequencies: HashMap<_, _> = [
        ("the", 0.05),
        ("front", 0.001),
        ("beta", 0.0001),
        ("μg", 0.0001),
    ]
    .into_iter()
    .map(|(word, value)| (word.to_string(), value))
    .collect();
    let words = [
        "frolicsomeness",
        "Beta",
        "front",
        "beta",
        "zzzq",
        "the",
        "µg",
    ]
    .map(String::from);
    assert_eq!(
        sort_by_frequency(words, &frequencies),
        [
            "the",
            "front",
            "beta",
            "Beta",
            "µg",
            "frolicsomeness",
            "zzzq"
        ]
    );
}

#[test]
fn top_n_skips_number_sequences_like_wordfreq() {
    let list: Vec<(String, f64)> = ["00", "the", "0.0", "3", "1,5", "4x", "from"]
        .iter()
        .map(|word| (word.to_string(), 0.0))
        .collect();
    assert_eq!(
        top_n(&list, 4).collect::<Vec<_>>(),
        ["the", "3", "4x", "from"]
    );
}

#[test]
fn loads_zstd_word_list_in_order() {
    let words = load_frequencies(&fixture("freq_en_test.txt.zst")).unwrap();
    assert_eq!(words[0], ("the".to_string(), 0.05));
    assert_eq!(words[3], ("it's".to_string(), 0.0009));
    assert_eq!(words.len(), 6);
}
