//! `write`: turn an agent's tokenized content back into real values and save
//! it, only ever under the private folder.
//!
//! Refused with [`Error::Refused`]: absolute names, `.` or `..` segments, empty
//! segments, a target directory outside the private folder, any symbolic link
//! on the way (even one pointing inside), and a target that is a directory.
//! An existing file is replaced only when the caller allows it.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use crate::codename::propose_output;
use crate::fsutil::{atomic_write, is_within};
use crate::vault::{TokenScope, Vault};
use crate::{Codename, Error, Result};

fn refused(msg: impl Into<String>) -> Error {
    Error::Refused(msg.into())
}

/// Validate `name` (a relative file name, optionally with subfolders) and
/// return the absolute target under `dir`, creating intermediate folders.
/// `dir` must resolve inside `private_folder`.
pub fn resolve_target(
    private_folder: &Utf8Path,
    dir: &Utf8Path,
    name: &str,
) -> Result<Utf8PathBuf> {
    if name.is_empty() || name.contains('\0') || name.starts_with('/') || name.starts_with('\\') {
        return Err(refused("output name must be a relative file name"));
    }
    let segments: Vec<&str> = name.split(['/', '\\']).collect();
    if segments
        .iter()
        .any(|s| s.is_empty() || *s == "." || *s == "..")
    {
        return Err(refused(
            "output name may not contain '.', '..' or empty segments",
        ));
    }
    let private = canonical(private_folder)?;
    let mut cur = canonical(dir)?;
    if !is_within(&cur, &private) {
        return Err(refused("output folder is outside the private folder"));
    }
    let (last, parents) = segments
        .split_last()
        .ok_or_else(|| refused("empty output name"))?;
    for seg in parents {
        let next = cur.join(seg);
        match fs::symlink_metadata(&next) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(refused("output path goes through a symbolic link"));
            }
            Ok(m) if m.is_dir() => {}
            Ok(_) => return Err(refused("output path goes through a file")),
            Err(_) => fs::create_dir(&next)?,
        }
        cur = next;
    }
    let target = cur.join(last);
    if let Ok(m) = fs::symlink_metadata(&target) {
        if m.file_type().is_symlink() {
            return Err(refused("output target is a symbolic link"));
        }
        if m.is_dir() {
            return Err(refused("output target is a folder"));
        }
    }
    Ok(target)
}

fn canonical(p: &Utf8Path) -> Result<Utf8PathBuf> {
    let c = fs::canonicalize(p).map_err(|_| refused("output folder does not exist"))?;
    Utf8PathBuf::from_path_buf(c).map_err(|_| refused("path is not valid UTF-8"))
}

/// Rehydrate the tokens of `content` that are in `scope` and write it to
/// `dir/name`. Returns the path written, the proposed codename for the
/// output, `<source>-<name>` (not yet checked for collisions), and the number
/// of tokens left as literal text.
#[allow(clippy::too_many_arguments)]
pub fn write(
    vault: &Vault,
    scope: &TokenScope,
    private_folder: &Utf8Path,
    dir: &Utf8Path,
    name: &str,
    content: &str,
    source: &Codename,
    allow_overwrite: bool,
) -> Result<(Utf8PathBuf, Codename, u32)> {
    let target = resolve_target(private_folder, dir, name)?;
    if !allow_overwrite && fs::symlink_metadata(&target).is_ok() {
        return Err(refused("a file with that name already exists"));
    }
    let (text, skipped) = vault.rehydrate_scoped(content, scope);
    atomic_write(&target, text.as_bytes())?;
    tracing::info!(skipped, "wrote rehydrated output");
    Ok((target, propose_output(source, name), skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::MemoryKeyStore;
    use crate::{Error, TokenKind};
    use std::fs;

    struct Fx {
        _d: tempfile::TempDir,
        root: Utf8PathBuf,
        private: Utf8PathBuf,
        vault: Vault,
    }

    fn fx() -> Fx {
        let d = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(d.path().canonicalize().unwrap()).unwrap();
        let private = root.join("Private");
        fs::create_dir_all(private.join("sub")).unwrap();
        let vault = Vault::open(&root.join("vault.bin"), &MemoryKeyStore::new().unwrap()).unwrap();
        Fx {
            _d: d,
            root,
            private,
            vault,
        }
    }

    fn src() -> Codename {
        Codename::parse("w2-acme-2025").unwrap()
    }

    fn refused<T: std::fmt::Debug>(r: Result<T>) -> bool {
        matches!(r, Err(Error::Refused(_)))
    }

    #[test]
    fn writes_rehydrated_content_under_private() {
        let f = fx();
        let t = f.vault.mint(&TokenKind::Ssn, "000-12-3456").unwrap();
        let scope: TokenScope = [(t.kind.label(), t.index)].into_iter().collect();
        let (path, code, skipped) = write(
            &f.vault,
            &scope,
            &f.private,
            &f.private.join("sub"),
            "summary.md",
            "SSN: [SSN_1] [SSN_7]",
            &src(),
            false,
        )
        .unwrap();
        assert_eq!(path, f.private.join("sub/summary.md"));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "SSN: 000-12-3456 [SSN_7]"
        );
        assert_eq!(code.as_str(), "w2-acme-2025-summary");
        assert_eq!(skipped, 1);
    }

    #[test]
    fn nested_names_create_directories() {
        let f = fx();
        let p = resolve_target(&f.private, &f.private, "notes/q1/out.txt").unwrap();
        assert_eq!(p, f.private.join("notes/q1/out.txt"));
        assert!(f.private.join("notes/q1").is_dir());
    }

    #[test]
    fn refuses_bad_names() {
        let f = fx();
        for name in [
            "",
            "/etc/passwd",
            "../x",
            "a/../../x",
            "./x",
            "a/./b",
            "x\0y",
        ] {
            assert!(
                refused(resolve_target(&f.private, &f.private, name)),
                "{name:?}"
            );
        }
    }

    #[test]
    fn refuses_dir_outside_private() {
        let f = fx();
        assert!(refused(resolve_target(&f.private, &f.root, "x.txt")));
    }

    #[test]
    fn refuses_existing_unless_overwrite_and_never_dirs() {
        let f = fx();
        fs::write(f.private.join("a.txt"), "orig").unwrap();
        assert!(refused(write(
            &f.vault,
            &TokenScope::new(),
            &f.private,
            &f.private,
            "a.txt",
            "new",
            &src(),
            false
        )));
        assert_eq!(fs::read_to_string(f.private.join("a.txt")).unwrap(), "orig");
        write(
            &f.vault,
            &TokenScope::new(),
            &f.private,
            &f.private,
            "a.txt",
            "new",
            &src(),
            true,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(f.private.join("a.txt")).unwrap(), "new");
        assert!(refused(resolve_target(&f.private, &f.private, "sub")));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_escapes() {
        let f = fx();
        let outside = f.root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, f.private.join("escape")).unwrap();
        assert!(refused(resolve_target(
            &f.private,
            &f.private,
            "escape/x.txt"
        )));
        assert!(refused(resolve_target(
            &f.private,
            &f.private.join("escape"),
            "x.txt"
        )));
        // A symlinked file target is refused too, even pointing inside.
        fs::write(f.private.join("real.txt"), "r").unwrap();
        std::os::unix::fs::symlink(f.private.join("real.txt"), f.private.join("ln.txt")).unwrap();
        assert!(refused(resolve_target(&f.private, &f.private, "ln.txt")));
        assert!(!outside.join("x.txt").exists());
    }
}
