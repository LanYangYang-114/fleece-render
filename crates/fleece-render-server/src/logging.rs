use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{Local, NaiveDate};

/// Open one new file for this process. Empty/absent settings disable file logging.
pub(crate) fn open_log_file(
    configured: Option<&str>,
    max_files: usize,
) -> Result<Option<(PathBuf, File)>> {
    let Some(configured) = configured.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    anyhow::ensure!(max_files > 0, "log_max_files must be positive");
    let base = Path::new(configured);
    let base = if base.is_absolute() {
        base.to_path_buf()
    } else {
        let executable =
            std::env::current_exe().context("failed to locate executable for logging")?;
        resolve_log_path(base, &executable)?
    };
    let date = Local::now().format("%Y-%m-%d").to_string();
    // Allocate before pruning so retention never resets today's sequence to one.
    let (path, file) = create_numbered_log(&base, &date)?;
    prune_old_logs(&base, &path, max_files)?;
    Ok(Some((path, file)))
}

fn resolve_log_path(base: &Path, executable: &Path) -> Result<PathBuf> {
    if base.is_absolute() {
        return Ok(base.to_path_buf());
    }
    let directory = executable
        .parent()
        .context("executable has no parent directory")?;
    Ok(directory.join(base))
}

fn create_numbered_log(base: &Path, date: &str) -> Result<(PathBuf, File)> {
    let directory = base
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let stem = base
        .file_stem()
        .filter(|stem| !stem.is_empty())
        .context("log file path must include a file name")?;
    let mut prefix = OsString::from(stem);
    prefix.push(format!("-{date}-"));
    let mut suffix = OsString::new();
    if let Some(extension) = base.extension() {
        suffix.push(".");
        suffix.push(extension);
    }

    std::fs::create_dir_all(directory)
        .with_context(|| format!("failed to create log directory {}", directory.display()))?;
    let mut highest = 0_u64;
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("failed to read log directory {}", directory.display()))?
    {
        let name = entry
            .context("failed to read log directory entry")?
            .file_name();
        let Some(number) = name
            .as_encoded_bytes()
            .strip_prefix(prefix.as_encoded_bytes())
            .and_then(|name| name.strip_suffix(suffix.as_encoded_bytes()))
            .filter(|number| !number.is_empty() && number.iter().all(u8::is_ascii_digit))
            .and_then(|number| std::str::from_utf8(number).ok())
            .and_then(|number| number.parse::<u64>().ok())
        else {
            continue;
        };
        highest = highest.max(number);
    }

    loop {
        highest = highest
            .checked_add(1)
            .context("daily log sequence exhausted")?;
        let mut name = prefix.clone();
        name.push(highest.to_string());
        name.push(&suffix);
        let path = directory.join(name);
        // Exclusive creation is atomic, including when multiple processes start together.
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to create log file {}", path.display()));
            }
        }
    }
}

