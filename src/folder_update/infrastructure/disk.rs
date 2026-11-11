//! The file operations of a folder update. Each path is relative to the job search folder, with `/` separators, and is
//! looked at one folder at a time from the job search folder down, so nothing is read, written, renamed or deleted
//! through a link inside it.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::shared_kernel::installed_source;

/// What is at a path of the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// A plain file, its bytes and its mode.
    File(Vec<u8>, u32),
    /// Nothing.
    Missing,
    /// The path, or a folder above it, is a link.
    Link,
    /// A folder or something else stands at the path, or something that is not a folder above it.
    Other,
}

/// What is at a path, and the folders above it that are missing, outermost first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Look {
    pub found: Found,
    pub missing: Vec<String>,
}

/// `relative` under `folder`, one part at a time.
pub fn joined(folder: &Path, relative: &str) -> PathBuf {
    relative.split('/').fold(folder.to_path_buf(), |path, part| path.join(part))
}

/// What is at `relative` in `folder`. Each folder above it is looked at first, without following a link.
pub fn look(folder: &Path, relative: &str) -> io::Result<Look> {
    let parts: Vec<&str> = relative.split('/').collect();
    let mut at = folder.to_path_buf();
    let mut missing = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        at.push(part);
        let last = index + 1 == parts.len();
        if !missing.is_empty() {
            if !last {
                missing.push(parts[..=index].join("/"));
            }
            continue;
        }
        let meta = match fs::symlink_metadata(&at) {
            Ok(meta) => meta,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if !last {
                    missing.push(parts[..=index].join("/"));
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        let found = if meta.file_type().is_symlink() {
            Found::Link
        } else if !last && meta.is_dir() {
            continue;
        } else if last && meta.is_file() {
            let (bytes, mode) = read_plain(&at)?;
            Found::File(bytes, mode)
        } else {
            Found::Other
        };
        return Ok(Look { found, missing: Vec::new() });
    }
    Ok(Look { found: Found::Missing, missing })
}

/// Whether `relative` in `folder`, looked at from the folder down, holds the bytes whose SHA-256 is `sha256`, or nothing
/// when that is `None`. A link on the way, or anything but a plain file, holds neither.
pub fn holds(folder: &Path, relative: &str, sha256: Option<&str>) -> io::Result<bool> {
    let seen = look(folder, relative)?;
    Ok(match (seen.found, sha256) {
        (Found::File(bytes, _), Some(sha256)) => installed_source::sha256_hex(&bytes) == sha256,
        (Found::Missing, None) => true,
        _ => false,
    })
}

/// The whole of the plain file at `path` and its mode, refusing a link that took its place since it was looked at.
pub fn read_plain(path: &Path) -> io::Result<(Vec<u8>, u32)> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NOFOLLOW);
    let mut file = options.open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(io::Error::other("not a plain file"));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok((bytes, mode_of(&meta)))
}

/// The read, write and run bits of a file, or the mode a release gives a plain file where there are none.
fn mode_of(meta: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0o644
    }
}

/// Writes a new file of `bytes` at `path` with `mode`, and waits until it is on the disk. A file or link already there
/// fails it.
pub fn write_new(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, mode);
    #[cfg(not(unix))]
    let _ = mode;
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Waits until the names in the folder `dir` are on the disk. Windows has no such call for a folder, and needs none.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = (dir, File::open);
    Ok(())
}

/// Removes `path` and what it holds. A link is removed itself, never followed.
pub fn remove(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Whether `path` is a real folder, not a link to one.
pub fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}

/// Whether anything, a link included, is at `path`.
pub fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Whether `path` is in the folder's `.jobdesk`, spelled in any letter case. The release zip refuses such a path
/// already: this keeps a file of a release away from it whatever the zip holds.
pub fn in_jobdesk(path: &str) -> bool {
    path.split('/').next().is_some_and(|first| first.eq_ignore_ascii_case(installed_source::DIR))
}

/// Whether `path` is a plain relative path of plain names: none empty, `.` or `..`, and none with a backslash, a colon
/// or a control character. A path read back from a record in the folder must be one before it is used.
pub fn is_plain_relative(path: &str) -> bool {
    path.split('/').all(|part| !part.is_empty() && part != "." && part != ".." && !part.chars().any(|c| c.is_control() || matches!(c, '\\' | ':')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_finds_a_file_nothing_or_missing_folders() {
        let folder = tempfile::tempdir().unwrap();
        fs::create_dir_all(folder.path().join("a/b")).unwrap();
        fs::write(folder.path().join("a/b/c.md"), "text").unwrap();
        assert!(matches!(look(folder.path(), "a/b/c.md").unwrap().found, Found::File(bytes, _) if bytes == b"text"));
        assert_eq!(look(folder.path(), "a/b/d.md").unwrap(), Look { found: Found::Missing, missing: Vec::new() });
        assert_eq!(look(folder.path(), "a/x/y/z.md").unwrap(), Look { found: Found::Missing, missing: vec!["a/x".to_owned(), "a/x/y".to_owned()] });
        assert_eq!(look(folder.path(), "a/b").unwrap().found, Found::Other, "a folder where a file goes");
        assert_eq!(look(folder.path(), "a/b/c.md/d").unwrap().found, Found::Other, "a file where a folder goes");
    }

    #[cfg(unix)]
    #[test]
    fn a_look_never_follows_a_link() {
        use std::os::unix::fs::symlink;
        let folder = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("c.md"), "outside").unwrap();
        symlink(outside.path(), folder.path().join("linked")).unwrap();
        symlink(outside.path().join("c.md"), folder.path().join("c.md")).unwrap();
        assert_eq!(look(folder.path(), "linked/c.md").unwrap().found, Found::Link);
        assert_eq!(look(folder.path(), "c.md").unwrap().found, Found::Link);
        assert!(read_plain(&folder.path().join("c.md")).is_err());
    }

    #[test]
    fn jobdesk_in_any_case_is_known() {
        assert!(in_jobdesk(".jobdesk/source.zip"));
        assert!(in_jobdesk(".JobDesk"));
        assert!(!in_jobdesk("docs/.jobdesk"));
        assert!(!in_jobdesk(".jobdesk-notes/a.md"));
    }

    #[test]
    fn only_plain_relative_paths_pass() {
        for plain in ["a.md", "a/b.md", ".claude/settings.json", "CLAUDE.md.update-1.8.0"] {
            assert!(is_plain_relative(plain), "{plain}");
        }
        for bad in ["", "/a", "a/", "a//b", "../a", "a/../b", "./a", "a\\b", "C:/a", "a\nb"] {
            assert!(!is_plain_relative(bad), "{bad:?}");
        }
    }
}
