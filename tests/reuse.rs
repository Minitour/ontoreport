use std::path::PathBuf;

use ontoreport::releases;
use ontoreport::report::{Report, Section};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reuse")
}

fn run() -> Report {
    let index = ontoreport::load(&fixture()).unwrap();
    let plans = releases::plans(&index);
    let dir = tempfile::tempdir().unwrap();
    // The fixture's release is a local file, so nothing is downloaded.
    let rels = releases::fetch_all(&plans, dir.path(), true, 100);
    ontoreport::report_with(
        &index,
        &ontoreport::Context {
            releases: Some(&rels),
            ..Default::default()
        },
    )
}

fn section<'a>(r: &'a Report, id: &str) -> &'a Section {
    r.sections.iter().find(|s| s.id == id).expect("section")
}

fn found(s: &Section, check: &str) -> Vec<String> {
    s.findings
        .iter()
        .filter(|f| f.check == check)
        .map(|f| f.element.clone())
        .collect()
}

#[test]
fn releases_toml_is_read_with_paths_relative_to_it() {
    let index = ontoreport::load(&fixture()).unwrap();
    assert_eq!(index.release_specs.len(), 1);
    assert!(index.release_specs[0].files[0].ends_with("water.ttl"));
}

#[test]
fn each_kind_of_reuse_problem_is_found() {
    let r = run();
    let s = section(&r, "reuse");
    let o = "(ontology file)";
    // Lake, River, Stream, Pond, Sea, Ocean; Bay is the project's own.
    assert_eq!(
        s.metric_value(&format!("reused terms checked {o}"))
            .unwrap(),
        6
    );
    // Lake matches: case, spacing and the final full stop do not count.
    assert_eq!(
        s.metric_value(&format!("reused terms matching their release {o}"))
            .unwrap(),
        1
    );
    let w = |id: &str, label: &str| format!("{label} (http://example.org/water#{id})");
    assert_eq!(
        found(s, "obsolete in the source release"),
        vec![w("River", "river")]
    );
    assert!(s
        .findings
        .iter()
        .any(|f| f.check == "obsolete in the source release" && f.detail.contains("'stream'")));
    assert_eq!(
        found(s, "not in the source release"),
        vec![w("Ocean", "ocean")]
    );
    assert_eq!(
        found(s, "parent obsolete in the source release"),
        vec![w("Pond", "pond")]
    );
    assert_eq!(
        found(s, "label differs from the release"),
        vec![w("Stream", "brook")]
    );
    assert_eq!(
        found(s, "definition not in the release"),
        vec![w("Sea", "sea")]
    );
    assert!(found(s, "definition differs from the release").is_empty());
    assert_eq!(
        s.metric_value("releases that differ from the recorded version")
            .unwrap(),
        0
    );
}

#[test]
fn without_releases_only_identifiers_are_checked() {
    let index = ontoreport::load(&fixture()).unwrap();
    let r = ontoreport::report(&index);
    let s = section(&r, "reuse");
    assert!(s.ran);
    assert!(s.findings.is_empty());
    assert!(s.notes[0].contains("not read"));
}

#[test]
fn every_reuse_metric_and_chart_has_help() {
    let r = run();
    let s = section(&r, "reuse");
    for m in &s.metrics {
        assert!(m.help.is_some(), "no help for metric {}", m.name);
    }
    for c in &s.charts {
        let title = serde_json::to_value(c).unwrap()["title"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            ontoreport::help::about(&title).is_some(),
            "no help for chart {title}"
        );
    }
    for f in &s.findings {
        assert!(ontoreport::help::about(&f.check).is_some(), "{}", f.check);
    }
}

#[test]
fn recorded_decisions_apply_to_any_check() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture();
    std::fs::create_dir_all(dir.path().join("source")).unwrap();
    for f in ["onto.ttl", "releases.toml", "source/water.ttl"] {
        std::fs::copy(src.join(f), dir.path().join(f)).unwrap();
    }
    std::fs::write(
        dir.path().join("decisions.json"),
        r#"{"findings": [{"id": "label differs from the release",
             "elements": ["brook (http://example.org/water#Stream)"],
             "decision": "accepted", "reason": "local name kept", "ref": "notes 3"}]}"#,
    )
    .unwrap();
    let index = ontoreport::load(dir.path()).unwrap();
    let rels = releases::fetch_all(&releases::plans(&index), dir.path(), true, 100);
    let r = ontoreport::report_with(
        &index,
        &ontoreport::Context {
            releases: Some(&rels),
            ..Default::default()
        },
    );
    let s = section(&r, "reuse");
    let f = s
        .findings
        .iter()
        .find(|f| f.check == "label differs from the release")
        .unwrap();
    assert_eq!(f.status.as_deref(), Some("accepted"));
    assert!(f.detail.starts_with("accepted: local name kept (notes 3)"));
    // Other findings stay as they were.
    assert!(s
        .findings
        .iter()
        .filter(|f| f.check != "label differs from the release")
        .all(|f| f.status.is_none()));
}
