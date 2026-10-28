//! Where INKER keeps its data and its socket.

use std::path::{Path, PathBuf};

/// Environment variable that overrides the data directory.
pub const DATA_DIR_ENV: &str = "INKER_DATA_DIR";
/// File name of the socket inside the data directory.
pub const SOCKET_NAME: &str = "inker.sock";

/// The data directory, from an explicit environment lookup and home directory.
/// Pure, for tests.
pub fn data_dir_with(env: impl Fn(&str) -> Option<String>, home: &Path, macos: bool) -> PathBuf {
    let non_empty = |k: &str| env(k).filter(|v| !v.is_empty());
    if let Some(d) = non_empty(DATA_DIR_ENV) {
        return PathBuf::from(d);
    }
    if macos {
        return home.join("Library/Application Support/sh.inker");
    }
    match non_empty("XDG_DATA_HOME") {
        Some(x) => PathBuf::from(x).join("inker"),
        None => home.join(".local/share/inker"),
    }
}

/// The user's home directory (`$HOME`), falling back to `/` so paths stay absolute.
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// The data directory for this process.
pub fn data_dir() -> PathBuf {
    data_dir_with(
        |k| std::env::var(k).ok(),
        &home_dir(),
        cfg!(target_os = "macos"),
    )
}

/// The socket path for this process.
pub fn socket_path() -> PathBuf {
    data_dir().join(SOCKET_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn override_wins_everywhere() {
        let home = Path::new("/home/u");
        for macos in [true, false] {
            let d = data_dir_with(
                env(&[(DATA_DIR_ENV, "/x/y"), ("XDG_DATA_HOME", "/z")]),
                home,
                macos,
            );
            assert_eq!(d, PathBuf::from("/x/y"));
        }
    }

    #[test]
    fn macos_uses_application_support() {
        let d = data_dir_with(env(&[]), Path::new("/Users/u"), true);
        assert_eq!(
            d,
            PathBuf::from("/Users/u/Library/Application Support/sh.inker")
        );
    }

    #[test]
    fn elsewhere_uses_xdg_data_home_or_default() {
        let home = Path::new("/home/u");
        assert_eq!(
            data_dir_with(env(&[("XDG_DATA_HOME", "/d")]), home, false),
            PathBuf::from("/d/inker")
        );
        assert_eq!(
            data_dir_with(env(&[]), home, false),
            PathBuf::from("/home/u/.local/share/inker")
        );
        // Empty values are ignored, as the XDG spec says.
        assert_eq!(
            data_dir_with(
                env(&[("XDG_DATA_HOME", ""), (DATA_DIR_ENV, "")]),
                home,
                false
            ),
            PathBuf::from("/home/u/.local/share/inker")
        );
    }

    #[test]
    fn process_paths_follow_the_override() {
        // Only reads the environment; INKER_DATA_DIR is set by the test runner if at all.
        let d = data_dir();
        assert!(d.is_absolute(), "{d:?}");
        assert_eq!(socket_path(), d.join(SOCKET_NAME));
    }
}
