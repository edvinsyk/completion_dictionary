use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::attr;
use crate::frequency::{ensure_frequency_source, load_frequencies, sort_by_frequency};
use crate::paths::{atomic_output, data_root, ensure_download};

/// Upstream Swedish Hunspell dictionary (yeager/hunspell-sv), fully expanded forms.
pub const SWEDISH_HUNSPELL_URL: &str =
    "https://raw.githubusercontent.com/yeager/hunspell-sv/main/sv_SE_expanded.dic";

/// LibreOffice Swedish thesaurus (MyThes / Synlex).
pub const MYTHES_URL: &str =
    "https://raw.githubusercontent.com/LibreOffice/dictionaries/master/sv_SE/th_sv_SE.dat";

/// SALDO morphology (Språkbanken): every inflected form with its base word.
/// It adds forms the Hunspell list lacks, such as `viktiga` and `husen`.
pub const SALDO_MORPHOLOGY_URL: &str =
    "https://svn.spraakbanken.gu.se/sb-arkiv/pub/lmf/saldom/saldom.xml";

/// Built from SALDO: inflected forms whose base word has a thesaurus entry.
pub const LEMMA_INDEX_NAME: &str = "sv-lemmas.tsv";

/// SALDO form tags for compound parts, which are not words on their own.
const COMPOUND_MSDS: [&str; 4] = ["c", "ci", "cm", "sms"];

/// Synonym groups, in thesaurus order.
pub type Groups = Vec<Vec<String>>;

fn source_path(directory: &str, file: &str) -> PathBuf {
    data_root().join("sources").join(directory).join(file)
}

pub fn thesaurus_source_path() -> PathBuf {
    source_path("mythes-sv", "th_sv_SE.dat")
}

pub fn lemma_index_path() -> PathBuf {
    data_root().join(LEMMA_INDEX_NAME)
}

/// Parse a Hunspell `.dic` file into unique word forms.
///
/// An initial numeric word-count line is ignored, affix flags after `/` are
/// stripped, whitespace is trimmed, and empty lines are discarded.
pub fn parse_hunspell_dictionary(path: &Path) -> io::Result<HashSet<String>> {
    let text = fs::read_to_string(path)?;
    let mut lines = text.lines().peekable();
    if lines.peek().is_some_and(|line| {
        !line.trim().is_empty() && line.trim().chars().all(|c| c.is_ascii_digit())
    }) {
        lines.next();
    }
    Ok(lines
        .map(|line| line.split('/').next().unwrap_or("").trim())
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect())
}

