mod common;

use std::fs;

use common::{TempDir, fixture, gzip};
use completion_dictionary::english::{Sense, lookup, read_wordnet};

fn sense(lemma: &str, pos: &str, siblings: &[&str], ili: Option<&str>, gloss: &str) -> Sense {
    Sense {
        lemma: lemma.into(),
        pos: pos.into(),
        siblings: siblings.iter().map(|s| s.to_string()).collect(),
        ili: ili.map(str::to_string),
        gloss: Some(gloss.into()),
    }
}

#[test]
fn reads_senses_in_document_order() {
    let directory = TempDir::new();
    let source = directory.path().join("oewn.xml.gz");
    fs::write(&source, gzip(&fs::read(fixture("oewn_test.xml")).unwrap())).unwrap();

    let (lemmas, senses) = read_wordnet(&source).unwrap();

    assert_eq!(
        lemmas,
        [
            "bed",
            "bed",
            "bunk",
            "US",
            "ice cream",
            "frolicsomeness",
            "AS",
            "As",
            "as"
        ]
    );
    assert_eq!(
        senses[0],
        sense(
            "bed",
            "n",
            &["bed", "bunk"],
            Some("i1"),
            "a piece of furniture that provides a place to sleep"
        )
    );
    // A proposed ILI ("in") is no ILI, and entities are unescaped.
    assert_eq!(
        senses[1],
        sense("bed", "n", &["bed"], None, "a plot of ground & soil")
    );
    assert_eq!(senses.len(), 11);
}

/// A built English data directory.
fn built() -> TempDir {
    let directory = TempDir::new();
    directory.seed_english();
    let output = directory.path().join("en.dict");
    let (code, stdout, stderr) = directory.run(&[
        "build",
        "--profile",
        "english",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, format!("Wrote 10 lemmas to {}\n", output.display()));
    directory
}

#[test]
fn build_sorts_by_frequency_and_adds_common_words() {
    // Common words first; equal frequencies by casefold, lowercase first.
    // "it's" and "3" are not alphabetic, and "ice cream" is multiword.
    let directory = built();
    assert_eq!(
        fs::read_to_string(directory.path().join("en.dict")).unwrap(),
        "the\nfrom\nbed\nus\nUS\nas\nAS\nAs\nbunk\nfrolicsomeness\n"
    );
}

#[test]
fn lookup_ignores_case_and_keeps_wordnet_order() {
    let directory = built();
    let index = directory.data().join("en-oewn.tsv");

    let senses = lookup(&index, "Bed").unwrap();
    let summary: Vec<_> = senses
        .iter()
        .map(|s| (s.pos.as_str(), s.gloss.as_deref().unwrap()))
        .collect();
    assert_eq!(
        summary,
        [
            ("n", "a piece of furniture that provides a place to sleep"),
            ("n", "a plot of ground & soil"),
            ("v", "furnish with a bed"),
            ("v", "put to bed"),
        ]
    );
    assert_eq!(lookup(&index, "us").unwrap()[0].lemma, "US");
    assert_eq!(lookup(&index, "ice cream").unwrap().len(), 1);
}

#[test]
fn lookup_puts_exact_case_first_unless_query_is_normalized() {
    let directory = built();
    let index = directory.data().join("en-oewn.tsv");
    let lemmas = |query| -> Vec<String> {
        lookup(&index, query)
            .unwrap()
            .into_iter()
            .map(|s| s.lemma)
            .collect()
    };
    assert_eq!(lemmas("As"), ["As", "AS", "as"]);
    assert_eq!(lemmas("AS"), ["AS", "As", "as"]);
    assert_eq!(lemmas("aS"), ["AS", "As", "as"]);
    assert_eq!(lemmas("as"), ["AS", "As", "as"]);
}

#[test]
fn lookup_outside_the_index_is_empty() {
    let directory = built();
    let index = directory.data().join("en-oewn.tsv");
    for query in ["", "aaa", "be", "beds", "zzzz", "ö"] {
        assert_eq!(lookup(&index, query).unwrap(), [], "{query}");
    }
    assert_eq!(
        lookup(&directory.path().join("missing.tsv"), "bed").unwrap(),
        []
    );
}
