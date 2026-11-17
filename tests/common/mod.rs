#![allow(dead_code)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use flate2::Compression;
use flate2::write::GzEncoder;

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// A directory under the system temp directory, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "completion-dictionary-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The data root the binary uses with `XDG_DATA_HOME` set to this directory.
    pub fn data(&self) -> PathBuf {
        self.0.join("completion-dictionary")
    }

    pub fn write(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = self.data().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    pub fn seed(&self, relative: &str, fixture_name: &str) -> PathBuf {
        self.write(relative, &fs::read(fixture(fixture_name)).unwrap())
    }

    pub fn seed_swedish(&self) {
        self.seed("sources/hunspell-sv/sv_SE_expanded.dic", "sv_test.dic");
        self.seed("sources/mythes-sv/th_sv_SE.dat", "th_sv_test.dat");
        self.seed("sources/saldo/saldom.xml", "saldom_test.xml");
        self.seed("sources/wordfreq/large_sv.txt.zst", "freq_sv_test.txt.zst");
    }

    pub fn seed_english(&self) {
        self.write(
            "sources/oewn/english-wordnet-2025-plus.xml.gz",
            &gzip(&fs::read(fixture("oewn_test.xml")).unwrap()),
        );
        self.seed("sources/wordfreq/large_en.txt.zst", "freq_en_test.txt.zst");
    }

    /// Run the binary with this directory as `XDG_DATA_HOME`.
    pub fn run(&self, args: &[&str]) -> (i32, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_completion-dictionary"))
            .args(args)
            .env("XDG_DATA_HOME", &self.0)
            .output()
            .unwrap();
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

pub fn list_dir(path: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    entries
}
