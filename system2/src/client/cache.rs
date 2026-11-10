use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

#[derive(Debug)]
pub(crate) struct Cache {
    path: PathBuf,

    // one lock for both, so a request asked twice at once gets one line
    entries: Mutex<Entries>,
}

#[derive(Debug)]
struct Entries {
    // `None` for a file that the client can only read
    writer: Option<File>,
    responses: HashMap<[u8; 32], Value>,
}

// a new response, which goes into the cache only after its answers pass
#[derive(Debug)]
pub(crate) struct PendingLine {
    cache: Arc<Cache>,
    line: Line<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Line<R> {
    #[serde(with = "hex::serde")]
    request_sha256: [u8; 32],
    response: R,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CacheError {
    #[error("the client cannot read the cache file `{}`{}", path.display(), read_fix(source))]
    Read { path: PathBuf, source: io::Error },
    #[error("the client cannot add to the cache file `{}`", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error(
        "line {line} of the cache file `{}` must be the hash of a request and its response",
        path.display()
    )]
    InvalidLine {
        path: PathBuf,
        line: usize,
        source: serde_json::Error,
    },
}

impl Cache {
    pub(crate) fn open(path: PathBuf) -> Result<Cache, CacheError> {
        let read_error = |source| CacheError::Read {
            path: path.clone(),
            source,
        };
        let open = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&path);
        let (file, writable) = match open {
            Ok(file) => (file, true),

            // a file that the client can only read still gives its responses
            Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                let file = File::open(&path).map_err(|_| read_error(error))?;
                (file, false)
            }
            Err(error) => return Err(read_error(error)),
        };

        // the first line of a request wins, as it does in `write`
        let mut reader = BufReader::new(&file);
        let mut bytes = Vec::new();
        let mut line = 0;
        let mut ends_with_newline = true;
        let mut responses = HashMap::new();
        while reader.read_until(b'\n', &mut bytes).map_err(read_error)? > 0 {
            line += 1;
            ends_with_newline = bytes.ends_with(b"\n");
            let (request_sha256, response) =
                parse_line(&bytes).map_err(|source| CacheError::InvalidLine {
                    path: path.clone(),
                    line,
                    source,
                })?;
            responses.entry(request_sha256).or_insert(response);
            bytes.clear();
        }
        let mut entries = Entries {
            writer: writable.then_some(file),
            responses,
        };

        // an editor can leave the last line with no newline, and the next line must not join it
        if !ends_with_newline && entries.writer.is_some() {
            entries.append(&path, "\n")?;
        }
        let entries = Mutex::new(entries);
        Ok(Cache { path, entries })
    }

    pub(crate) fn get(&self, request_sha256: &[u8; 32]) -> Option<Value> {
        self.lock().responses.get(request_sha256).cloned()
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    fn lock(&self) -> MutexGuard<'_, Entries> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Entries {
    fn append(&mut self, path: &Path, text: &str) -> Result<(), CacheError> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::from(ErrorKind::PermissionDenied));
        writer
            .and_then(|writer| writer.write_all(text.as_bytes()))
            .map_err(|source| CacheError::Write {
                path: path.to_owned(),
                source,
            })
    }
}

impl PendingLine {
    pub(crate) fn new(cache: Arc<Cache>, request_sha256: [u8; 32], response: Value) -> PendingLine {
        let line = Line {
            request_sha256,
            response,
        };
        PendingLine { cache, line }
    }

    #[expect(
        clippy::expect_used,
        reason = "a hash and a json value always serialize"
    )]
    pub(crate) fn write(self) -> Result<(), CacheError> {
        let PendingLine { cache, line } = self;
        let mut entries = cache.lock();

        // a request already in the cache, maybe from a twin asked at the same time, adds no line
        if entries.responses.contains_key(&line.request_sha256) {
            return Ok(());
        }
        let text = serde_json::to_string(&line).expect("the line serializes");

        // one write, as `writeln!` writes the newline apart, and another writer can come between
        entries.append(&cache.path, &format!("{text}\n"))?;
        entries.responses.insert(line.request_sha256, line.response);
        Ok(())
    }
}

pub(crate) fn sha256(body: &[u8]) -> [u8; 32] {
    Sha256::digest(body).into()
}

fn parse_line(bytes: &[u8]) -> Result<([u8; 32], Value), serde_json::Error> {
    let Line {
        request_sha256,
        response,
    } = serde_json::from_slice::<Line<&RawValue>>(bytes)?;

    // the response parses alone, as the line around it is one more level for the limit of depth
    let response = serde_json::from_str(response.get())?;
    Ok((request_sha256, response))
}

// a missing file is made, so only a missing folder is not found
fn read_fix(source: &io::Error) -> &'static str {
    if source.kind() == ErrorKind::NotFound {
        ": make its folder first"
    } else {
        ""
    }
}
