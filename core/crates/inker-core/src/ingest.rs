//! Moving or copying a dropped file or folder into the private folder.
//!
//! Pure `std::fs`. Extraction and detection are orchestrated elsewhere.
//! Rules (decision 5.10): cross-volume moves are copy, verify size, delete; a
//! path already inside the private folder is indexed in place; folders move as
//! a unit; nothing is ever moved out of the private folder; an existing name
//! is never overwritten (`w2.pdf` becomes `w2-2.pdf`).
//!
//! Error messages describe the failure and never name the file: they reach
//! logs through the pipeline's outcome, and file names are often personal.

use std::fs;
use std::io;

use camino::{Utf8Path, Utf8PathBuf};

use crate::fsutil::is_within;
use crate::{Error, IngestMode, Result};

/// Where a dropped item ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestOutcome {
    /// New location, inside the private folder (canonical).
    pub path: Utf8PathBuf,
    /// The item was already inside the private folder and was not moved.
    pub in_place: bool,
    pub is_dir: bool,
}

fn utf8(p: std::path::PathBuf) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(p).map_err(|_| Error::Ingest("path is not valid UTF-8".into()))
}

fn canonical(p: &Utf8Path) -> Result<Utf8PathBuf> {
    utf8(
        fs::canonicalize(p)
            .map_err(|e| Error::Ingest(format!("cannot resolve the source: {e}")))?,
    )
}

/// Bring `src` into `private_folder`, by move or copy.
pub fn ingest(
    private_folder: &Utf8Path,
    src: &Utf8Path,
    mode: IngestMode,
) -> Result<IngestOutcome> {
    fs::create_dir_all(private_folder)?;
    let private = canonical(private_folder)?;
    let meta = fs::symlink_metadata(src)
        .map_err(|e| Error::Ingest(format!("cannot read the source: {e}")))?;
    if meta.file_type().is_symlink() {
        return Err(Error::Ingest(
            "source is a symbolic link; drop the file it points to".into(),
        ));
    }
    if !meta.is_file() && !meta.is_dir() {
        return Err(Error::Ingest(
            "source is not a regular file or folder".into(),
        ));
    }
    let src = canonical(src)?;
    let is_dir = meta.is_dir();

    if is_within(&private, &src) {
        return Err(Error::Refused(
            "cannot move the private folder or a folder that contains it".into(),
        ));
    }
    if is_within(&src, &private) {
        return Ok(IngestOutcome {
            path: src,
            in_place: true,
            is_dir,
        });
    }

    let name = src
        .file_name()
        .ok_or_else(|| Error::Ingest("source has no file name".into()))?;
    let dest = unique_destination(&private, name);
    match mode {
        IngestMode::Copy => copy_tree(&src, &dest)?,
        IngestMode::Move => match fs::rename(&src, &dest) {
            Ok(()) => crate::fsutil::sync_dir(&private),
            Err(e) if e.kind() == io::ErrorKind::CrossesDevices => move_by_copy(&src, &dest)?,
            Err(e) => return Err(Error::Ingest(format!("move failed: {e}"))),
        },
    }
    tracing::info!(is_dir, ?mode, "ingested into private folder");
    Ok(IngestOutcome {
        path: dest,
        in_place: false,
        is_dir,
    })
}

/// Cross-volume move: copy, verify sizes, then delete the source. If the
/// copy cannot be verified, the copy is removed and the source kept.
pub fn move_by_copy(src: &Utf8Path, dest: &Utf8Path) -> Result<()> {
    copy_tree(src, dest)?;
    if fs::symlink_metadata(src)?.is_dir() {
        fs::remove_dir_all(src)?;
    } else {
        fs::remove_file(src)?;
    }
    Ok(())
}

/// Copy a file or a folder tree to `dest` (which must not exist) and verify
/// every copied file's size. Symbolic links inside a folder are not followed
/// or copied.
pub fn copy_tree(src: &Utf8Path, dest: &Utf8Path) -> Result<()> {
    if fs::symlink_metadata(dest).is_ok() {
        return Err(Error::Ingest("destination exists".into()));
    }
    let result = copy_inner(src, dest);
    if result.is_err() {
        let _ = if dest.is_dir() {
            fs::remove_dir_all(dest)
        } else {
            fs::remove_file(dest)
        };
    }
    result
}

