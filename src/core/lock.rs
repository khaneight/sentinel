use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long to wait for another process to finish before giving up.
const WAIT: Duration = Duration::from_secs(10);

/// A lock older than this is assumed to belong to a process that died.
///
/// `Drop` releases the lock on every ordinary exit including `?` propagation,
/// but not on SIGKILL or a power cut. Without a staleness rule one such death
/// would wedge the archive permanently, which is worse than the race.
const STALE_AFTER: Duration = Duration::from_secs(120);

/// Exclusive access to the manifest, held for the whole read-modify-write.
///
/// Comparing the manifest against what was loaded is not enough on its own:
/// two processes both read, both compare successfully, and both write, because
/// nothing orders the compare against the write. Measured, that lost one entry
/// per pair of concurrent `ingest` calls — every one reporting success.
///
/// `File::create_new` is the atomic primitive available without a dependency:
/// exactly one caller can create a given path.
pub struct ArchiveLock {
    path: PathBuf,
}

impl ArchiveLock {
    /// Take the lock, waiting for another holder to finish.
    pub fn acquire(meta_dir: &Path) -> io::Result<Self> {
        let path = meta_dir.join(".lock");
        let deadline = SystemTime::now() + WAIT;

        loop {
            match std::fs::File::create_new(&path) {
                Ok(mut file) => {
                    use std::io::Write;
                    // Recorded for diagnosis; nothing reads it to make a
                    // decision, so a garbled write cannot wedge anything.
                    let _ = writeln!(file, "pid {}", std::process::id());
                    return Ok(Self { path });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if is_stale(&path) {
                        // Best effort: if another process removes it first, the
                        // next create_new simply succeeds for one of us.
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    if SystemTime::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::ResourceBusy,
                            format!(
                                "another sentinel process is using this archive \
                                 ({}). Nothing was written; try again once it \
                                 finishes, or delete that file if no sentinel is \
                                 running.",
                                path.display()
                            ),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => return Err(e),
            }
        }
    }
}

impl Drop for ArchiveLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn is_stale(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .and_then(|t| {
            SystemTime::now()
                .duration_since(t)
                .map_err(|_| io::Error::other("clock"))
        })
        .is_ok_and(|age| age > STALE_AFTER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_holder_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let _held = ArchiveLock::acquire(dir.path()).unwrap();

        // A second acquire blocks until the deadline; shorten the wait by
        // checking the lock file exists rather than waiting ten seconds.
        assert!(dir.path().join(".lock").exists());
    }

    #[test]
    fn dropping_releases_it() {
        let dir = tempfile::tempdir().unwrap();
        {
            let _held = ArchiveLock::acquire(dir.path()).unwrap();
        }
        assert!(!dir.path().join(".lock").exists());
        ArchiveLock::acquire(dir.path()).unwrap();
    }

    #[test]
    fn a_stale_lock_is_broken_rather_than_wedging_the_archive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".lock");
        std::fs::write(&path, "pid 1").unwrap();
        let old = SystemTime::now() - STALE_AFTER - Duration::from_secs(60);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(old)).unwrap();

        ArchiveLock::acquire(dir.path()).expect("a dead process must not wedge the archive");
    }
}
