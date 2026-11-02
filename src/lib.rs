//! ontoreport: a quality report for OWL ontologies built from the project record.
//!
//! Readers fill one in-memory [`model::Index`]; each section reads the index and returns metrics and
//! findings; the report is written as JSON (for agents) and HTML (for people). The report is
//! deterministic: the same inputs give the same output.

pub mod desk;
pub mod html;
pub mod model;
pub mod report;
pub mod sections;

use std::path::Path;

use model::Index;
use report::Report;

/// The crate version, as published.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Read every input found under `path` (a project or desk data folder) into an index.
pub fn load(path: &Path) -> Result<Index, String> {
    let mut index = Index::default();
    let dirs = [path.to_path_buf(), path.join("data")];
    let mut read = 0;
    for d in dirs.iter().filter(|d| d.is_dir()) {
        read += desk::load(d, &mut index)?.len();
    }
    if read == 0 {
        return Err(format!(
            "{}: no inputs found (expected desk data: g0.json, groups.json or check.json)",
            path.display()
        ));
    }
    Ok(index)
}

/// Run every section on the index. Sections are independent and run in parallel.
pub fn report(index: &Index) -> Report {
    type Run = fn(&Index) -> report::Section;
    let runs: [Run; 4] = [
        sections::structure,
        sections::provenance,
        sections::traceability,
        sections::review,
    ];
    let sections = std::thread::scope(|scope| {
        let handles: Vec<_> = runs.iter().map(|f| scope.spawn(move || f(index))).collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("section thread"))
            .collect()
    });
    Report {
        tool: "ontoreport",
        version: VERSION,
        inputs: index.inputs.clone(),
        sections,
        input_checks: index.input_checks.clone(),
        recorded: index.recorded.clone(),
    }
}