fn copy_inner(src: &Utf8Path, dest: &Utf8Path) -> Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.is_dir() {
        fs::create_dir(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| Error::Ingest("file name is not valid UTF-8".into()))?;
            let ft = entry.file_type()?;
            if ft.is_symlink() {
                tracing::warn!("skipping a symbolic link inside a dropped folder");
                continue;
            }
            copy_inner(&src.join(name), &dest.join(name))?;
        }
    } else if meta.is_file() {
        if dest.exists() {
            return Err(Error::Ingest("destination exists".into()));
        }
        fs::copy(src, dest)?;
        fs::File::open(dest)?.sync_all()?;
        let copied = fs::metadata(dest)?.len();
        if copied != meta.len() {
            return Err(Error::Ingest(format!(
                "copy is {copied} bytes, expected {}",
                meta.len()
            )));
        }
    }
    Ok(())
}

/// `dir/name` if free, else `dir/stem-2.ext`, `dir/stem-3.ext`, ...
pub fn unique_destination(dir: &Utf8Path, name: &str) -> Utf8PathBuf {
    let first = dir.join(name);
    if fs::symlink_metadata(&first).is_err() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, Some(e)),
        _ => (name, None),
    };
    let mut n = 2u32;
    loop {
        let candidate = match ext {
            Some(e) => dir.join(format!("{stem}-{n}.{e}")),
            None => dir.join(format!("{stem}-{n}")),
        };
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
        n += 1;
    }
}

