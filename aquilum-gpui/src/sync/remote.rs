use std::fmt;
use std::fs::File;
use std::io::{self, Read, Write};
use std::ops::Range;
use std::path::Path;

/// Minimal streaming content source used by the Dropbox client.
///
/// It exists instead of `Vec<u8>` so large accepted files can be uploaded and
/// hashed without materialising the whole file in memory.
pub trait Content {
    fn byte_len(&self) -> usize;
    fn read(&self, range: Range<usize>, visit: &mut dyn FnMut(&[u8]) -> io::Result<()>) -> io::Result<()>;
}

impl Content for [u8] {
    fn byte_len(&self) -> usize {
        self.len()
    }

    fn read(&self, range: Range<usize>, visit: &mut dyn FnMut(&[u8]) -> io::Result<()>) -> io::Result<()> {
        visit(&self[range])
    }
}

impl Content for str {
    fn byte_len(&self) -> usize {
        self.len()
    }

    fn read(&self, range: Range<usize>, visit: &mut dyn FnMut(&[u8]) -> io::Result<()>) -> io::Result<()> {
        self.as_bytes().read(range, visit)
    }
}

impl Content for String {
    fn byte_len(&self) -> usize {
        self.len()
    }

    fn read(&self, range: Range<usize>, visit: &mut dyn FnMut(&[u8]) -> io::Result<()>) -> io::Result<()> {
        self.as_str().read(range, visit)
    }
}

impl<T: Content + ?Sized> Content for &T {
    fn byte_len(&self) -> usize {
        (**self).byte_len()
    }

    fn read(&self, range: Range<usize>, visit: &mut dyn FnMut(&[u8]) -> io::Result<()>) -> io::Result<()> {
        (**self).read(range, visit)
    }
}

impl Content for Vec<u8> {
    fn byte_len(&self) -> usize {
        self.len()
    }

    fn read(&self, range: Range<usize>, visit: &mut dyn FnMut(&[u8]) -> io::Result<()>) -> io::Result<()> {
        self.as_slice().read(range, visit)
    }
}

/// A file opened for range reads; used for uploads and Dropbox content hashes.
pub struct FileContent {
    file: File,
    len: usize,
}

impl FileContent {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len() as usize;
        Ok(Self { file, len })
    }
}

impl Content for FileContent {
    fn byte_len(&self) -> usize {
        self.len
    }

    fn read(&self, range: Range<usize>, visit: &mut dyn FnMut(&[u8]) -> io::Result<()>) -> io::Result<()> {
        const BLOCK: usize = 128 * 1024;
        let mut buf = vec![0u8; BLOCK.min(range.len().max(1))];
        let mut at = range.start;
        while at < range.end {
            let take = (range.end - at).min(BLOCK);
            read_at(&self.file, &mut buf[..take], at as u64)?;
            visit(&buf[..take])?;
            at += take;
        }
        Ok(())
    }
}

#[cfg(unix)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.read_exact_at(buf, offset)
}

#[cfg(windows)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    let mut done = 0;
    while done < buf.len() {
        let n = file.seek_read(&mut buf[done..], offset + done as u64)?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "file is shorter than expected"));
        }
        done += n;
    }
    Ok(())
}

/// `Read` adapter over a ranged `Content`, for `ureq` request bodies.
pub struct ContentReader<'a> {
    content: &'a dyn Content,
    at: usize,
    end: usize,
}

impl<'a> ContentReader<'a> {
    pub fn range(content: &'a dyn Content, range: Range<usize>) -> Self {
        Self { content, at: range.start, end: range.end }
    }
}

impl Read for ContentReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.at >= self.end || out.is_empty() {
            return Ok(0);
        }
        let want = (self.end - self.at).min(out.len());
        let mut done = 0usize;
        self.content.read(self.at..self.at + want, &mut |bytes| {
            let take = bytes.len().min(want - done);
            out[done..done + take].copy_from_slice(&bytes[..take]);
            done += take;
            Ok(())
        })?;
        self.at += done;
        Ok(done)
    }
}

#[derive(Clone, Debug)]
pub struct RemoteFile {
    pub path: String,
    pub id: String,
    pub rev: String,
    pub checksum: String,
    pub size: u64,
    pub modified: u64,
}

pub struct Delta {
    pub changed: Vec<RemoteFile>,
    pub removed: Vec<String>,
    pub cursor: String,
}

#[derive(Debug)]
pub enum RemoteError {
    Auth,
    Stale,
    Missing,
    Offline(String),
    Failed(String),
}

impl RemoteError {
    pub fn transport(transport: ureq::Transport) -> Self {
        match transport.kind() {
            ureq::ErrorKind::Dns | ureq::ErrorKind::ConnectionFailed | ureq::ErrorKind::Io => {
                Self::Offline(transport.to_string())
            }
            _ => Self::Failed(transport.to_string()),
        }
    }
}

impl fmt::Display for RemoteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth => formatter.write_str("Dropbox rejected the sign-in; reconnect required"),
            Self::Stale => formatter.write_str("the cloud file changed during synchronization"),
            Self::Missing => formatter.write_str("the cloud file or revision no longer exists"),
            Self::Offline(message) | Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for RemoteError {}

impl From<io::Error> for RemoteError {
    fn from(error: io::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

pub type RemoteResult<T> = Result<T, RemoteError>;

pub trait Remote: Sync {
    fn cursor(&self) -> RemoteResult<String>;
    fn list(&self) -> RemoteResult<Vec<RemoteFile>>;
    fn delta(&self, cursor: &str) -> RemoteResult<Option<Delta>>;
    fn download(&self, file: &RemoteFile, out: &mut dyn Write) -> RemoteResult<()>;
    fn upload(
        &self,
        path: &str,
        content: &dyn Content,
        modified: u64,
        replaces: Option<&RemoteFile>,
    ) -> RemoteResult<RemoteFile>;
    fn delete(&self, file: &RemoteFile) -> RemoteResult<()>;
    fn checksum(&self, content: &dyn Content) -> io::Result<String>;
}
