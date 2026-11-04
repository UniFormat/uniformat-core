//! Byte sources addressable by range.
//!
//! A [`ByteSource`] is a read-only, stateless view of a file or buffer: any number of threads may read disjoint or
//! overlapping ranges at the same time. Format crates parse headers and pages through this trait, so the same code
//! reads a local file, a memory buffer or (in adapters) a remote object.

use std::borrow::Cow;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// A source of bytes readable by `(offset, length)` ranges without shared cursor state.
pub trait ByteSource {
    /// Total size in bytes.
    fn len(&self) -> u64;

    /// True if the source is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Reads exactly `len` bytes starting at `offset`.
    ///
    /// # Errors
    /// `UnexpectedEof` if the range exceeds the source.
    fn read_range(&self, offset: u64, len: usize) -> io::Result<Cow<'_, [u8]>>;
}

fn eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "range outside of the source")
}

/// In-memory source (zero copy).
#[derive(Debug, Clone, Copy)]
pub struct SliceSource<'a>(pub &'a [u8]);

impl ByteSource for SliceSource<'_> {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_range(&self, offset: u64, len: usize) -> io::Result<Cow<'_, [u8]>> {
        let start = usize::try_from(offset).map_err(|_| eof())?;
        let end = start.checked_add(len).ok_or_else(eof)?;
        self.0.get(start..end).map(Cow::Borrowed).ok_or_else(eof)
    }
}

impl ByteSource for Vec<u8> {
    fn len(&self) -> u64 {
        self.as_slice().len() as u64
    }

    fn read_range(&self, offset: u64, len: usize) -> io::Result<Cow<'_, [u8]>> {
        let start = usize::try_from(offset).map_err(|_| eof())?;
        let end = start.checked_add(len).ok_or_else(eof)?;
        self.get(start..end).map(Cow::Borrowed).ok_or_else(eof)
    }
}

/// A file read by position (`pread`): there is no shared cursor.
#[derive(Debug)]
pub struct FileSource {
    file: File,
    len: u64,
}

impl FileSource {
    /// Opens a file for reading.
    ///
    /// # Errors
    /// Any error while opening the file or reading its metadata.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Self { file, len })
    }
}

impl ByteSource for FileSource {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_range(&self, offset: u64, len: usize) -> io::Result<Cow<'_, [u8]>> {
        let end = offset.checked_add(len as u64).ok_or_else(eof)?;
        if end > self.len {
            return Err(eof());
        }
        let mut buf = vec![0u8; len];
        read_exact_at(&self.file, &mut buf, offset)?;
        Ok(Cow::Owned(buf))
    }
}

#[cfg(unix)]
fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, buf, offset)
}

#[cfg(windows)]
fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    while !buf.is_empty() {
        let n = std::os::windows::fs::FileExt::seek_read(file, buf, offset)?;
        if n == 0 {
            return Err(eof());
        }
        buf = &mut buf[n..];
        offset += n as u64;
    }
    Ok(())
}

/// Wrapper that counts the bytes and calls of a source (tests such as "the schema reads no data page", requests
/// made to a remote object, benchmarks).
#[derive(Debug)]
pub struct CountingSource<S> {
    inner: S,
    bytes: AtomicU64,
    reads: AtomicU64,
    ranges: Mutex<Vec<(u64, usize)>>,
}

impl<S> CountingSource<S> {
    /// Wraps `inner`.
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            bytes: AtomicU64::new(0),
            reads: AtomicU64::new(0),
            ranges: Mutex::new(Vec::new()),
        }
    }

    /// Bytes read since creation (or since the last reset).
    pub fn bytes_read(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    /// Number of `read_range` calls.
    pub fn reads(&self) -> u64 {
        self.reads.load(Ordering::Relaxed)
    }

    /// Requested `(offset, length)` ranges, in order.
    pub fn ranges(&self) -> Vec<(u64, usize)> {
        self.ranges.lock().map(|r| r.clone()).unwrap_or_default()
    }

    /// Resets the counters.
    pub fn reset(&self) {
        self.bytes.store(0, Ordering::Relaxed);
        self.reads.store(0, Ordering::Relaxed);
        if let Ok(mut r) = self.ranges.lock() {
            r.clear();
        }
    }

    /// The wrapped source.
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: ByteSource> ByteSource for CountingSource<S> {
    fn len(&self) -> u64 {
        self.inner.len()
    }

    fn read_range(&self, offset: u64, len: usize) -> io::Result<Cow<'_, [u8]>> {
        self.bytes.fetch_add(len as u64, Ordering::Relaxed);
        self.reads.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut r) = self.ranges.lock() {
            r.push((offset, len));
        }
        self.inner.read_range(offset, len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_source_ranges() {
        let s = SliceSource(&[1, 2, 3, 4]);
        assert_eq!(s.len(), 4);
        assert_eq!(&*s.read_range(1, 2).unwrap(), &[2, 3]);
        assert_eq!(&*s.read_range(4, 0).unwrap(), &[] as &[u8]);
        assert_eq!(
            s.read_range(3, 2).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        assert_eq!(
            s.read_range(u64::MAX, 1).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn counting_source_counts() {
        let s = CountingSource::new(vec![0u8; 100]);
        s.read_range(10, 20).unwrap();
        s.read_range(0, 5).unwrap();
        assert_eq!((s.bytes_read(), s.reads()), (25, 2));
        assert_eq!(s.ranges(), vec![(10, 20), (0, 5)]);
        s.reset();
        assert_eq!(s.bytes_read(), 0);
    }
}
