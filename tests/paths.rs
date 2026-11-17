mod common;

use std::fs;
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::thread;

use common::{TempDir, list_dir};
use completion_dictionary::paths::{atomic_output, ensure_download};

#[test]
fn readers_see_old_contents_until_replacement() {
    let directory = TempDir::new();
    let destination = directory.path().join("output.dict");
    fs::write(&destination, "old").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o640)).unwrap();
    }
    let original_permissions = fs::metadata(&destination).unwrap().permissions();

    atomic_output(&destination, |file| {
        file.write_all(b"new")?;
        assert_eq!(fs::read_to_string(&destination).unwrap(), "old");
        Ok(())
    })
    .unwrap();

    assert_eq!(fs::read_to_string(&destination).unwrap(), "new");
    assert_eq!(
        fs::metadata(&destination).unwrap().permissions(),
        original_permissions
    );
    assert_eq!(list_dir(directory.path()), vec![destination]);
}

#[test]
fn failed_write_preserves_old_contents() {
    let directory = TempDir::new();
    let destination = directory.path().join("index.tsv");
    fs::write(&destination, "old").unwrap();

    let error = atomic_output(&destination, |file| {
        file.write_all(b"for")?;
        Err::<(), _>(io::Error::other("disk full"))
    })
    .unwrap_err();

    assert_eq!(error.to_string(), "disk full");
    assert_eq!(fs::read_to_string(&destination).unwrap(), "old");
    assert_eq!(list_dir(directory.path()), vec![destination]);
}

#[test]
fn overlapping_writers_use_distinct_temporary_files() {
    let directory = TempDir::new();
    let destination = directory.path().join("nested").join("output.dict");
    atomic_output(&destination, |first| {
        first.write_all(b"first")?;
        atomic_output(&destination, |second| second.write_all(b"second"))?;
        assert_eq!(fs::read_to_string(&destination).unwrap(), "second");
        Ok(())
    })
    .unwrap();
    assert_eq!(fs::read_to_string(&destination).unwrap(), "first");
    assert_eq!(
        list_dir(destination.parent().unwrap()),
        vec![destination.clone()]
    );
}

#[test]
fn data_root_prefers_xdg_data_home() {
    let directory = TempDir::new();
    let (code, stdout, _) = directory.run(&["path", "--profile", "english"]);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        format!("{}\n", directory.data().join("en-oewn.dict").display())
    );
}

/// Serve `response` once to each of `connections` requests, then close.
fn serve(response: &'static [u8], connections: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/source", listener.local_addr().unwrap());
    thread::spawn(move || {
        for stream in listener.incoming().take(connections) {
            let mut stream = stream.unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
            }
            stream.write_all(response).unwrap();
        }
    });
    url
}

fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/source", listener.local_addr().unwrap());
    drop(listener);
    url
}

fn no_temporary_files(directory: &TempDir) {
    assert!(
        list_dir(directory.path())
            .iter()
            .all(|p| !p.to_string_lossy().ends_with(".tmp")),
        "temporary file left behind"
    );
}

#[test]
fn downloads_to_destination() {
    let directory = TempDir::new();
    let destination = directory.path().join("sources").join("source.dic");
    let url = serve(
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nnew",
        1,
    );

    assert_eq!(
        ensure_download(&url, &destination, false).unwrap(),
        destination
    );
    assert_eq!(fs::read(&destination).unwrap(), b"new");
    assert_eq!(
        list_dir(destination.parent().unwrap()),
        vec![destination.clone()]
    );
}

#[test]
fn download_without_content_length_reads_until_close() {
    let directory = TempDir::new();
    let destination = directory.path().join("source.dic");
    fs::write(&destination, "old").unwrap();
    let url = serve(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nnew", 1);

    ensure_download(&url, &destination, true).unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"new");
    no_temporary_files(&directory);
}

#[test]
fn skips_download_when_destination_already_exists() {
    let directory = TempDir::new();
    let destination = directory.path().join("th_sv_SE.dat");
    fs::write(&destination, "cached").unwrap();

    ensure_download(&closed_port_url(), &destination, false).unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"cached");
}

#[test]
fn short_body_fails_and_preserves_cache() {
    let url = serve(
        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nnew",
        2,
    );
    for cached in [true, false] {
        let directory = TempDir::new();
        let destination = directory.path().join("source.dic");
        if cached {
            fs::write(&destination, "old").unwrap();
        }

        let error = ensure_download(&url, &destination, true).unwrap_err();

        assert!(
            error.starts_with(&format!("Could not download {url}: ")),
            "{error}"
        );
        if cached {
            assert_eq!(fs::read(&destination).unwrap(), b"old");
        } else {
            assert!(!destination.exists());
        }
        no_temporary_files(&directory);
    }
}

#[test]
fn network_failure_keeps_cache_without_temp_file() {
    let directory = TempDir::new();
    let destination = directory.path().join("th_sv_SE.dat");
    fs::write(&destination, "old").unwrap();

    let error = ensure_download(&closed_port_url(), &destination, true).unwrap_err();

    assert!(error.starts_with("Could not download"), "{error}");
    assert_eq!(fs::read(&destination).unwrap(), b"old");
    assert_eq!(list_dir(directory.path()), vec![destination]);
}

#[test]
fn http_error_status_fails() {
    let directory = TempDir::new();
    let destination = directory.path().join("source.dic");
    let url = serve(
        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        1,
    );

    assert!(ensure_download(&url, &destination, false).is_err());
    assert!(!destination.exists());
}
