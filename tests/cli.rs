use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ontoreport"))
}

/// The binary with a temporary home folder, so tests neither read nor write ~/.ontoreport.
fn bin_in(home: &std::path::Path) -> Command {
    let mut c = bin();
    c.env("ONTOREPORT_HOME", home);
    c
}

#[test]
fn prints_version() {
    let out = bin().arg("--version").output().expect("run ontoreport");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
}

#[test]
fn writes_html_and_json_reports() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/desk");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("report.html");
    let res = bin_in(&dir.path().join("home"))
        .arg(&fixture)
        .arg("--out")
        .arg(&out)
        .arg("--no-embeddings")
        .arg("--offline")
        .output()
        .expect("run ontoreport");
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    assert!(out.is_file());
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("report.json")).unwrap()).unwrap();
    assert_eq!(json["tool"], "ontoreport");
    assert_eq!(json["sections"].as_array().unwrap().len(), 8);
}

#[test]
fn first_run_writes_the_configuration_file() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus");
    let res = bin_in(&home)
        .arg(&fixture)
        .arg("--out")
        .arg(dir.path().join("r.html"))
        .arg("--no-embeddings")
        .arg("--offline")
        .output()
        .expect("run ontoreport");
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    let cfg = std::fs::read_to_string(home.join("config.toml")).unwrap();
    assert_eq!(cfg, ontoreport::config::DEFAULT_CONFIG);
    // Nothing was downloaded.
    assert!(!home.join("models").exists());
}

#[test]
fn a_broken_configuration_file_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("bad.toml");
    std::fs::write(
        &cfg,
        "[model]
revison = \"x\"
",
    )
    .unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus");
    let res = bin_in(&dir.path().join("home"))
        .arg(&fixture)
        .arg("--out")
        .arg(dir.path().join("r.html"))
        .arg("--config")
        .arg(&cfg)
        .output()
        .expect("run ontoreport");
    assert_eq!(res.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&res.stderr).contains("revison"));
}

#[test]
fn folder_without_inputs_exits_with_code_2() {
    let dir = tempfile::tempdir().unwrap();
    let res = bin_in(&dir.path().join("home"))
        .arg(dir.path())
        .output()
        .expect("run ontoreport");
    assert_eq!(res.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&res.stderr).contains("no inputs found"));
}
