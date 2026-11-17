use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use quick_xml::Reader;
use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::Event;

use crate::attr;
use crate::frequency::{
    casefold, ensure_frequency_source, load_frequencies, sort_by_frequency, top_n,
};
use crate::paths::{atomic_output, data_root, ensure_download};

/// Open English WordNet 2025+, the lexicon `wn` installs as `oewn:2025+`.
pub const OEWN_URL: &str = "https://en-word.net/static/english-wordnet-2025-plus.xml.gz";

/// How many of the most common English words to add to the WordNet lemmas.
/// WordNet lacks function words ("from", "the") and inflected forms ("was").
pub const ENGLISH_COMMON_WORDS: usize = 20000;

/// Built from OEWN: one line per sense, sorted by casefolded lemma.
pub const INDEX_NAME: &str = "en-oewn.tsv";

#[derive(Debug, Clone, PartialEq)]
pub struct Sense {
    pub lemma: String,
    pub pos: String,
    pub siblings: Vec<String>,
    pub ili: Option<String>,
    pub gloss: Option<String>,
}

pub fn wordnet_source_path() -> PathBuf {
    data_root()
        .join("sources")
        .join("oewn")
        .join("english-wordnet-2025-plus.xml.gz")
}

pub fn index_path() -> PathBuf {
    data_root().join(INDEX_NAME)
}

#[derive(Default)]
struct Synset {
    ili: Option<String>,
    gloss: Option<String>,
    members: Vec<String>,
}

