use std::path::PathBuf;

use ontoreport::releases;
use ontoreport::report::Report;

fn run(with_releases: bool) -> Report {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/imports");
    let index = ontoreport::load(&dir).unwrap();
    if !with_releases {
        return ontoreport::report(&index);
    }
    let tmp = tempfile::tempdir().unwrap();
    let rels = releases::fetch_all(&releases::plans(&index), tmp.path(), true, 100);
    ontoreport::report_with(
        &index,
        &ontoreport::Context {
            releases: Some(&rels),
            ..Default::default()
        },
    )
}

fn without_path(r: &Report) -> Vec<(String, String)> {
    let s = r.sections.iter().find(|s| s.id == "structure").unwrap();
    s.findings
        .iter()
        .filter(|f| f.check == "no path to the top class")
        .map(|f| (f.element.clone(), f.detail.clone()))
        .collect()
}

#[test]
fn an_import_of_bfo_makes_bfo_the_top_class() {
    let r = run(false);
    let s = r.sections.iter().find(|s| s.id == "structure").unwrap();
    assert_eq!(s.metric_value("top class").unwrap(), "BFO entity");
}

#[test]
fn file_classes_are_followed_into_the_release_of_an_import_not_in_the_folder() {
    let found = without_path(&run(true));
    let names: Vec<&str> = found.iter().map(|(e, _)| e.as_str()).collect();
    // buoy and moored buoy reach entity through BFO's release; orphan has no parent, stray a parent no
    // file or release has.
    assert_eq!(names, vec!["orphan", "stray"]);
    let stray = &found.iter().find(|(e, _)| e == "stray").unwrap().1;
    assert!(
        stray.contains("XYZ:0000001 (in no file or import read)"),
        "{stray}"
    );
}

#[test]
fn without_releases_the_import_is_not_followed() {
    let names: Vec<String> = without_path(&run(false))
        .into_iter()
        .map(|(e, _)| e)
        .collect();
    assert!(names.contains(&"buoy".to_string()) && names.contains(&"moored buoy".to_string()));
}
