//! Small filesystem helpers shared by the vault, ingest and rehydrate.

use std::fs;
use std::io::Write;

use camino::Utf8Path;

use crate::Result;

/// Write `bytes` to `path` atomically: write a temp file in the same
/// directory, fsync it, rename it over the target, then fsync the directory.
/// On unix the file is created `0600`. A symlink at `path` is replaced, not
/// followed.
pub fn atomic_write(path: &Utf8Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Utf8Path::new("."));
    let name = path.file_name().unwrap_or("file");
    let tmp = dir.join(format!(".{name}.tmp-{}", uuid::Uuid::new_v4().simple()));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
        return result;
    }
    sync_dir(dir);
    Ok(())
}

/// Best-effort directory fsync so a rename survives a crash.
pub fn sync_dir(dir: &Utf8Path) {
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
}

/// True if `path` is `base` or below it. Both must already be canonical.
pub fn is_within(path: &Utf8Path, base: &Utf8Path) -> bool {
    path.starts_with(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    #[test]
    fn atomic_write_creates_and_replaces() {
        let d = tempfile::tempdir().unwrap();
        let p = Utf8PathBuf::from_path_buf(d.path().join("f.txt")).unwrap();
        atomic_write(&p, b"one").unwrap();
        atomic_write(&p, b"two").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
        // No temp files left behind.
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_replaces_symlink_instead_of_following() {
        let d = tempfile::tempdir().unwrap();
        let outside = d.path().join("outside.txt");
        fs::write(&outside, b"keep").unwrap();
        let link = Utf8PathBuf::from_path_buf(d.path().join("link")).unwrap();
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        atomic_write(&link, b"new").unwrap();
        assert_eq!(fs::read(&outside).unwrap(), b"keep");
        assert!(
            !fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn is_within_checks_components() {
        assert!(is_within(Utf8Path::new("/a/b/c"), Utf8Path::new("/a/b")));
        assert!(is_within(Utf8Path::new("/a/b"), Utf8Path::new("/a/b")));
        assert!(!is_within(Utf8Path::new("/a/bc"), Utf8Path::new("/a/b")));
    }

    #[test]
    fn sync_dir_tolerates_missing() {
        sync_dir(Utf8Path::new("/definitely/not/here"));
    }
}
