mod common;

use std::fs;
use std::path::PathBuf;

use common::{TempDir, fixture};
use completion_dictionary::swedish::{
    MYTHES_URL, SWEDISH_HUNSPELL_URL, lookup_synonyms, lookup_word, parse_hunspell_dictionary,
    parse_thesaurus,
};

fn groups(groups: &[&[&str]]) -> Vec<Vec<String>> {
    groups
        .iter()
        .map(|group| group.iter().map(|s| s.to_string()).collect())
        .collect()
}

fn thesaurus() -> PathBuf {
    fixture("th_sv_test.dat")
}

const BETYDELSEFULL: &[&str] = &[
    "mäktig",
    "inflytelserik",
    "viktigt",
    "signifikant",
    "väsentlig",
    "viktig",
    "betydande",
];

#[test]
fn parses_plain_forms_strips_flags_and_removes_duplicates() {
    let words = parse_hunspell_dictionary(&fixture("sv_test.dic")).unwrap();
    let mut words: Vec<_> = words.into_iter().collect();
    words.sort();
    assert_eq!(
        words,
        [
            "arbetade",
            "arbetar",
            "arbetat",
            "arbete",
            "överenskommelse"
        ]
    );
}

#[test]
fn source_urls_name_expected_files() {
    assert!(SWEDISH_HUNSPELL_URL.ends_with("sv_SE_expanded.dic"));
    assert!(MYTHES_URL.ends_with("th_sv_SE.dat"));
}

#[test]
fn parses_thesaurus_entries_and_groups() {
    let thesaurus = parse_thesaurus(&thesaurus()).unwrap();
    assert_eq!(thesaurus["betydelsefull"], groups(&[BETYDELSEFULL]));
    assert_eq!(thesaurus["hälsa"], groups(&[&["hälsning", "hälsande"]]));
    assert_eq!(thesaurus["överenskommelse"], groups(&[&["avtal"]]));
    assert_eq!(
        thesaurus["flitig"],
        groups(&[&["ivrig", "ambitiös"], &["arbetsam", "strävsam"]])
    );
    assert!(!thesaurus.contains_key("finnsintte"));
}

#[test]
fn decodes_declared_latin1_and_strips_bom() {
    let directory = TempDir::new();
    let latin1 = directory.path().join("latin1.dat");
    fs::write(&latin1, b"ISO8859-1\nh\xe4lsa|1\n|h\xe4lsning\n").unwrap();
    assert_eq!(
        lookup_synonyms("hälsa", &latin1, None).unwrap(),
        Some(groups(&[&["hälsning"]]))
    );

    let bom = directory.path().join("bom.dat");
    fs::write(&bom, "\u{feff}UTF-8\r\nhälsa|1\r\n|hälsning\r\n").unwrap();
    assert_eq!(
        lookup_synonyms("hälsa", &bom, None).unwrap(),
        Some(groups(&[&["hälsning"]]))
    );
}

#[test]
fn lookup_returns_groups_for_exact_word() {
    assert_eq!(
        lookup_synonyms("betydelsefull", &thesaurus(), None).unwrap(),
        Some(groups(&[BETYDELSEFULL]))
    );
    assert_eq!(
        lookup_synonyms("finnsintte", &thesaurus(), None).unwrap(),
        None
    );
}

#[test]
fn lookup_matches_full_parser_for_every_headword() {
    for (word, expected) in parse_thesaurus(&thesaurus()).unwrap() {
        assert_eq!(
            lookup_synonyms(&word, &thesaurus(), None).unwrap(),
            Some(expected)
        );
    }
}

#[test]
fn prefix_of_headword_is_not_a_match() {
    assert_eq!(lookup_synonyms("flit", &thesaurus(), None).unwrap(), None);
}

#[test]
fn falls_back_to_lowercase() {
    assert_eq!(
        lookup_synonyms("Hälsa", &thesaurus(), None).unwrap(),
        Some(groups(&[&["hälsning", "hälsande"]]))
    );
}

/// A built Swedish data directory, with the paths the tests read.
fn built() -> (TempDir, PathBuf, PathBuf) {
    let directory = TempDir::new();
    directory.seed_swedish();
    let output = directory.path().join("sv-hunspell.dict");
    let (code, _, stderr) = directory.run(&["build", "--output", output.to_str().unwrap()]);
    assert_eq!(code, 0, "{stderr}");
    let index = directory.data().join("sv-lemmas.tsv");
    (directory, output, index)
}