/// Read a gzipped WN-LMF file into its lemmas and senses, in document order.
pub fn read_wordnet(path: &Path) -> Result<(Vec<String>, Vec<Sense>), String> {
    let error = |e: &dyn std::fmt::Display| format!("Could not read {}: {e}", path.display());
    let file = File::open(path).map_err(|e| error(&e))?;
    let mut reader = Reader::from_reader(BufReader::new(GzDecoder::new(BufReader::new(file))));
    let mut buf = Vec::new();

    // (entry id, lemma, part of speech, synset ids)
    let mut entries: Vec<(String, String, String, Vec<String>)> = Vec::new();
    let mut synsets: HashMap<String, Synset> = HashMap::new();
    let mut synset_id: Option<String> = None;
    let mut definition: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf).map_err(|e| error(&e))? {
            Event::Start(element) | Event::Empty(element) => match element.local_name().as_ref() {
                "LexicalEntry" => entries.push((
                    attr(&element, "id").unwrap_or_default(),
                    String::new(),
                    String::new(),
                    Vec::new(),
                )),
                "Lemma" => {
                    if let Some(entry) = entries.last_mut() {
                        entry.1 = attr(&element, "writtenForm").unwrap_or_default();
                        entry.2 = attr(&element, "partOfSpeech").unwrap_or_default();
                    }
                }
                "Sense" => {
                    if let (Some(entry), Some(synset)) =
                        (entries.last_mut(), attr(&element, "synset"))
                    {
                        entry.3.push(synset);
                    }
                }
                "Synset" => {
                    let id = attr(&element, "id").unwrap_or_default();
                    let synset = Synset {
                        // wn reports proposed ILIs ("in") as no ILI.
                        ili: attr(&element, "ili").filter(|ili| !ili.is_empty() && ili != "in"),
                        gloss: None,
                        members: attr(&element, "members")
                            .unwrap_or_default()
                            .split_whitespace()
                            .map(str::to_string)
                            .collect(),
                    };
                    synsets.insert(id.clone(), synset);
                    synset_id = Some(id);
                }
                "Definition" if synset_id.is_some() => definition = Some(String::new()),
                _ => {}
            },
            Event::Text(text) => {
                if let Some(definition) = &mut definition {
                    definition.push_str(&text.xml10_content());
                }
            }
            Event::GeneralRef(reference) => {
                if let Some(definition) = &mut definition {
                    if let Some(c) = reference.resolve_char_ref().map_err(|e| error(&e))? {
                        definition.push(c);
                    } else if let Some(entity) = resolve_xml_entity(&reference) {
                        definition.push_str(entity);
                    }
                }
            }
            Event::End(element) => match element.local_name().as_ref() {
                "Definition" => {
                    // wn uses a synset's first definition.
                    if let (Some(text), Some(synset)) = (
                        definition.take(),
                        synset_id.as_ref().and_then(|id| synsets.get_mut(id)),
                    ) && synset.gloss.is_none()
                        && !text.is_empty()
                    {
                        synset.gloss = Some(text);
                    }
                }
                "Synset" => synset_id = None,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    let lemmas: HashMap<&str, &str> = entries
        .iter()
        .map(|(id, lemma, _, _)| (id.as_str(), lemma.as_str()))
        .collect();
    let mut senses = Vec::new();
    for (_, lemma, pos, synset_ids) in &entries {
        for synset_id in synset_ids {
            let synset = synsets
                .get(synset_id)
                .ok_or_else(|| error(&format!("unknown synset {synset_id}")))?;
            let mut seen = HashSet::new();
            senses.push(Sense {
                lemma: lemma.clone(),
                pos: pos.clone(),
                siblings: synset
                    .members
                    .iter()
                    .filter_map(|member| lemmas.get(member.as_str()))
                    .filter(|sibling| seen.insert(**sibling))
                    .map(|sibling| sibling.to_string())
                    .collect(),
                ili: synset.ili.clone(),
                gloss: synset.gloss.clone(),
            });
        }
    }
    let lemmas = entries.into_iter().map(|(_, lemma, _, _)| lemma).collect();
    Ok((lemmas, senses))
}

/// Tabs and newlines would break the index format.
fn clean(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

/// Write `key\tlemma\tpos\tili\tgloss\tsibling...` lines, sorted by key.
///
/// The sort is stable, so senses with the same key keep WordNet order.
fn write_index(senses: &[Sense], path: &Path) -> io::Result<()> {
    let mut lines: Vec<(String, String)> = senses
        .iter()
        .map(|sense| {
            let mut fields = vec![
                clean(&sense.lemma),
                clean(&sense.pos),
                sense.ili.as_deref().map(clean).unwrap_or_default(),
                sense.gloss.as_deref().map(clean).unwrap_or_default(),
            ];
            fields.extend(sense.siblings.iter().map(|sibling| clean(sibling)));
            (casefold(&clean(&sense.lemma)), fields.join("\t"))
        })
        .collect();
    lines.sort_by(|a, b| a.0.cmp(&b.0));
    atomic_output(path, |file| {
        let mut out = io::BufWriter::new(file);
        for (key, line) in &lines {
            writeln!(out, "{key}\t{line}")?;
        }
        out.flush()
    })
}

/// Build the English completion dictionary and the sense index from OEWN.
pub fn build(output: &Path) -> Result<(), String> {
    let source = ensure_download(OEWN_URL, &wordnet_source_path(), false)?;
    let frequencies = ensure_frequency_source("en")?;
    let (lemmas, senses) = read_wordnet(&source)?;
    write_index(&senses, &index_path()).map_err(|e| e.to_string())?;

    let frequencies = load_frequencies(&frequencies).map_err(|e| e.to_string())?;
    // Multiword lemmas ("ice cream") never match a single typed keyword.
    let mut words: HashSet<String> = lemmas
        .iter()
        .map(|lemma| lemma.trim())
        .filter(|lemma| !lemma.is_empty() && !lemma.contains(' '))
        .map(str::to_string)
        .collect();
    words.extend(
        top_n(&frequencies, ENGLISH_COMMON_WORDS)
            .filter(|word| !word.is_empty() && word.chars().all(char::is_alphabetic))
            .map(str::to_string),
    );
    let sorted = sort_by_frequency(words, &frequencies.into_iter().collect());
    atomic_output(output, |file| {
        file.write_all((sorted.join("\n") + "\n").as_bytes())
    })
    .map_err(|e| e.to_string())?;
    println!("Wrote {} lemmas to {}", sorted.len(), output.display());
    Ok(())
}

/// Return the start offset and bytes of the first line at or after `pos`.
fn line_at(file: &mut File, pos: u64) -> io::Result<(u64, Vec<u8>)> {
    let from = pos.saturating_sub(1);
    file.seek(SeekFrom::Start(from))?;
    let mut reader = BufReader::new(file);
    let mut skipped = 0;
    if pos > 0 {
        skipped = reader.read_until(b'\n', &mut Vec::new())? as u64;
    }
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line)?;
    Ok((from + skipped, line))
}

fn line_key(line: &[u8]) -> &[u8] {
    line.split(|&b| b == b'\t').next().unwrap_or(line)
}

/// Return the senses whose lemma matches `query` without regard to case.
///
/// Binary-searches the sorted index, so a lookup reads a few pages instead of
/// the whole file. A missing index returns no senses.
pub fn lookup(index: &Path, query: &str) -> io::Result<Vec<Sense>> {
    let mut file = match File::open(index) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        file => file?,
    };
    let key = casefold(query);
    let (mut low, mut high) = (0, file.metadata()?.len());
    while low < high {
        let middle = low + (high - low) / 2;
        let (_, line) = line_at(&mut file, middle)?;
        if !line.is_empty() && line_key(&line) < key.as_bytes() {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let (start, _) = line_at(&mut file, low)?;
    file.seek(SeekFrom::Start(start))?;

    let mut senses = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        let mut fields = line.split('\t');
        if fields.next() != Some(key.as_str()) {
            break;
        }
        let mut field = || fields.next().unwrap_or("").to_string();
        let (lemma, pos, ili, gloss) = (field(), field(), field(), field());
        senses.push(Sense {
            lemma,
            pos,
            ili: (!ili.is_empty()).then_some(ili),
            gloss: (!gloss.is_empty()).then_some(gloss),
            siblings: fields.map(str::to_string).collect(),
        });
    }
    // wn lists exact-case matches first, unless the query is already
    // normalized (casefolded, no accents); then all matches keep entry order.
    // shortcut: treats any non-ASCII query as not normalized; wn uses NFKD.
    if query != key || !query.is_ascii() {
        senses.sort_by_key(|sense| sense.lemma != query);
    }
    Ok(senses)
}

pub(crate) fn format_pos(pos: &str) -> &str {
    match pos {
        "n" => "noun",
        "v" => "verb",
        "a" => "adjective",
        "s" => "adjective satellite",
        "r" => "adverb",
        other => other,
    }
}

/// Render the blink-cmp-dictionary documentation for `query`.
pub fn render_doc(query: &str) -> io::Result<String> {
    let senses = lookup(&index_path(), query)?;
    if senses.is_empty() {
        return Ok(String::new());
    }
    let mut lines = vec![query.to_string()];
    for (index, sense) in senses.iter().take(3).enumerate() {
        lines.push(String::new());
        lines.push(format!("Sense {} [{}]", index + 1, format_pos(&sense.pos)));
        lines.push(format!("Synonyms: {}", sense.siblings.join(", ")));
        lines.push(format!("ILI: {}", sense.ili.as_deref().unwrap_or("n/a")));
        lines.push(format!(
            "English gloss: {}",
            sense.gloss.as_deref().unwrap_or("unavailable")
        ));
    }
    Ok(lines.join("\n"))
}
