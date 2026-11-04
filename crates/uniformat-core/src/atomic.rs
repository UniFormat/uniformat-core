//! Transactional file creation with private temporary files in the destination directory.

use std::fs;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

/// A private temporary file renamed to its destination by [`AtomicFile::commit`].
///
/// New files are private (0600 on Unix). Replacements preserve ordinary Unix permission bits,
/// but do not copy ownership, special mode bits or extended ACLs. The destination directory must
/// be trusted: atomic replacement cannot protect against another writer changing that directory.
/// A failed or abandoned write leaves the destination untouched and removes the temporary file.
#[derive(Debug)]
pub struct AtomicFile {
    file: NamedTempFile,
    dest: PathBuf,
}

impl AtomicFile {
    /// Creates a private, exclusively created temporary file next to `dest`.
    ///
    /// # Errors
    /// Any error while creating the temporary file.
    pub fn create(dest: impl AsRef<Path>) -> io::Result<Self> {
        let dest = dest.as_ref();
        let dir = dest
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        Ok(Self {
            file: tempfile::Builder::new()
                .prefix(".uniformat-")
                .tempfile_in(dir)?,
            dest: dest.to_path_buf(),
        })
    }

    /// Flushes and atomically replaces the destination, retaining its ordinary permissions.
    ///
    /// # Errors
    /// I/O errors, or a destination that exists and is not a regular file (including symlinks).
    pub fn commit(self) -> io::Result<()> {
        // Read permissions at commit, not at create: a caller may tighten them while writing.
        match fs::symlink_metadata(&self.dest) {
            Ok(meta) => {
                if !meta.file_type().is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "destination is not a regular file",
                    ));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    self.file
                        .as_file()
                        .set_permissions(fs::Permissions::from_mode(
                            meta.permissions().mode() & 0o777,
                        ))?;
                }
                #[cfg(not(unix))]
                self.file.as_file().set_permissions(meta.permissions())?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        self.file.as_file().sync_all()?;
        self.file.persist(&self.dest).map_err(|e| e.error)?;
        Ok(())
    }
}

impl Write for AtomicFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Seek for AtomicFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_publishes_and_drop_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("t.bin");
        {
            let mut f = AtomicFile::create(&dest).unwrap();
            f.write_all(b"partial").unwrap();
        }
        assert!(!dest.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        let mut f = AtomicFile::create(&dest).unwrap();
        f.write_all(b"done").unwrap();
        f.commit().unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"done");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn temporary_files_are_private_and_replacements_keep_restrictive_modes() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("private.bin");
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let f = AtomicFile::create(&dest).unwrap();
        assert_eq!(mode(f.file.path()) & 0o077, 0);
        f.commit().unwrap();
        assert_eq!(mode(&dest) & 0o077, 0);
        for permissions in [0o600, 0o640, 0o400] {
            fs::set_permissions(&dest, fs::Permissions::from_mode(0o644)).unwrap();
            let mut f = AtomicFile::create(&dest).unwrap();
            f.write_all(b"private replacement").unwrap();
            // Even a restriction made after create must survive the replacement.
            fs::set_permissions(&dest, fs::Permissions::from_mode(permissions)).unwrap();
            assert_eq!(mode(f.file.path()) & 0o077, 0);
            f.commit().unwrap();
            assert_eq!(mode(&dest), permissions);
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_destination_is_rejected_and_temporary_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let dest = dir.path().join("link");
        fs::write(&target, b"unchanged").unwrap();
        std::os::unix::fs::symlink(&target, &dest).unwrap();
        let mut f = AtomicFile::create(&dest).unwrap();
        f.write_all(b"replacement").unwrap();
        assert!(f.commit().is_err());
        assert_eq!(fs::read(target).unwrap(), b"unchanged");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