/// Retain only this base name's numbered, regular log files, including this run.
fn prune_old_logs(base: &Path, current: &Path, max_files: usize) -> Result<()> {
    anyhow::ensure!(max_files > 0, "log_max_files must be positive");
    let directory = current
        .parent()
        .context("log file has no parent directory")?;
    let mut prefix = OsString::from(base.file_stem().context("log file has no file name")?);
    prefix.push("-");
    let mut suffix = OsString::new();
    if let Some(extension) = base.extension() {
        suffix.push(".");
        suffix.push(extension);
    }
    let mut logs = Vec::new();
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("failed to read log directory {}", directory.display()))?
    {
        let entry = entry.context("failed to read log directory entry")?;
        let path = entry.path();
        if path == current {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name
            .as_encoded_bytes()
            .strip_prefix(prefix.as_encoded_bytes())
            .and_then(|name| name.strip_suffix(suffix.as_encoded_bytes()))
            .and_then(|name| std::str::from_utf8(name).ok())
        else {
            continue;
        };
        let Some((date, number)) = name.rsplit_once('-') else {
            continue;
        };
        if date.len() != 10
            || number.is_empty()
            || !number.bytes().all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        let (Ok(date), Ok(number)) = (
            NaiveDate::parse_from_str(date, "%Y-%m-%d"),
            number.parse::<u64>(),
        ) else {
            continue;
        };
        if number == 0
            || !entry
                .file_type()
                .context("failed to inspect log file")?
                .is_file()
        {
            continue;
        }
        logs.push((date, number, path));
    }
    // Compare numeric sequence numbers, not lexical names (e.g. 2 is older than 10).
    logs.sort();
    let excess = logs.len().saturating_sub(max_files - 1);
    for (_, _, path) in logs.into_iter().take(excess) {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            // Another starting process may already have removed the same old file.
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to remove old log file {}", path.display()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Barrier;

    use super::*;

    const DATE: &str = "2026-11-13";

    #[test]
    fn empty_settings_disable_file_logging() {
        for value in [None, Some(""), Some("  \t")] {
            assert!(open_log_file(value, 30).unwrap().is_none());
        }
    }

    #[test]
    fn relative_paths_are_anchored_to_the_executable() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("bin").join("fleece-render.exe");
        let relative = Path::new("logs/fleece-render.log");
        assert_eq!(
            resolve_log_path(relative, &executable).unwrap(),
            root.path().join("bin").join(relative)
        );
        let absolute = root.path().join("custom.log");
        assert_eq!(resolve_log_path(&absolute, &executable).unwrap(), absolute);
    }

    #[test]
    fn default_config_uses_a_single_logs_directory() {
        let config = fleece_render_server::AppConfig::default();
        assert_eq!(config.server.log_max_files, 30);
        assert_eq!(
            config.server.log_file.as_deref(),
            Some("logs/fleece-render.log")
        );
        let disabled: fleece_render_server::AppConfig =
            toml::from_str("[server]\nlog_file = \"\"").unwrap();
        assert!(
            open_log_file(disabled.server.log_file.as_deref(), 30)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn each_start_creates_a_new_file_without_changing_previous_logs() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("logs/fleece-render.log");
        let (first, mut file) = create_numbered_log(&base, DATE).unwrap();
        file.write_all(b"first run\n").unwrap();
        drop(file);
        let (second, _) = create_numbered_log(&base, DATE).unwrap();
        assert_eq!(first.file_name().unwrap(), "fleece-render-2026-11-13-1.log");
        assert_eq!(
            second.file_name().unwrap(),
            "fleece-render-2026-11-13-2.log"
        );
        assert_eq!(std::fs::read(first).unwrap(), b"first run\n");
        assert_eq!(std::fs::metadata(second).unwrap().len(), 0);
        assert!(!base.exists());
    }

    #[test]
    fn numbering_continues_after_the_highest_number_without_reusing_gaps() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "fleece-render-2026-11-13-1.log",
            "fleece-render-2026-11-13-7.log",
            "fleece-render-2026-11-12-99.log",
            "other-2026-11-13-99.log",
            "fleece-render-2026-11-13-invalid.log",
            "fleece-render-2026-11-13-99.log.bak",
            "fleece-render.log",
        ] {
            std::fs::write(root.path().join(name), b"preserve").unwrap();
        }
        let (path, _) = create_numbered_log(&root.path().join("fleece-render.log"), DATE).unwrap();
        assert_eq!(path.file_name().unwrap(), "fleece-render-2026-11-13-8.log");
        assert_eq!(
            std::fs::read(root.path().join("fleece-render.log")).unwrap(),
            b"preserve"
        );
    }

    #[test]
    fn a_new_day_starts_at_one_and_does_not_rotate_an_open_file() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("fleece-render.log");
        let (yesterday, mut open_file) = create_numbered_log(&base, "2026-11-12").unwrap();
        let (today, _) = create_numbered_log(&base, DATE).unwrap();
        open_file.write_all(b"still the same run").unwrap();
        assert_eq!(today.file_name().unwrap(), "fleece-render-2026-11-13-1.log");
        assert_eq!(std::fs::read(yesterday).unwrap(), b"still the same run");
    }

    #[test]
    fn custom_names_and_extensions_are_preserved() {
        let root = tempfile::tempdir().unwrap();
        for (base, expected) in [
            ("custom.txt", "custom-2026-11-13-1.txt"),
            ("custom", "custom-2026-11-13-1"),
            ("渲染日志.log", "渲染日志-2026-11-13-1.log"),
        ] {
            let (path, _) = create_numbered_log(&root.path().join(base), DATE).unwrap();
            assert_eq!(path.file_name().unwrap(), expected);
        }
    }

    #[test]
    fn absolute_overrides_create_numbered_files_in_the_requested_directory() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("custom.log");
        let (path, _) = open_log_file(Some(base.to_str().unwrap()), 30)
            .unwrap()
            .unwrap();
        assert_eq!(path.parent().unwrap(), root.path());
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("custom-")
        );
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with("-1.log")
        );
    }

    #[test]
    fn simultaneous_starts_get_distinct_files() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("fleece-render.log");
        let barrier = Barrier::new(8);
        let mut paths = std::thread::scope(|scope| {
            let jobs: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        let (path, mut file) = create_numbered_log(&base, DATE).unwrap();
                        file.write_all(b"one process").unwrap();
                        path
                    })
                })
                .collect();
            jobs.into_iter()
                .map(|job| job.join().unwrap())
                .collect::<Vec<_>>()
        });
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), 8);
        for path in paths {
            assert_eq!(std::fs::read(path).unwrap(), b"one process");
        }
    }

    #[test]
    fn retention_deletes_oldest_dates_and_numeric_sequences_first() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("service.log");
        // Create in reverse chronological order to ensure filesystem order is irrelevant.
        for name in [
            "service-2026-11-13-10.log",
            "service-2026-11-13-9.log",
            "service-2026-11-13-2.log",
            "service-2026-11-12-20.log",
            "service-2026-11-11-99.log",
        ] {
            std::fs::write(root.path().join(name), b"old run").unwrap();
        }
        let (current, _file) = create_numbered_log(&base, DATE).unwrap();
        prune_old_logs(&base, &current, 3).unwrap();
        let mut remaining: Vec<_> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        remaining.sort();
        let mut expected: Vec<OsString> = [
            "service-2026-11-13-9.log",
            "service-2026-11-13-10.log",
            "service-2026-11-13-11.log",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        expected.sort();
        assert_eq!(remaining, expected);
        assert!(current.exists());
    }

    #[test]
    fn retention_does_not_remove_unrelated_files_directories_or_invalid_names() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("service.log");
        let preserved = [
            "service.log",
            "other-2020-01-01-1.log",
            "service-2020-01-01-1.txt",
            "service-2020-01-01-1.log.bak",
            "service-2020-01-01-0.log",
            "service-2020-13-01-1.log",
            "service-2020-02-30-1.log",
            "service-2020-01-01-invalid.log",
        ];
        for name in preserved {
            std::fs::write(root.path().join(name), b"preserve").unwrap();
        }
        let directory = root.path().join("service-2020-01-01-2.log");
        std::fs::create_dir(&directory).unwrap();
        let old = root.path().join("service-2020-01-01-3.log");
        std::fs::write(&old, b"delete").unwrap();
        let (current, _file) = create_numbered_log(&base, DATE).unwrap();
        prune_old_logs(&base, &current, 1).unwrap();
        assert!(current.exists());
        assert!(!old.exists());
        assert!(directory.is_dir());
        for name in preserved {
            assert_eq!(std::fs::read(root.path().join(name)).unwrap(), b"preserve");
        }
    }

    #[test]
    fn retention_always_keeps_current_run_and_preserves_sequence_with_a_limit_of_one() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("service.log");
        let future = root.path().join("service-2099-01-01-1.log");
        std::fs::write(&future, b"future timestamp").unwrap();
        let (current, file) = create_numbered_log(&base, DATE).unwrap();
        prune_old_logs(&base, &current, 1).unwrap();
        assert!(current.exists());
        assert!(!future.exists());
        drop(file);
        let (next, _file) = create_numbered_log(&base, DATE).unwrap();
        prune_old_logs(&base, &next, 1).unwrap();
        assert!(
            next.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with("-2.log")
        );
        assert!(next.exists());
        assert!(!current.exists());
    }

    #[test]
    fn retention_does_nothing_below_the_limit_and_rejects_zero() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("service.log");
        let (old, file) = create_numbered_log(&base, DATE).unwrap();
        drop(file);
        let (current, _file) = create_numbered_log(&base, DATE).unwrap();
        prune_old_logs(&base, &current, 30).unwrap();
        assert!(old.exists());
        assert!(current.exists());
        assert!(prune_old_logs(&base, &current, 0).is_err());
        assert!(open_log_file(Some(base.to_str().unwrap()), 0).is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[test]
    fn retention_supports_custom_extensions_and_non_ascii_names() {
        let root = tempfile::tempdir().unwrap();
        for name in ["custom.txt", "custom", "渲染日志.log"] {
            let base = root.path().join(name);
            let (old, file) = create_numbered_log(&base, DATE).unwrap();
            drop(file);
            let (current, _file) = create_numbered_log(&base, DATE).unwrap();
            prune_old_logs(&base, &current, 1).unwrap();
            assert!(!old.exists());
            assert!(current.exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn retention_ignores_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("preserve.txt");
        std::fs::write(&target, b"preserve").unwrap();
        let link = root.path().join("service-2000-01-01-1.log");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let base = root.path().join("service.log");
        let (current, _file) = create_numbered_log(&base, DATE).unwrap();
        prune_old_logs(&base, &current, 1).unwrap();
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(target).unwrap(), b"preserve");
    }
    #[test]
    fn unwritable_path_and_sequence_overflow_return_errors() {
        let root = tempfile::tempdir().unwrap();
        let blocker = root.path().join("not-a-directory");
        std::fs::write(&blocker, b"preserve").unwrap();
        assert!(create_numbered_log(&blocker.join("fleece-render.log"), DATE).is_err());
        std::fs::write(
            root.path()
                .join(format!("fleece-render-{DATE}-{}.log", u64::MAX)),
            b"preserve",
        )
        .unwrap();
        assert!(create_numbered_log(&root.path().join("fleece-render.log"), DATE).is_err());
    }
}
