use std::path::PathBuf;

use ontoreport::report::{Report, Section};
use ontoreport::sections::requirement_ids;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/desk")
}

fn run() -> Report {
    let index = ontoreport::load(&fixture()).expect("fixture loads");
    ontoreport::report(&index)
}

fn section<'a>(r: &'a Report, id: &str) -> &'a Section {
    r.sections
        .iter()
        .find(|s| s.id == id)
        .expect("section present")
}

fn metric(s: &Section, name: &str) -> serde_json::Value {
    s.metrics
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("metric {name}"))
        .value
        .clone()
}

fn elements(s: &Section, check: &str) -> Vec<String> {
    s.findings
        .iter()
        .filter(|f| f.check == check)
        .map(|f| f.element.clone())
        .collect()
}

#[test]
fn structure_finds_broken_chains_across_files_and_candidates() {
    let r = run();
    let s = section(&r, "structure");
    // "water sample" reaches the top through "sample", a term of the ontology file, and
    // "undefined thing" through the desk's parent-chain table; "dropped term" is out and not checked.
    assert_eq!(
        elements(s, "no path to the top class"),
        vec!["chart", "orphan class"]
    );
    assert_eq!(metric(s, "classes checked (ontology file)"), 3);
    assert_eq!(metric(s, "classes checked (desk candidates)"), 4);
    assert_eq!(metric(s, "tree errors recorded by the desk"), 1);
    let orphan = s
        .findings
        .iter()
        .find(|f| f.element == "orphan class")
        .unwrap();
    assert_eq!(orphan.detail, "chain breaks at: missing parent");
}

#[test]
fn provenance_counts_missing_definitions_and_weak_sources() {
    let r = run();
    let s = section(&r, "provenance");
    // An elucidation counts as a definition; the ontology header page is not a term.
    assert_eq!(
        elements(s, "no definition"),
        vec!["editor note", "orphan class", "undefined thing"]
    );
    assert_eq!(elements(s, "definition cites a weak source"), vec!["chart"]);
    assert_eq!(metric(s, "without a definition (ontology file)"), 2);
    assert_eq!(metric(s, "without a definition (desk candidates)"), 1);
    assert_eq!(
        metric(
            s,
            "definitions with their own source annotation (ontology file)"
        ),
        1
    );
    assert_eq!(
        metric(
            s,
            "definitions with their own source annotation (desk candidates)"
        ),
        1
    );
}

#[test]
fn traceability_reads_requirements_from_groups() {
    let r = run();
    let s = section(&r, "traceability");
    assert_eq!(metric(s, "groups checked (not out)"), 3);
    assert_eq!(
        metric(s, "groups naming a competency question or use case"),
        1
    );
    assert_eq!(metric(s, "groups with a reason but no question"), 1);
    assert_eq!(metric(s, "distinct requirements named"), 2);
    assert_eq!(
        elements(s, "group with no question, use case or reason"),
        vec!["Unjustified group"]
    );
}

#[test]
fn review_status_counts_terms_by_source() {
    let r = run();
    let s = section(&r, "review");
    assert_eq!(
        metric(s, "terms by status: EX"),
        serde_json::json!({"debatable": 1, "in": 3, "out": 1})
    );
}

#[test]
fn input_checks_and_recorded_findings_are_carried() {
    let r = run();
    assert_eq!(r.input_checks.len(), 1);
    assert_eq!(r.recorded.len(), 1);
    assert_eq!(r.recorded[0].element, "chart");
}

#[test]
fn report_is_deterministic() {
    let a = serde_json::to_string(&run()).unwrap();
    let b = serde_json::to_string(&run()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn html_is_self_contained() {
    let html = ontoreport::html::render(&run());
    assert!(html.starts_with("<!doctype html>"));
    assert!(!html.contains("<script src") && !html.contains("<link "));
    assert!(html.contains("orphan class"));
}

#[test]
fn requirement_ids_are_parsed_from_free_text() {
    assert_eq!(
        requirement_ids("A1, A3, B12 (what was measured), CQ7; see X4"),
        vec!["A1", "A3", "B12", "CQ7", "X4"]
    );
    assert!(requirement_ids("values, BFO, ABC1, 12").is_empty());
}