/// Call `visit(form, base word)` for each standalone word form in SALDO.
///
/// Streams the XML and skips compound parts and pieces of multiword
/// expressions (`msd` values such as `c` or `sg indef nom 1:1-2`).
pub fn read_saldo_forms(path: &Path, mut visit: impl FnMut(String, &str)) -> Result<(), String> {
    let error = |e: &dyn std::fmt::Display| format!("Could not read {}: {e}", path.display());
    let file = File::open(path).map_err(|e| error(&e))?;
    let mut reader = Reader::from_reader(BufReader::with_capacity(1 << 16, file));
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut lemma: Option<String> = None;
    let mut forms: Vec<(String, String)> = Vec::new();
    let mut word_form: Option<(String, String)> = None;

    loop {
        let event = reader.read_event_into(&mut buf).map_err(|e| error(&e))?;
        let (element, is_start) = match &event {
            Event::Start(element) => (element, true),
            Event::Empty(element) => (element, false),
            Event::End(element) => {
                match element.local_name().as_ref() {
                    "WordForm" => forms.extend(word_form.take()),
                    "LexicalEntry" => {
                        if let Some(lemma) = lemma.take().filter(|l| !l.is_empty()) {
                            for (form, msd) in forms.drain(..) {
                                if !form.is_empty()
                                    && !COMPOUND_MSDS.contains(&msd.as_str())
                                    && !msd.contains(':')
                                {
                                    visit(form, &lemma);
                                }
                            }
                        }
                        forms.clear();
                    }
                    _ => {}
                }
                stack.pop();
                buf.clear();
                continue;
            }
            Event::Eof => break,
            _ => {
                buf.clear();
                continue;
            }
        };
        let name = element.local_name().as_ref().to_string();
        match name.as_str() {
            "LexicalEntry" => {
                lemma = None;
                forms.clear();
            }
            "WordForm" => word_form = Some((String::new(), String::new())),
            "feat" => {
                let parent = stack.last().map(String::as_str);
                let grandparent = stack.len().checked_sub(2).map(|i| stack[i].as_str());
                let att = attr(element, "att");
                let val = attr(element, "val").unwrap_or_default();
                match (parent, att.as_deref()) {
                    (Some("FormRepresentation"), Some("writtenForm"))
                        if grandparent == Some("Lemma") && lemma.is_none() =>
                    {
                        lemma = Some(val);
                    }
                    (Some("WordForm"), Some("writtenForm")) => {
                        if let Some(current) = &mut word_form {
                            current.0 = val;
                        }
                    }
                    (Some("WordForm"), Some("msd")) => {
                        if let Some(current) = &mut word_form {
                            current.1 = val;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        if is_start {
            stack.push(name);
        } else if name == "WordForm" {
            forms.extend(word_form.take());
        }
        buf.clear();
    }
    Ok(())
}

/// Python's `str.isspace` for one character, which also counts `\x1c`–`\x1f`.
fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// Build the Swedish completion dictionary from Hunspell and SALDO.
///
/// Downloads/caches the sources if needed and merges the Hunspell forms with
/// SALDO's single-word forms, most common first. The same SALDO pass writes
/// the form index that maps inflected forms of thesaurus headwords to their
/// base word. The thesaurus is cached here too, so later `doc` calls never
/// download from inside the editor. `refresh` downloads all sources again.
pub fn build(output: &Path, refresh: bool) -> Result<(), String> {
    let hunspell = ensure_download(
        SWEDISH_HUNSPELL_URL,
        &source_path("hunspell-sv", "sv_SE_expanded.dic"),
        refresh,
    )?;
    let thesaurus = ensure_download(MYTHES_URL, &thesaurus_source_path(), refresh)?;
    let saldo = ensure_download(
        SALDO_MORPHOLOGY_URL,
        &source_path("saldo", "saldom.xml"),
        refresh,
    )?;
    let frequencies = ensure_frequency_source("sv")?;

    let io_error = |e: io::Error| e.to_string();
    let mut words = parse_hunspell_dictionary(&hunspell).map_err(io_error)?;
    let headwords: HashSet<String> = parse_thesaurus(&thesaurus)
        .map_err(io_error)?
        .into_keys()
        .collect();
    let mut index_pairs = BTreeSet::new();
    read_saldo_forms(&saldo, |form, lemma| {
        if headwords.contains(lemma) && form != lemma {
            index_pairs.insert((form.clone(), lemma.to_string()));
        }
        words.insert(form);
    })?;

    let index_path = lemma_index_path();
    atomic_output(&index_path, |file| {
        let mut out = io::BufWriter::new(file);
        for (form, lemma) in &index_pairs {
            writeln!(out, "{form}\t{lemma}")?;
        }
        out.flush()
    })
    .map_err(io_error)?;
    println!(
        "Wrote {} inflected forms to {}",
        index_pairs.len(),
        index_path.display()
    );

    let frequencies: HashMap<_, _> = load_frequencies(&frequencies)
        .map_err(io_error)?
        .into_iter()
        .collect();
    // Multiword expressions from either source never match a single keyword.
    let sorted_words = sort_by_frequency(
        words.into_iter().filter(|word| !word.chars().any(is_space)),
        &frequencies,
    );
    atomic_output(output, |file| {
        file.write_all((sorted_words.join("\n") + "\n").as_bytes())
    })
    .map_err(io_error)?;
    println!(
        "Wrote {} word forms to {}",
        sorted_words.len(),
        output.display()
    );
    Ok(())
}

/// Parse a MyThes `.dat` file into `{headword: synonym groups}`.
///
/// The first line declares the encoding. Each entry is a `word|N` headword
/// line followed by exactly `N` `|`-separated synonym-group lines. The first
/// entry for a headword wins.
pub fn parse_thesaurus(path: &Path) -> io::Result<HashMap<String, Groups>> {
    let text = read_thesaurus_text(path)?;
    let lines: Vec<&str> = text.lines().collect();
    let mut thesaurus = HashMap::new();
    let mut i = 0;
    while i < lines.len() {
        let Some((headword, group_count)) = parse_headword(lines[i]) else {
            i += 1;
            continue;
        };
        let groups = lines[i + 1..]
            .iter()
            .take(group_count)
            .map(|line| split_group(line))
            .filter(|group| !group.is_empty())
            .collect();
        thesaurus.entry(headword.to_string()).or_insert(groups);
        i = i.saturating_add(group_count).saturating_add(1);
    }
    Ok(thesaurus)
}

/// Read a MyThes file as text, without a UTF-8 byte order mark.
fn read_thesaurus_text(path: &Path) -> io::Result<String> {
    let raw = fs::read(path)?;
    let raw = raw.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&raw);
    Ok(decode_thesaurus(raw))
}

/// Decode MyThes bytes using the encoding declared on the first line.
///
/// shortcut: only UTF-8 and Latin-1 are known; other declared encodings fall
/// back to UTF-8, then Latin-1. Add codecs if a thesaurus needs them.
fn decode_thesaurus(raw: &[u8]) -> String {
    let latin1 = |bytes: &[u8]| bytes.iter().map(|&b| b as char).collect::<String>();
    if let Some(end) = raw.iter().position(|&b| b == b'\n') {
        let header = String::from_utf8_lossy(&raw[..end])
            .trim()
            .to_ascii_lowercase()
            .replace(['-', '_'], "");
        if matches!(header.as_str(), "iso88591" | "latin1") {
            return latin1(raw);
        }
    }
    String::from_utf8(raw.to_vec()).unwrap_or_else(|_| latin1(raw))
}

/// Return `(word, group count)` if `line` is a MyThes `word|N` headword line.
fn parse_headword(line: &str) -> Option<(&str, usize)> {
    if line.starts_with('|') {
        return None;
    }
    let (word, count) = line.rsplit_once('|')?;
    if count.is_empty() || !count.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((word, count.parse().ok()?))
}

/// Split a `|syn1|syn2|...` line into its non-empty synonyms.
fn split_group(line: &str) -> Vec<String> {
    line.split('|')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Byte offset of each match of `key` in `text`.
fn find_all<'a>(text: &'a str, key: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(key).map(|(offset, _)| offset)
}

/// Return the synonym groups of the first `word|N` entry in `text`.
///
/// `text` is the thesaurus with a leading newline. Searching for one headword
/// line is much faster than [`parse_thesaurus`] for a single `doc` call.
/// Group lines start with `|`, so they never match.
fn thesaurus_entry(text: &str, word: &str) -> Option<Groups> {
    let key = format!("\n{word}|");
    for start in find_all(text, &key) {
        let line_end = text[start + 1..]
            .find('\n')
            .map_or(text.len(), |i| start + 1 + i);
        let line = text[start + 1..line_end].trim_end_matches('\r');
        if let Some((headword, group_count)) = parse_headword(line)
            && headword == word
        {
            let rest = text.get(line_end + 1..).unwrap_or("");
            return Some(
                rest.split('\n')
                    .take(group_count)
                    .map(|line| split_group(line.trim_end_matches('\r')))
                    .filter(|group| !group.is_empty())
                    .collect(),
            );
        }
    }
    None
}

/// Return the base words listed for `form` in newline-led index text.
fn index_lemmas<'a>(text: &'a str, form: &str) -> Vec<&'a str> {
    let key = format!("\n{form}\t");
    find_all(text, &key)
        .map(|start| {
            let value = &text[start + key.len()..];
            &value[..value.find('\n').unwrap_or(value.len())]
        })
        .collect()
}

/// Return the synonym groups for `word`, or `None` if not found.
///
/// An exact match wins; otherwise the lowercased word is tried, so a
/// capitalized word at the start of a sentence still finds its entry. If
/// neither is a headword, the SALDO form index maps an inflected form
/// (`viktiga`) to its base word (`viktig`). A form with several base words
/// (`tomten`: `tomt` and `tomte`) returns the groups of all of them. Nothing
/// is downloaded: a missing thesaurus returns `None` and a missing index is
/// skipped.
pub fn lookup_synonyms(word: &str, thesaurus: &Path, lemma_index: Option<&Path>) -> Option<Groups> {
    let text = format!("\n{}", read_thesaurus_text(thesaurus).ok()?);
    let lower = word.to_lowercase();
    let candidates = if lower == word {
        vec![word]
    } else {
        vec![word, lower.as_str()]
    };
    for candidate in &candidates {
        if let Some(groups) = thesaurus_entry(&text, candidate) {
            return Some(groups);
        }
    }

    let index = format!("\n{}", fs::read_to_string(lemma_index?).ok()?);
    let mut groups: Groups = Vec::new();
    for candidate in &candidates {
        for lemma in index_lemmas(&index, candidate) {
            for group in thesaurus_entry(&text, lemma).unwrap_or_default() {
                if !groups.contains(&group) {
                    groups.push(group);
                }
            }
        }
    }
    (!groups.is_empty()).then_some(groups)
}

/// Return `(in completion dictionary, synonym groups)` for `word`.
///
/// Membership is checked against the built dictionary, which merges Hunspell
/// and SALDO forms; it is `false` before a build.
pub fn lookup_word(
    word: &str,
    dictionary: &Path,
    thesaurus: &Path,
    lemma_index: Option<&Path>,
) -> (bool, Option<Groups>) {
    let in_dictionary = fs::read_to_string(dictionary)
        .is_ok_and(|text| format!("\n{text}").contains(&format!("\n{word}\n")));
    (in_dictionary, lookup_synonyms(word, thesaurus, lemma_index))
}

/// Render the blink-cmp-dictionary doc for `query` from the thesaurus.
///
/// A single synonym group is joined on one line; multiple groups are numbered.
/// Returns `""` when there is no thesaurus entry.
pub fn render_doc(query: &str) -> String {
    let index = lemma_index_path();
    let Some(groups) = lookup_synonyms(query, &thesaurus_source_path(), Some(&index)) else {
        return String::new();
    };
    let mut lines = vec![query.to_string(), String::new(), "Synonymer".to_string()];
    if groups.len() == 1 {
        lines.push(groups[0].join(", "));
    } else {
        for (index, group) in groups.iter().enumerate() {
            lines.push(format!("{}. {}", index + 1, group.join(", ")));
        }
    }
    lines.join("\n")
}
