use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ontoreport"))
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
    let res = bin()
        .arg(&fixture)
        .arg("--out")
        .arg(&out)
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
    assert_eq!(json["sections"].as_array().unwrap().len(), 5);
}

#[test]
fn folder_without_inputs_exits_with_code_2() {
    let dir = tempfile::tempdir().unwrap();
    let res = bin().arg(dir.path()).output().expect("run ontoreport");
    assert_eq!(res.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&res.stderr).contains("no inputs found"));
}
