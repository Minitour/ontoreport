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
fn placeholder_exits_with_message() {
    let out = bin().arg(".").output().expect("run ontoreport");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("placeholder release"));
}
