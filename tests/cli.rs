mod common;

use common::TempDir;

fn built_english() -> TempDir {
    let directory = TempDir::new();
    directory.seed_english();
    let (code, _, stderr) = directory.run(&["build", "--profile", "english"]);
    assert_eq!(code, 0, "{stderr}");
    directory
}

#[test]
fn unknown_profile_is_a_usage_error() {
    let (code, _, stderr) = TempDir::new().run(&["doc", "--profile", "unknown", "bed"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("invalid value 'unknown'"), "{stderr}");
}

#[test]
fn english_doc_limits_output_to_three_senses() {
    let (code, stdout, _) = built_english().run(&["doc", "--profile", "english", "Bed"]);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "Bed\n\
         \nSense 1 [noun]\nSynonyms: bed, bunk\nILI: i1\nEnglish gloss: a piece of furniture that provides a place to sleep\n\
         \nSense 2 [noun]\nSynonyms: bed\nILI: n/a\nEnglish gloss: a plot of ground & soil\n\
         \nSense 3 [verb]\nSynonyms: bed\nILI: i3\nEnglish gloss: furnish with a bed\n"
    );
}

#[test]
fn doc_and_synonyms_before_build_are_empty() {
    let directory = TempDir::new();
    for profile in ["english", "swedish"] {
        let (code, stdout, stderr) = directory.run(&["doc", "--profile", profile, "bed"]);
        assert_eq!(
            (code, stdout.as_str(), stderr.as_str()),
            (0, "", ""),
            "{profile}"
        );
        let (code, stdout, _) = directory.run(&["synonyms", "--profile", profile, "bed"]);
        assert_eq!((code, stdout.as_str()), (0, "[]\n"), "{profile}");
    }
    // Nothing was downloaded.
    assert!(!directory.data().exists());
}

#[test]
fn build_refresh_rejected_for_english() {
    let directory = TempDir::new();
    let (code, _, stderr) = directory.run(&["build", "--profile", "english", "--refresh"]);
    assert_eq!(code, 1);
    assert_eq!(
        stderr,
        "--refresh is only supported for the swedish profile.\n"
    );
    assert!(!directory.data().exists());
}

#[test]
fn english_lookup_prints_debug_view() {
    let (code, stdout, _) = built_english().run(&["lookup", "--profile", "english", "us"]);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "Profile: english\nLemma: us\nMatches: 1 sense(s)\n\
         \nSense 1\n  Lemma: US\n  Part of speech: noun\n  Synonyms: US\n  ILI: i5\n  English gloss: North American republic\n"
    );
}

#[test]
fn swedish_lookup_reports_dictionary_and_thesaurus() {
    let directory = TempDir::new();
    directory.seed_swedish();
    assert_eq!(directory.run(&["build"]).0, 0);

    let (code, stdout, _) = directory.run(&["lookup", "flitiga"]);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "Profile: swedish\nWord: flitiga\nCompletion dictionary: yes\nThesaurus: yes\n\
         \nSynonyms:\n  ivrig\n  ambitiös\n  arbetsam\n  strävsam\n"
    );

    let (_, stdout, _) = directory.run(&["lookup", "arbetet"]);
    assert_eq!(
        stdout,
        "Profile: swedish\nWord: arbetet\nCompletion dictionary: yes\nThesaurus: no\n"
    );
}

#[test]
fn english_synonyms_keep_pos_and_gloss() {
    let (code, stdout, _) = built_english().run(&["synonyms", "--profile", "english", "bed"]);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "[{\"words\": [\"bunk\"], \"pos\": \"noun\", \"gloss\": \"a piece of furniture that provides a place to sleep\"}]\n"
    );
}

#[test]
fn swedish_synonyms_drop_query_and_empty_groups_unescaped() {
    let directory = TempDir::new();
    directory.write(
        "sources/mythes-sv/th_sv_SE.dat",
        "UTF-8\nviktig|2\n|Viktig|väsentlig\n|viktig\n".as_bytes(),
    );
    let (code, stdout, _) = directory.run(&["synonyms", "viktig"]);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "[{\"words\": [\"väsentlig\"], \"pos\": null, \"gloss\": null}]\n"
    );

    let (_, stdout, _) = directory.run(&["synonyms", "x"]);
    assert_eq!(stdout, "[]\n");
}
