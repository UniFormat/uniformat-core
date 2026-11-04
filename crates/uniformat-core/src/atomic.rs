//! Transactional file creation: write to a temporary file in the same directory, rename on commit.

use std::fs::{self, File};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A temporary file renamed to its destination by [`AtomicFile::commit`]; removed if dropped without commit.
///
/// A failed or abandoned write therefore never leaves a partial file at the destination, and an existing
/// destination is untouched until the commit.
#[derive(Debug)]
pub struct AtomicFile {
    file: Option<File>,
    tmp: PathBuf,
    dest: PathBuf,
}

impl AtomicFile {
    /// Creates the temporary file next to `dest`.
    ///
    /// # Errors
    /// Any error while creating the temporary file.
    pub fn create(dest: impl AsRef<Path>) -> io::Result<Self> {
        let dest = dest.as_ref();
        let dir = dest.parent().filter(|d| !d.as_os_str().is_empty());
        let name = dest
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "table".to_owned());
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let tmp_name = format!(".{name}.{}.{nonce}.tmp", std::process::id());
        let tmp = match dir {
            Some(d) => d.join(tmp_name),
            None => PathBuf::from(tmp_name),
        };
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        Ok(Self {
            file: Some(file),
            tmp,
            dest: dest.to_path_buf(),
        })
    }

    /// Flushes the file to disk and renames it to the destination.
    ///
    /// # Errors
    /// Any error while syncing or renaming (the temporary file is then removed).
    pub fn commit(mut self) -> io::Result<()> {
        if let Some(f) = self.file.take() {
            f.sync_all()?;
        }
        fs::rename(&self.tmp, &self.dest)?;
        self.tmp = PathBuf::new();
        Ok(())
    }

    fn file(&mut self) -> io::Result<&mut File> {
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("file already closed"))
    }
}

impl Write for AtomicFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file()?.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file()?.flush()
    }
}

impl Seek for AtomicFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file()?.seek(pos)
    }
}

impl Drop for AtomicFile {
    fn drop(&mut self) {
        if !self.tmp.as_os_str().is_empty() {
            self.file.take();
            // abandoned or failed: no partial file is left behind
            let _ = fs::remove_file(&self.tmp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_publishes_and_drop_cleans_up() {
        let dir =
            std::env::temp_dir().join(format!("uniformat-core-atomic-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("t.bin");
        {
            let mut f = AtomicFile::create(&dest).unwrap();
            f.write_all(b"partial").unwrap();
        }
        assert!(!dest.exists());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let mut f = AtomicFile::create(&dest).unwrap();
        f.write_all(b"done").unwrap();
        f.commit().unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"done");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
