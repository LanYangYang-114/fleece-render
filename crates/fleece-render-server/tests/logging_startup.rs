use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use chrono::NaiveDate;

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Fixture {
    _root: tempfile::TempDir,
    executable: PathBuf,
    working_directory: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        let working_directory = root.path().join("different-working-directory");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&working_directory).unwrap();
        let source = Path::new(env!("CARGO_BIN_EXE_fleece-render"));
        let executable = bin.join(source.file_name().unwrap());
        fs::copy(source, &executable).unwrap();
        Self {
            _root: root,
            executable,
            working_directory,
        }
    }

    fn start(&self, config: &str, env_log: Option<&str>, cli_log: Option<&str>) {
        let config_path = self.working_directory.join("test.toml");
        fs::write(&config_path, config).unwrap();
        let stderr_path = self.working_directory.join("stderr.log");
        let mut command = Command::new(&self.executable);
        command
            .current_dir(&self.working_directory)
            .arg("--config")
            .arg(config_path)
            .args(["--host", "127.0.0.1", "--port", "0", "--backend", "cpu"])
            .env("RUST_LOG", "info")
            .env_remove("FLEECE_RENDER_LOG_FILE")
            .stdout(Stdio::null())
            .stderr(File::create(&stderr_path).unwrap());
        if let Some(value) = env_log {
            command.env("FLEECE_RENDER_LOG_FILE", value);
        }
        if let Some(value) = cli_log {
            command.arg("--log-file").arg(value);
        }
        let mut server = Server(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let stderr = fs::read_to_string(&stderr_path).unwrap();
            if stderr.contains("fleece-render API started") {
                break;
            }
            assert!(
                server.0.try_wait().unwrap().is_none(),
                "Server exited: {stderr}"
            );
            assert!(Instant::now() < deadline, "Server did not start: {stderr}");
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn bin(&self) -> &Path {
        self.executable.parent().unwrap()
    }
}

fn service_logs(directory: &Path, stem: &str) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            let name = path.file_name().unwrap().to_str().unwrap();
            let Some(tail) = name
                .strip_prefix(&format!("{stem}-"))
                .and_then(|tail| tail.strip_suffix(".log"))
            else {
                return false;
            };
            let Some((date, sequence)) = tail.rsplit_once('-') else {
                return false;
            };
            NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok()
                && sequence.parse::<u64>().is_ok_and(|number| number > 0)
        })
        .collect();
    paths.sort();
    paths
}

#[test]
fn repeated_starts_use_executable_logs_directory_and_preserve_old_files() {
    let fixture = Fixture::new();
    fixture.start("", None, None);
    let logs = fixture.bin().join("logs");
    let first = service_logs(&logs, "fleece-render");
    assert_eq!(first.len(), 1);
    assert!(
        first[0]
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("-1.log")
    );
    let original = fs::read(&first[0]).unwrap();
    assert!(String::from_utf8_lossy(&original).contains("file logging enabled"));
    fixture.start("", None, None);
    assert_eq!(service_logs(&logs, "fleece-render").len(), 2);
    assert_eq!(fs::read(&first[0]).unwrap(), original);
    assert!(!fixture.working_directory.join("logs").exists());
    assert!(!fixture.bin().join("output").exists());
}

#[test]
fn config_env_and_cli_overrides_keep_their_precedence() {
    let fixture = Fixture::new();
    let config = "[server]\nlog_file = \"custom/config.log\"\n";
    fixture.start(config, None, None);
    assert_eq!(
        service_logs(&fixture.bin().join("custom"), "config").len(),
        1
    );
    let env_base = fixture.working_directory.join("absolute/env.log");
    fixture.start(config, Some(env_base.to_str().unwrap()), None);
    assert_eq!(service_logs(env_base.parent().unwrap(), "env").len(), 1);
    fixture.start(
        config,
        Some(env_base.to_str().unwrap()),
        Some("custom/cli.log"),
    );
    let custom = fixture.bin().join("custom");
    assert_eq!(service_logs(&custom, "cli").len(), 1);
    assert_eq!(service_logs(&custom, "config").len(), 1);
    assert_eq!(service_logs(env_base.parent().unwrap(), "env").len(), 1);
    assert!(!fixture.bin().join("logs").exists());
    assert!(!fixture.working_directory.join("custom").exists());
}

#[test]
fn empty_config_env_and_cli_settings_disable_files_but_not_stderr() {
    let fixture = Fixture::new();
    fixture.start("[server]\nlog_file = \"\"\n", None, None);
    fixture.start("", Some(""), None);
    fixture.start("", Some("custom/ignored.log"), Some(""));
    assert!(!fixture.bin().join("logs").exists());
    assert!(!fixture.bin().join("custom").exists());
    assert!(
        fs::read_to_string(fixture.working_directory.join("stderr.log"))
            .unwrap()
            .contains("file logging disabled")
    );
}

#[test]
fn startup_keeps_thirty_logs_by_default_including_the_new_run() {
    let fixture = Fixture::new();
    let logs = fixture.bin().join("logs");
    fs::create_dir(&logs).unwrap();
    for number in 1..=35 {
        fs::write(
            logs.join(format!("fleece-render-2000-01-01-{number}.log")),
            b"old",
        )
        .unwrap();
    }
    fixture.start("", None, None);
    assert_eq!(service_logs(&logs, "fleece-render").len(), 30);
    for number in 1..=6 {
        assert!(
            !logs
                .join(format!("fleece-render-2000-01-01-{number}.log"))
                .exists()
        );
    }
    assert!(logs.join("fleece-render-2000-01-01-7.log").exists());
}

#[test]
fn startup_applies_custom_retention_after_every_new_file() {
    let fixture = Fixture::new();
    let logs = fixture.working_directory.join("absolute");
    fs::create_dir(&logs).unwrap();
    for number in 1..=3 {
        fs::write(logs.join(format!("custom-2000-01-01-{number}.log")), b"old").unwrap();
    }
    let base = logs.join("custom.log");
    let config = "[server]\nlog_max_files = 2";
    for _ in 0..3 {
        fixture.start(config, Some(base.to_str().unwrap()), None);
        assert_eq!(service_logs(&logs, "custom").len(), 2);
    }
    for number in 1..=3 {
        assert!(
            !logs
                .join(format!("custom-2000-01-01-{number}.log"))
                .exists()
        );
    }
    assert!(!fixture.bin().join("logs").exists());
}

#[test]
fn disabling_file_logging_leaves_existing_logs_untouched() {
    let fixture = Fixture::new();
    let logs = fixture.bin().join("logs");
    fs::create_dir(&logs).unwrap();
    for number in 1..=3 {
        fs::write(
            logs.join(format!("fleece-render-2000-01-01-{number}.log")),
            b"preserve",
        )
        .unwrap();
    }
    fixture.start("[server]\nlog_max_files = 1", Some(""), None);
    assert_eq!(service_logs(&logs, "fleece-render").len(), 3);
}