#[test]
fn index_keeps_inflected_forms_of_headwords_only() {
    // Base forms, compound parts ("flitig-"), multiword pieces ("hälsor"),
    // and words without a thesaurus entry ("arbetet") are left out.
    let (_directory, _, index) = built();
    assert_eq!(
        fs::read_to_string(index).unwrap(),
        "flitiga\tflitig\nflitigt\tflitig\nhälsan\thälsa\n"
    );
}

#[test]
fn completion_list_merges_saldo_forms() {
    let (_directory, output, _) = built();
    let text = fs::read_to_string(output).unwrap();
    let words: Vec<_> = text.lines().collect();
    for form in ["arbetet", "flitiga", "hälsan", "arbetar"] {
        assert!(words.contains(&form), "{form}");
    }
    for form in ["flitig-", "hälsor", "på gott humör"] {
        assert!(!words.contains(&form), "{form}");
    }
}

#[test]
fn inflected_form_finds_base_word_synonyms() {
    let (_directory, _, index) = built();
    assert_eq!(
        lookup_synonyms("Flitiga", &thesaurus(), Some(&index)).unwrap(),
        Some(groups(&[&["ivrig", "ambitiös"], &["arbetsam", "strävsam"]]))
    );
    assert_eq!(
        lookup_synonyms("arbetet", &thesaurus(), Some(&index)).unwrap(),
        None
    );
}

#[test]
fn missing_index_is_skipped() {
    let (_directory, _, index) = built();
    fs::remove_file(&index).unwrap();
    assert_eq!(
        lookup_synonyms("flitiga", &thesaurus(), Some(&index)).unwrap(),
        None
    );
}

#[test]
fn lookup_word_reports_dictionary_and_thesaurus() {
    let dictionary = fixture("sv_test.dict");
    let lookup = |word| lookup_word(word, &dictionary, &thesaurus(), None).unwrap();
    assert_eq!(
        lookup("överenskommelse"),
        (true, Some(groups(&[&["avtal"]])))
    );
    assert_eq!(lookup("arbete"), (true, None));
    assert_eq!(
        lookup("betydelsefull"),
        (false, Some(groups(&[BETYDELSEFULL])))
    );
    // "viktiga" comes from SALDO, so only the built dictionary has it.
    assert!(lookup("viktiga").0);
    assert_eq!(lookup("någotutanträff"), (false, None));
    assert!(
        !lookup_word("arbete", &fixture("missing.dict"), &thesaurus(), None)
            .unwrap()
            .0
    );
}

#[test]
fn doc_renders_single_and_numbered_groups() {
    let directory = TempDir::new();
    directory.seed("sources/mythes-sv/th_sv_SE.dat", "th_sv_test.dat");

    let (code, stdout, _) = directory.run(&["doc", "betydelsefull"]);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "betydelsefull\n\nSynonymer\nmäktig, inflytelserik, viktigt, signifikant, väsentlig, viktig, betydande\n"
    );

    let (_, stdout, _) = directory.run(&["doc", "flitig"]);
    assert_eq!(
        stdout,
        "flitig\n\nSynonymer\n1. ivrig, ambitiös\n2. arbetsam, strävsam\n"
    );

    let (code, stdout, _) = directory.run(&["doc", "någotutanträff"]);
    assert_eq!((code, stdout.as_str()), (0, ""));
}

#[test]
fn build_writes_sorted_forms_without_multiword_entries() {
    let directory = TempDir::new();
    directory.seed_swedish();
    // An empty SALDO keeps the output to the Hunspell forms under test.
    directory.write(
        "sources/saldo/saldom.xml",
        b"<LexicalResource><Lexicon/></LexicalResource>",
    );
    directory.write(
        "sources/hunspell-sv/sv_SE_expanded.dic",
        "7\narbetat\nBeta\nbeta\növerenskommelse\nad hoc\nalla\tfall\nRed\u{a0}Bull\n".as_bytes(),
    );
    let output = directory.path().join("sv-hunspell.dict");

    let (code, stdout, stderr) = directory.run(&[
        "build",
        "--profile",
        "swedish",
        "--output",
        output.to_str().unwrap(),
    ]);

    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("Wrote 4 word forms to"), "{stdout}");
    assert_eq!(
        fs::read_to_string(output).unwrap(),
        "arbetat\növerenskommelse\nbeta\nBeta\n"
    );
}

#[test]
fn huge_group_count_does_not_overflow() {
    let directory = TempDir::new();
    let path = directory.path().join("huge.dat");
    fs::write(&path, "UTF-8\nord|18446744073709551615\n|synonym\n").unwrap();
    assert_eq!(
        parse_thesaurus(&path).unwrap()["ord"],
        groups(&[&["synonym"]])
    );
}
