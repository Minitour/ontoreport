//! ontoreport: a quality report for OWL ontologies built from the project record.
//!
//! Readers fill one in-memory [`model::Index`]; each section reads the index and returns metrics, chart
//! data and findings; an overview summarizes them; the report is written as JSON (for agents) and HTML
//! (for people). The report is deterministic: the same inputs give the same output.

pub mod desk;
pub mod html;
pub mod model;
pub mod report;
pub mod sections;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use model::{Index, Origin};
use report::{Chart, Direction, Kpi, Node, Overview, Report, Section, Tone};

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
    type Run = fn(&Index) -> Section;
    let runs: [Run; 4] = [
        sections::structure,
        sections::provenance,
        sections::traceability,
        sections::review,
    ];
    let sections: Vec<Section> = std::thread::scope(|scope| {
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
        overview: overview(index, &sections),
        sections,
        input_checks: index.input_checks.clone(),
        recorded: index.recorded.clone(),
    }
}

fn overview(index: &Index, sections: &[Section]) -> Overview {
    let mut kpis = Vec::new();
    let files = index
        .terms
        .iter()
        .filter(|t| t.origin == Origin::Statements)
        .count();
    let cands = index.terms.len() - files;
    kpis.push(Kpi {
        label: "Terms".into(),
        value: thousands(index.terms.len() as u64),
        detail: format!(
            "{} in ontology files · {} desk candidates",
            thousands(files as u64),
            thousands(cands as u64)
        ),
        tone: Tone::Neutral,
    });
    let sources: BTreeSet<&str> = index
        .terms
        .iter()
        .filter_map(|t| t.source.as_deref())
        .collect();
    if !sources.is_empty() {
        kpis.push(Kpi {
            label: "Sources reused".into(),
            value: sources.len().to_string(),
            detail: "ontologies and vocabularies on the desk".into(),
            tone: Tone::Neutral,
        });
    }
    let find = |id: &str| sections.iter().find(|s| s.id == id && s.ran);
    if let Some(s) = find("structure") {
        kpis.push(ratio_kpi("Classes reaching the top class", s));
    }
    if let Some(s) = find("provenance") {
        kpis.push(ratio_kpi("Terms with a definition", s));
    }
    if let Some(s) = find("traceability") {
        kpis.push(ratio_kpi("Groups naming a requirement", s));
    }
    let total: usize = sections.iter().map(|s| s.findings.len()).sum();
    kpis.push(Kpi {
        label: "Findings".into(),
        value: thousands(total as u64),
        detail: sections
            .iter()
            .filter(|s| !s.findings.is_empty())
            .map(|s| format!("{} {}", s.title, s.findings.len()))
            .collect::<Vec<_>>()
            .join(" · "),
        tone: if total == 0 {
            Tone::Good
        } else {
            Tone::Neutral
        },
    });

    // Findings: section -> check -> source.
    let mut roots = Vec::new();
    for s in sections.iter().filter(|s| !s.findings.is_empty()) {
        let mut by_check: BTreeMap<&str, BTreeMap<String, u64>> = BTreeMap::new();
        for f in &s.findings {
            let src = f
                .source
                .clone()
                .unwrap_or_else(|| sections::origin_name(f.origin).to_string());
            *by_check.entry(f.check).or_default().entry(src).or_default() += 1;
        }
        let mut checks: Vec<Node> = by_check
            .into_iter()
            .map(|(check, srcs)| {
                let mut kids: Vec<Node> = srcs
                    .into_iter()
                    .map(|(label, n)| Node {
                        label,
                        values: vec![n],
                        children: Vec::new(),
                    })
                    .collect();
                kids.sort_by(|a, b| {
                    b.values[0]
                        .cmp(&a.values[0])
                        .then_with(|| a.label.cmp(&b.label))
                });
                Node {
                    label: check.to_string(),
                    values: vec![kids.iter().map(|k| k.values[0]).sum()],
                    children: kids,
                }
            })
            .collect();
        checks.sort_by(|a, b| {
            b.values[0]
                .cmp(&a.values[0])
                .then_with(|| a.label.cmp(&b.label))
        });
        roots.push(Node {
            label: s.title.to_string(),
            values: vec![s.findings.len() as u64],
            children: checks,
        });
    }
    Overview {
        kpis,
        findings: Chart::Tree {
            title: "Where the findings are: section › check › source".into(),
            series: vec!["findings".into()],
            roots,
        },
    }
}

/// A KPI from a section's judged ratio charts, summed over origins, with each origin in the detail.
fn ratio_kpi(label: &str, s: &Section) -> Kpi {
    let (mut part, mut whole) = (0u64, 0u64);
    let mut parts = Vec::new();
    for c in &s.charts {
        if let Chart::Ratio {
            title,
            part: p,
            whole: w,
            direction: Direction::HigherIsBetter,
        } = c
        {
            part += p;
            whole += w;
            let origin = title
                .rsplit_once('(')
                .map(|(_, o)| o.trim_end_matches(')').to_string())
                .unwrap_or_default();
            parts.push(if origin.is_empty() {
                format!("{} of {}", thousands(*p), thousands(*w))
            } else {
                format!("{origin}: {} of {}", thousands(*p), thousands(*w))
            });
        }
    }
    let share = if whole == 0 {
        None
    } else {
        Some(part as f64 / whole as f64)
    };
    Kpi {
        label: label.into(),
        value: share
            .map(|x| format!("{:.1}%", 100.0 * x))
            .unwrap_or_else(|| "n/a".into()),
        detail: parts.join(" · "),
        tone: match share {
            None => Tone::Neutral,
            Some(x) if x >= 0.95 => Tone::Good,
            Some(x) if x >= 0.8 => Tone::Warn,
            Some(_) => Tone::Bad,
        },
    }
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
