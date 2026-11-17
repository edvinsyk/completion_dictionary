use std::error::Error;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const APP_NAME: &str = "completion-dictionary";

/// How long to wait on the network before giving up on a download.
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30);

pub fn data_root() -> PathBuf {
    if let Some(xdg_data_home) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(xdg_data_home).join(APP_NAME);
    }
    if cfg!(target_os = "macos") {
        return home()
            .join("Library")
            .join("Application Support")
            .join(APP_NAME);
    }
    if cfg!(windows) {
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
            return PathBuf::from(local_app_data).join(APP_NAME);
        }
        return home().join("AppData").join("Local").join(APP_NAME);
    }
    home().join(".local").join("share").join(APP_NAME)
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Replace `path` only after `write` has fully written its temporary sibling.
///
/// Readers see the old contents until the rename. A failed `write` leaves the
/// old file untouched and removes the temporary file.
pub fn atomic_output<T>(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<T>,
) -> io::Result<T> {
    let directory = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    fs::create_dir_all(directory)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("output path has no file name"))?
        .to_string_lossy();
    let (temporary_path, mut file) = create_temporary(directory, &name)?;

    let result = (|| {
        let value = write(&mut file)?;
        file.sync_all()?;
        drop(file);
        // Windows permissions can make a failed replacement's temporary file undeletable.
        #[cfg(unix)]
        if let Ok(metadata) = fs::metadata(path) {
            fs::set_permissions(&temporary_path, metadata.permissions())?;
        }
        fs::rename(&temporary_path, path)?;
        Ok(value)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn create_temporary(directory: &Path, name: &str) -> io::Result<(PathBuf, File)> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    loop {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let unique = format!(
            "{}{}{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
            nanos
        );
        let candidate = directory.join(format!(".{name}.{unique}.tmp"));
        match File::create_new(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Download `url` to `destination` unless it already exists.
///
/// The bytes stream into a temporary sibling first, so an interrupted download
/// cannot leave a corrupt source file. An existing destination is left
/// untouched (cache hit) unless `refresh` is true; a failed refresh keeps the
/// old copy.
pub fn ensure_download(url: &str, destination: &Path, refresh: bool) -> Result<PathBuf, String> {
    if destination.exists() && !refresh {
        return Ok(destination.to_path_buf());
    }
    download(url, destination).map_err(|error| format!("Could not download {url}: {error}"))?;
    Ok(destination.to_path_buf())
}

fn download(url: &str, destination: &Path) -> Result<(), Box<dyn Error>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(DOWNLOAD_TIMEOUT))
        .timeout_recv_response(Some(DOWNLOAD_TIMEOUT))
        .timeout_per_read(Some(DOWNLOAD_TIMEOUT))
        .build()
        .into();
    let mut response = agent.get(url).call()?;
    // The body reader fails if the connection closes before Content-Length bytes arrive.
    let mut body = response.body_mut().as_reader();
    atomic_output(destination, |file| io::copy(&mut body, file))?;
    Ok(())
}