/// Regular files under `dir`, recursively, sorted. Hidden files (`.DS_Store`)
/// and symbolic links are skipped.
pub fn list_files(dir: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let mut out = Vec::new();
    walk(dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Utf8Path, out: &mut Vec<Utf8PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = utf8(entry.path())?;
        if path.file_name().is_some_and(|n| n.starts_with('.')) {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_dir() {
            walk(&path, out)?;
        } else if ft.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use std::fs;

    struct Fx {
        _d: tempfile::TempDir,
        private: Utf8PathBuf,
        outside: Utf8PathBuf,
    }

    fn fx() -> Fx {
        let d = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(d.path().canonicalize().unwrap()).unwrap();
        let private = root.join("Private");
        let outside = root.join("Downloads");
        fs::create_dir_all(&private).unwrap();
        fs::create_dir_all(&outside).unwrap();
        Fx {
            _d: d,
            private,
            outside,
        }
    }

    #[test]
    fn move_file_in() {
        let f = fx();
        let src = f.outside.join("w2.txt");
        fs::write(&src, "x").unwrap();
        let out = ingest(&f.private, &src, IngestMode::Move).unwrap();
        assert_eq!(out.path, f.private.join("w2.txt"));
        assert!(!out.in_place && !out.is_dir);
        assert!(!src.exists());
        assert_eq!(fs::read_to_string(&out.path).unwrap(), "x");
    }

    #[test]
    fn copy_file_in_keeps_original_and_avoids_clobber() {
        let f = fx();
        let src = f.outside.join("w2.txt");
        fs::write(&src, "new").unwrap();
        fs::write(f.private.join("w2.txt"), "old").unwrap();
        let out = ingest(&f.private, &src, IngestMode::Copy).unwrap();
        assert_eq!(out.path, f.private.join("w2-2.txt"));
        assert!(src.exists());
        assert_eq!(fs::read_to_string(f.private.join("w2.txt")).unwrap(), "old");
    }

    #[test]
    fn unique_destination_variants() {
        let f = fx();
        assert_eq!(
            unique_destination(&f.private, "a.pdf"),
            f.private.join("a.pdf")
        );
        fs::write(f.private.join("a.pdf"), "").unwrap();
        fs::write(f.private.join("a-2.pdf"), "").unwrap();
        assert_eq!(
            unique_destination(&f.private, "a.pdf"),
            f.private.join("a-3.pdf")
        );
        fs::create_dir(f.private.join("dir")).unwrap();
        assert_eq!(
            unique_destination(&f.private, "dir"),
            f.private.join("dir-2")
        );
    }

    #[test]
    fn already_inside_is_indexed_in_place() {
        let f = fx();
        fs::create_dir(f.private.join("sub")).unwrap();
        let p = f.private.join("sub/x.txt");
        fs::write(&p, "x").unwrap();
        let out = ingest(&f.private, &p, IngestMode::Move).unwrap();
        assert!(out.in_place);
        assert_eq!(out.path, p);
    }

    #[test]
    fn folder_moves_as_unit() {
        let f = fx();
        let dir = f.outside.join("Taxes 2025");
        fs::create_dir_all(dir.join("inner")).unwrap();
        fs::write(dir.join("a.txt"), "a").unwrap();
        fs::write(dir.join("inner/b.txt"), "bb").unwrap();
        fs::write(dir.join(".DS_Store"), "junk").unwrap();
        let out = ingest(&f.private, &dir, IngestMode::Move).unwrap();
        assert!(out.is_dir);
        assert_eq!(out.path, f.private.join("Taxes 2025"));
        assert!(!dir.exists());
        let files = list_files(&out.path).unwrap();
        assert_eq!(
            files,
            vec![out.path.join("a.txt"), out.path.join("inner/b.txt")]
        );
    }

    #[test]
    fn refuses_private_folder_itself_or_ancestor() {
        let f = fx();
        assert!(matches!(
            ingest(&f.private, &f.private, IngestMode::Move),
            Err(Error::Refused(_))
        ));
        let parent = f.private.parent().unwrap().to_owned();
        assert!(matches!(
            ingest(&f.private, &parent, IngestMode::Move),
            Err(Error::Refused(_))
        ));
    }

    #[test]
    fn missing_source_is_an_ingest_error() {
        let f = fx();
        assert!(matches!(
            ingest(&f.private, &f.outside.join("nope"), IngestMode::Move),
            Err(Error::Ingest(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_source() {
        let f = fx();
        let real = f.outside.join("real.txt");
        fs::write(&real, "x").unwrap();
        let link = f.outside.join("link.txt");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(matches!(
            ingest(&f.private, &link, IngestMode::Move),
            Err(Error::Ingest(_))
        ));
    }

    fn assert_no_path(e: &Error) {
        let msg = e.to_string();
        for part in ["McFakename", "Downloads", "Private", ".txt", "/"] {
            assert!(!msg.contains(part), "{part:?} in {msg:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn ingest_errors_carry_no_path() {
        let f = fx();
        let missing = f.outside.join("Testy McFakename W2.txt");
        assert_no_path(&ingest(&f.private, &missing, IngestMode::Move).unwrap_err());

        let real = f.outside.join("Testy McFakename real.txt");
        fs::write(&real, "x").unwrap();
        let link = f.outside.join("Testy McFakename link.txt");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_no_path(&ingest(&f.private, &link, IngestMode::Move).unwrap_err());

        let dest = f.private.join("Testy McFakename dest.txt");
        fs::write(&dest, "keep").unwrap();
        assert_no_path(&copy_tree(&real, &dest).unwrap_err());
    }

    #[test]
    fn move_by_copy_verifies_and_deletes() {
        let f = fx();
        let dir = f.outside.join("d");
        fs::create_dir_all(dir.join("x")).unwrap();
        fs::write(dir.join("x/1.txt"), "12345").unwrap();
        let dest = f.private.join("d");
        move_by_copy(&dir, &dest).unwrap();
        assert!(!dir.exists());
        assert_eq!(fs::read_to_string(dest.join("x/1.txt")).unwrap(), "12345");

        let file = f.outside.join("f.txt");
        fs::write(&file, "abc").unwrap();
        move_by_copy(&file, &f.private.join("f.txt")).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn copy_tree_never_touches_an_existing_destination() {
        let f = fx();
        let src = f.outside.join("s.txt");
        fs::write(&src, "new").unwrap();
        let dest = f.private.join("s.txt");
        fs::write(&dest, "precious").unwrap();
        assert!(copy_tree(&src, &dest).is_err());
        assert_eq!(fs::read_to_string(&dest).unwrap(), "precious");
    }

    #[test]
    fn copy_tree_copies_without_deleting() {
        let f = fx();
        let dir = f.outside.join("d");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("1.txt"), "1").unwrap();
        copy_tree(&dir, &f.private.join("d")).unwrap();
        assert!(dir.join("1.txt").exists());
        assert!(f.private.join("d/1.txt").exists());
    }
}
