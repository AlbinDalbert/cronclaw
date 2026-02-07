use cronclaw::config;
use std::fs;
use tempfile::TempDir;

#[test]
fn config_defaults_when_missing() {
    let dir = TempDir::new().unwrap();
    let cfg = config::load(&dir.path().join("nope.yaml"));
    assert_eq!(cfg.timeout, 300);
}

#[test]
fn config_defaults_when_empty_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.yaml");
    fs::write(&path, "").unwrap();
    let cfg = config::load(&path);
    assert_eq!(cfg.timeout, 300);
}

#[test]
fn config_defaults_when_comment_only() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.yaml");
    fs::write(&path, "# cronclaw configuration\n").unwrap();
    let cfg = config::load(&path);
    assert_eq!(cfg.timeout, 300);
}

#[test]
fn config_custom_timeout() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.yaml");
    fs::write(&path, "timeout: 600\n").unwrap();
    let cfg = config::load(&path);
    assert_eq!(cfg.timeout, 600);
}
