use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Write `contents` to `path` so that a reader sees either the old file or the
/// new one, never a partial write.
///
/// `fs::write` truncates before writing. An interruption in between — a crash,
/// a full disk, Ctrl-C, the OOM killer — leaves the file truncated. For
/// `meta/manifest.json` that is not a recoverable state: it holds `origin` and
/// `ingested_at`, which #16 established cannot be derived from disk, and a
/// truncated manifest makes every command fail to parse it. For a wiki article
/// rewritten by `mv`, it is the user's own prose.
///
/// The standard remedy: write a sibling temp file, flush it to disk, then
/// rename over the target. `rename` is atomic within a filesystem, and the temp
/// file is a sibling specifically so the rename never crosses one.
pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let temp = temp_path(path)?;

    // Scoped so the handle is closed before the rename.
    let result = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(contents.as_ref())?;
        // Without this the rename can land before the data does, and a crash
        // leaves an empty file where the old one used to be.
        file.sync_all()
    })();

    if let Err(e) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }

    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

/// Rehearse `write` without changing anything, to find out whether it would
/// work.
///
/// Creates and removes the same temp sibling `write` would, so it exercises the
/// permission that actually fails — directory write access, not the target
/// file's mode. A caller about to perform several writes as one logical change
/// can check them all first.
///
/// This narrows a window; it does not close it. Nothing stops permissions
/// changing between the rehearsal and the write. It turns the overwhelmingly
/// common cause of a half-applied change — a directory that was never writable
/// — into a refusal that does nothing.
pub fn preflight(path: &Path) -> io::Result<()> {
    let temp = temp_path(path)?;
    std::fs::File::create(&temp)?;
    let _ = std::fs::remove_file(&temp);
    Ok(())
}

/// A hidden sibling of `path`, so the rename stays within one filesystem.
///
/// The pid keeps two concurrent sentinel processes from colliding on the temp
/// file. It does not make the overall update concurrency-safe — two processes
/// rewriting the manifest still race, last writer wins — but it stops them
/// corrupting each other's partial writes.
fn temp_path(path: &Path) -> io::Result<PathBuf> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no parent directory", path.display()),
        )
    })?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid filename"))?;
    Ok(parent.join(format!(".{name}.{}.tmp", std::process::id())))
}

/// Write only if the contents differ from what is already there.
///
/// Returns whether it wrote. Generated files are deterministic, so an `index`
/// that changes nothing should leave every mtime alone: the archive lives in
/// git, and a rebuild that rewrites five identical files makes the working tree
/// look modified when nothing happened.
pub fn write_if_changed(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<bool> {
    let contents = contents.as_ref();
    if let Ok(existing) = std::fs::read(path)
        && existing == contents
    {
        return Ok(false);
    }
    write(path, contents)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.json");
        write(&path, "hello").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn replaces_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.json");
        std::fs::write(&path, "old").unwrap();
        write(&path, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn leaves_no_temp_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("f.json"), "x").unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn a_failed_write_leaves_the_original_intact() {
        // The property that matters: the old file survives a failure, rather
        // than being truncated before the new content is known to be writable.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("f.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "original").unwrap();

        // A directory where the temp file needs to go makes File::create fail.
        let temp = temp_path(&path).unwrap();
        std::fs::create_dir(&temp).unwrap();

        assert!(write(&path, "replacement").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    }

    #[test]
    fn write_if_changed_skips_an_identical_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.md");
        assert!(
            write_if_changed(&path, "same").unwrap(),
            "first write happens"
        );

        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(
            !write_if_changed(&path, "same").unwrap(),
            "second is a no-op"
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before,
            "an unchanged write must not touch the mtime"
        );
    }

    #[test]
    fn write_if_changed_writes_when_it_differs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.md");
        write_if_changed(&path, "old").unwrap();
        assert!(write_if_changed(&path, "new").unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn the_temp_file_is_a_sibling_so_rename_stays_on_one_filesystem() {
        let path = Path::new("/a/b/meta/manifest.json");
        assert_eq!(
            temp_path(path).unwrap().parent().unwrap(),
            path.parent().unwrap()
        );
    }
}
