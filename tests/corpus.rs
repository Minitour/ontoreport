use std::collections::HashMap;
use std::path::PathBuf;

use ontoreport::config::TextForm;
use ontoreport::corpus;
use ontoreport::embed::Vectors;
use ontoreport::report::{Report, Section};

fn with(v: &Vectors) -> ontoreport::Context<'_> {
    ontoreport::Context {
        embeddings: Ok((v, TextForm::Label)),
        releases: None,
    }
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus")
}

fn section<'a>(r: &'a Report, id: &str) -> &'a Section {
    r.sections.iter().find(|s| s.id == id).expect("section")
}

fn metric(s: &Section, name: &str) -> serde_json::Value {
    s.metric_value(name)
        .unwrap_or_else(|| panic!("metric {name}"))
        .clone()
}

fn unit(v: [f32; 3]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter().map(|x| x / n).collect()
}

/// Water bodies point one way, the tomato another.
fn vectors(index: &ontoreport::model::Index) -> Vectors {
    let known: HashMap<&str, [f32; 3]> = [
        ("lake", [1.0, 0.1, 0.0]),
        ("river", [1.0, 0.2, 0.0]),
        ("ocean", [1.0, 0.0, 0.1]),
        ("tomato", [0.0, 0.0, 1.0]),
    ]
    .into_iter()
    .collect();
    let by_text = corpus::texts_needed(index, TextForm::Label)
        .into_iter()
        .map(|t| {
            let v = known.get(t.as_str()).copied().unwrap_or([0.5, 0.5, 0.5]);
            (t, unit(v))
        })
        .collect();
    Vectors {
        model: "test".into(),
        by_text,
        cached: 0,
        computed: 0,
    }
}

#[test]
fn concept_file_in_the_project_is_found_and_normalized() {
    let index = ontoreport::load(&fixture()).unwrap();
    let d = index.domain.as_ref().expect("concepts.txt is read");
    assert_eq!(d.entries, 5);
    assert_eq!(d.concepts["lake"], (30, "Lakes".to_string()));
    assert!(d.concepts.contains_key("mountain"));
}

#[test]
fn coverage_and_relevance_follow_the_2023_definitions() {
    let index = ontoreport::load(&fixture()).unwrap();
    let r = ontoreport::report(&index);
    let s = section(&r, "corpus");
    assert!(s.ran);
    let o = "(ontology file)";
    assert_eq!(metric(s, "domain concepts"), 5);
    assert_eq!(metric(s, &format!("classes compared {o}")), 10);
    // lake, river, mountain
    assert_eq!(metric(s, &format!("shared concepts {o}")), 3);
    // the three, plus water body, landform and environmental feature
    assert_eq!(
        metric(s, &format!("classes in the domain-relevant subset {o}")),
        6
    );
    assert_eq!(metric(s, &format!("domain coverage {o}")), 0.6);
    assert_eq!(metric(s, &format!("ontology relevance {o}")), 0.3);
    assert_eq!(metric(s, &format!("sub-ontology relevance {o}")), 0.5);
    // Without a model there is no coherence.
    assert!(!section(&r, "coherence").ran);
}

#[test]
fn coherence_flags_the_child_that_does_not_fit() {
    let index = ontoreport::load(&fixture()).unwrap();
    let v = vectors(&index);
    let r = ontoreport::report_with(&index, &with(&v));
    let s = section(&r, "coherence");
    assert!(s.ran);
    assert_eq!(metric(s, "concept families scored (ontology file)"), 3);
    let flagged: Vec<&str> = s.findings.iter().map(|f| f.element.as_str()).collect();
    assert_eq!(flagged, vec!["tomato"]);
    assert!(s.findings[0].detail.contains("under 'water body'"));
    let c = section(&r, "corpus");
    assert_eq!(metric(c, "families in the subset (ontology file)"), 2);
}

#[test]
fn reports_with_embeddings_are_deterministic() {
    let index = ontoreport::load(&fixture()).unwrap();
    let v = vectors(&index);
    let a = serde_json::to_string(&ontoreport::report_with(&index, &with(&v))).unwrap();
    let b = serde_json::to_string(&ontoreport::report_with(&index, &with(&v))).unwrap();
    assert_eq!(a, b);
}

#[test]
fn every_new_metric_and_chart_has_help() {
    let index = ontoreport::load(&fixture()).unwrap();
    let v = vectors(&index);
    let r = ontoreport::report_with(&index, &with(&v));
    for id in ["coherence", "corpus"] {
        let s = section(&r, id);
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
}
