use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::paths::{data_root, ensure_download};

/// wordfreq's "large" word lists, as published by wordfreq-rs.
pub const FREQUENCY_URL: &str =
    "https://github.com/kampersanda/wordfreq-rs/releases/download/models-v1/";

pub fn frequency_source_path(lang: &str) -> PathBuf {
    data_root()
        .join("sources")
        .join("wordfreq")
        .join(format!("large_{lang}.txt.zst"))
}

/// Return the cached word list for `lang`, downloading it if needed.
pub fn ensure_frequency_source(lang: &str) -> Result<PathBuf, String> {
    let url = format!("{FREQUENCY_URL}large_{lang}.txt.zst");
    ensure_download(&url, &frequency_source_path(lang), false)
}

/// Read `word frequency` lines, most common word first.
pub fn load_frequencies(path: &Path) -> io::Result<Vec<(String, f64)>> {
    let decoder = ruzstd::decoding::StreamingDecoder::new(BufReader::new(File::open(path)?))
        .map_err(io::Error::other)?;
    let mut frequencies = Vec::new();
    for line in BufReader::new(decoder).lines() {
        let line = line?;
        let Some((word, value)) = line.rsplit_once(' ') else {
            continue;
        };
        let value = value
            .parse()
            .map_err(|_| io::Error::other(format!("bad frequency line: {line}")))?;
        frequencies.push((word.to_string(), value));
    }
    Ok(frequencies)
}

/// Python's `str.casefold` for Latin and Greek text.
///
/// shortcut: folds only the Latin and Greek letters where case folding differs
/// from `to_lowercase`; use full Unicode case folding if a word list needs more.
pub fn casefold(word: &str) -> String {
    let mut folded = String::with_capacity(word.len());
    for c in word.to_lowercase().chars() {
        match c {
            'µ' => folded.push('μ'),
            'ß' => folded.push_str("ss"),
            'ſ' => folded.push('s'),
            'ς' => folded.push('σ'),
            c => folded.push(c),
        }
    }
    folded
}

/// wordfreq's `has_digit_sequence`: a digit, then more digits, dots or commas.
fn has_digit_sequence(word: &str) -> bool {
    let mut chars = word.chars();
    chars.next().is_some_and(char::is_numeric)
        && chars
            .next()
            .is_some_and(|c| c.is_numeric() || c == '.' || c == ',')
}

/// wordfreq's `top_n_list`: the `n` most common words, without number sequences.
pub fn top_n(frequencies: &[(String, f64)], n: usize) -> impl Iterator<Item = &str> {
    frequencies
        .iter()
        .map(|(word, _)| word.as_str())
        .filter(|word| !has_digit_sequence(word))
        .take(n)
}

/// Python's `str.islower`: has a cased character and no uppercase one.
fn is_lower(word: &str) -> bool {
    word.chars().any(char::is_lowercase) && !word.chars().any(char::is_uppercase)
}

/// Return `words` with the most common first, then alphabetically.
///
/// A lowercase word comes before its capitalized twin (`frost`, `Frost`),
/// because both have the same frequency. The completion menu keeps this file
/// order for equally good matches, so `fro` offers `from` before
/// `frolicsomeness`. Unknown words sort last, alphabetically.
pub fn sort_by_frequency(
    words: impl IntoIterator<Item = String>,
    frequencies: &HashMap<String, f64>,
) -> Vec<String> {
    let mut keyed: Vec<_> = words
        .into_iter()
        .map(|word| {
            let folded = casefold(&word);
            let frequency = frequencies.get(&folded).copied().unwrap_or(0.0);
            (frequency, folded, !is_lower(&word), word)
        })
        .collect();
    keyed.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(Ordering::Equal)
            .then_with(|| (&a.1, a.2, &a.3).cmp(&(&b.1, b.2, &b.3)))
    });
    keyed.into_iter().map(|(_, _, _, word)| word).collect()
}
