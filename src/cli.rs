use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::paths::data_root;
use crate::{english, swedish};

#[derive(Parser)]
#[command(
    name = "completion-dictionary",
    about = "Build language dictionaries and documentation backends for blink-cmp-dictionary."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate the newline-delimited dictionary file for a profile.
    Build {
        #[command(flatten)]
        profile: ProfileArg,
        /// Where to write the dictionary file. Defaults to the profile's standard path.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Redownload upstream dictionary data before building (swedish only).
        #[arg(long)]
        refresh: bool,
    },
    /// Print the default dictionary path for a profile.
    Path {
        #[command(flatten)]
        profile: ProfileArg,
        /// Echo a custom output path instead of the default.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Show a learning/debug view for a lemma.
    Lookup {
        #[command(flatten)]
        profile: ProfileArg,
        /// The lemma to inspect.
        word: String,
    },
    /// Print the documentation text for blink-cmp-dictionary.
    Doc {
        #[command(flatten)]
        profile: ProfileArg,
        /// The lemma to document.
        word: String,
    },
    /// Print synonym groups for a word as JSON, for editor pickers.
    Synonyms {
        #[command(flatten)]
        profile: ProfileArg,
        /// The word to find synonyms for.
        word: String,
    },
}

#[derive(Args)]
struct ProfileArg {
    /// Which dictionary profile to use.
    #[arg(long, value_enum, default_value_t = Profile::Swedish)]
    profile: Profile,
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum Profile {
    English,
    Swedish,
}

impl Profile {
    fn name(self) -> &'static str {
        match self {
            Profile::English => "english",
            Profile::Swedish => "swedish",
        }
    }

    fn default_output(self) -> PathBuf {
        data_root().join(match self {
            Profile::English => "en-oewn.dict",
            Profile::Swedish => "sv-hunspell.dict",
        })
    }
}

fn fail(message: &str) -> i32 {
    eprintln!("{message}");
    1
}

/// English senses from the index written by `build`; none before a build.
fn english_senses(query: &str) -> io::Result<Vec<english::Sense>> {
    english::lookup(&english::index_path(), query)
}

fn build(profile: Profile, output: Option<PathBuf>, refresh: bool) -> i32 {
    let output = output.unwrap_or_else(|| profile.default_output());
    let result = match profile {
        Profile::Swedish => swedish::build(&output, refresh),
        Profile::English if refresh => {
            return fail("--refresh is only supported for the swedish profile.");
        }
        Profile::English => english::build(&output),
    };
    match result {
        Ok(()) => 0,
        Err(message) => fail(&message),
    }
}

fn run_lookup(profile: Profile, query: &str) -> io::Result<()> {
    if profile == Profile::Swedish {
        let index = swedish::lemma_index_path();
        let (in_dictionary, groups) = swedish::lookup_word(
            query,
            &profile.default_output(),
            &swedish::thesaurus_source_path(),
            Some(&index),
        )?;
        println!("Profile: {}", profile.name());
        let yes_no = |value: bool| if value { "yes" } else { "no" };
        println!("Word: {query}");
        println!("Completion dictionary: {}", yes_no(in_dictionary));
        println!(
            "Thesaurus: {}",
            yes_no(groups.as_ref().is_some_and(|g| !g.is_empty()))
        );
        if let Some(groups) = groups.filter(|g| !g.is_empty()) {
            println!();
            println!("Synonyms:");
            for synonym in groups.iter().flatten() {
                println!("  {synonym}");
            }
        }
        return Ok(());
    }

    let senses = english_senses(query)?;
    println!("Profile: {}", profile.name());
    println!("Lemma: {query}");
    println!("Matches: {} sense(s)", senses.len());
    for (index, sense) in senses.iter().enumerate() {
        println!();
        println!("Sense {}", index + 1);
        println!("  Lemma: {}", sense.lemma);
        println!("  Part of speech: {}", english::format_pos(&sense.pos));
        println!("  Synonyms: {}", sense.siblings.join(", "));
        println!("  ILI: {}", sense.ili.as_deref().unwrap_or("n/a"));
        println!(
            "  English gloss: {}",
            sense.gloss.as_deref().unwrap_or("n/a")
        );
    }
    Ok(())
}

/// One synonym group: the words, the part of speech and the gloss.
type Group = (Vec<String>, Option<String>, Option<String>);

/// Return synonym groups for `query`, without the query itself.
///
/// Swedish groups have no part of speech or gloss. Groups left with no words
/// are dropped.
fn synonym_groups(profile: Profile, query: &str) -> io::Result<Vec<Group>> {
    let groups: Vec<Group> = match profile {
        Profile::Swedish => {
            let index = swedish::lemma_index_path();
            swedish::lookup_synonyms(query, &swedish::thesaurus_source_path(), Some(&index))?
                .unwrap_or_default()
                .into_iter()
                .map(|words| (words, None, None))
                .collect()
        }
        Profile::English => english_senses(query)?
            .into_iter()
            .map(|sense| {
                (
                    sense.siblings,
                    Some(english::format_pos(&sense.pos).to_string()),
                    sense.gloss,
                )
            })
            .collect(),
    };
    let query = crate::frequency::casefold(query);
    Ok(groups
        .into_iter()
        .map(|(words, pos, gloss)| {
            let words: Vec<String> = words
                .into_iter()
                .filter(|word| crate::frequency::casefold(word) != query)
                .collect();
            (words, pos, gloss)
        })
        .filter(|(words, _, _)| !words.is_empty())
        .collect())
}

/// Format like Python's `json.dumps(..., ensure_ascii=False)`, with `", "` and `": "`.
fn groups_json(groups: &[Group]) -> String {
    let string = |s: &str| serde_json::to_string(s).expect("strings always serialize");
    let optional = |s: &Option<String>| s.as_deref().map_or("null".to_string(), string);
    let items: Vec<String> = groups
        .iter()
        .map(|(words, pos, gloss)| {
            let words: Vec<String> = words.iter().map(|w| string(w)).collect();
            format!(
                "{{\"words\": [{}], \"pos\": {}, \"gloss\": {}}}",
                words.join(", "),
                optional(pos),
                optional(gloss)
            )
        })
        .collect();
    format!("[{}]", items.join(", "))
}

pub fn run(args: impl IntoIterator<Item = impl Into<OsString> + Clone>) -> i32 {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return error.exit_code();
        }
    };
    let result = match cli.command {
        Command::Build {
            profile,
            output,
            refresh,
        } => return build(profile.profile, output, refresh),
        Command::Path { profile, output } => {
            println!(
                "{}",
                output
                    .unwrap_or_else(|| profile.profile.default_output())
                    .display()
            );
            return 0;
        }
        Command::Lookup { profile, word } => run_lookup(profile.profile, &word),
        Command::Doc { profile, word } => {
            let output = match profile.profile {
                Profile::English => english::render_doc(&word),
                Profile::Swedish => swedish::render_doc(&word),
            };
            output.map(|output| {
                if !output.is_empty() {
                    println!("{output}");
                }
            })
        }
        Command::Synonyms { profile, word } => synonym_groups(profile.profile, &word)
            .map(|groups| println!("{}", groups_json(&groups))),
    };
    match result {
        Ok(()) => 0,
        Err(error) => fail(&error.to_string()),
    }
}
