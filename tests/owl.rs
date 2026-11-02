use std::path::PathBuf;

use ontoreport::report::{Report, Section};

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/owl")
        .join(rel)
}

fn run(rel: &str) -> Report {
    let index = ontoreport::load(&fixture(rel)).expect("fixture loads");
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

#[test]
fn turtle_is_read_and_terms_indexed() {
    let index = ontoreport::load(&fixture("onto.ttl")).unwrap();
    assert_eq!(index.ontologies.len(), 1);
    assert_eq!(
        index.ontologies[0].iri.as_deref(),
        Some("http://example.org/o")
    );
    assert_eq!(
        index.ontologies[0].imports,
        vec!["http://example.org/not-here"]
    );
    let labels: Vec<&str> = index.terms.iter().map(|t| t.label.as_str()).collect();
    for l in ["animal", "plant", "hybrid", "dog", "part of"] {
        assert!(labels.contains(&l), "{l} in {labels:?}");
    }
    let dog = index.terms.iter().find(|t| t.label == "dog").unwrap();
    assert_eq!(dog.parents, vec!["animal"]);
}

#[test]
fn reasoner_finds_the_class_under_two_disjoint_parents() {
    let r = run("onto.ttl");
    let s = section(&r, "logic");
    assert_eq!(metric(s, "consistent (OWL 2 EL)"), "no");
    assert_eq!(metric(s, "unsatisfiable classes"), 1);
    let unsat: Vec<&str> = s
        .findings
        .iter()
        .filter(|f| f.check == "unsatisfiable class")
        .map(|f| f.element.as_str())
        .collect();
    assert_eq!(unsat, vec!["hybrid (Hybrid)"]);
    assert_eq!(metric(s, "imports not loaded"), 1);
}

#[test]
fn recorded_decisions_mark_pitfalls() {
    let r = run("");
    let s = section(&r, "logic");
    let p11: Vec<_> = s
        .findings
        .iter()
        .filter(|f| f.check.starts_with("pitfall P11"))
        .collect();
    assert!(
        !p11.is_empty(),
        "the fixture's object property has no domain or range"
    );
    for f in p11 {
        assert_eq!(f.status.as_deref(), Some("accepted"));
        assert!(
            f.detail.contains("by design") && f.detail.contains("fixture"),
            "{}",
            f.detail
        );
    }
    let open: Vec<_> = s
        .findings
        .iter()
        .filter(|f| f.check.starts_with("pitfall") && f.status.as_deref() == Some("open"))
        .collect();
    assert!(!open.is_empty(), "other pitfalls stay open");
}

#[test]
fn provenance_reads_definitions_from_the_file() {
    let r = run("onto.ttl");
    let s = section(&r, "provenance");
    let none: Vec<&str> = s
        .findings
        .iter()
        .filter(|f| f.check == "no definition")
        .map(|f| f.element.as_str())
        .collect();
    assert_eq!(none, vec!["hybrid", "part of"]);
    let weak: Vec<&str> = s
        .findings
        .iter()
        .filter(|f| f.check == "definition cites a weak source")
        .map(|f| f.element.as_str())
        .collect();
    assert_eq!(weak, vec!["plant"]);
    assert_eq!(
        metric(
            s,
            "definitions with their own source annotation (ontology file)"
        ),
        1
    );
}

#[test]
fn ontology_without_bfo_uses_owl_thing_as_top() {
    let r = run("onto.ttl");
    let s = section(&r, "structure");
    assert_eq!(metric(s, "top class"), "owl:Thing");
    assert_eq!(
        metric(s, "classes without a path to the top class (ontology file)"),
        0
    );
}

#[test]
fn unreadable_file_is_a_finding_not_an_abort() {
    let r = run("broken");
    let s = section(&r, "logic");
    assert_eq!(metric(s, "ontology files read"), 1);
    assert_eq!(metric(s, "ontology files that could not be read"), 1);
    assert!(s
        .findings
        .iter()
        .any(|f| f.check == "file could not be read" && f.element == "bad.owl"));
}

#[test]
fn owl_report_is_deterministic() {
    let a = serde_json::to_string(&run("onto.ttl")).unwrap();
    let b = serde_json::to_string(&run("onto.ttl")).unwrap();
    assert_eq!(a, b);
}

#[test]
fn hierarchy_cycles_are_found_and_part_of_loops_are_not() {
    let r = run("cycles");
    let s = section(&r, "logic");
    let p06: Vec<&str> = s
        .findings
        .iter()
        .filter(|f| f.check.starts_with("pitfall P06"))
        .map(|f| f.element.as_str())
        .collect();
    assert_eq!(p06, vec!["a (A)", "b (B)"]);
}
