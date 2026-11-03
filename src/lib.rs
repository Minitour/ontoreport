//! ontoreport: a quality report for OWL ontologies built from the project record.
//!
//! Readers fill one in-memory [`model::Index`]; each section reads the index and returns metrics, chart
//! data and findings; an overview summarizes them; the report is written as JSON (for agents) and HTML
//! (for people). The report is deterministic: the same inputs give the same output.

pub mod config;
pub mod corpus;
pub mod desk;
pub mod embed;
pub mod help;
pub mod html;
pub mod model;
pub mod net;
pub mod owl;
pub mod releases;
pub mod report;
pub mod reuse;
pub mod sections;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use model::{Index, Origin};
use report::{Chart, Direction, Kpi, Node, Overview, Report, Section, Tone};

/// The crate version, as published.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Folders searched for inputs, relative to the project folder.
const INPUT_DIRS: &[&str] = &[
    "",
    "data",
    "ontology",
    "ontologies",
    "src/ontology",
    "qc",
    "corpus",
];

/// Domain-concept files looked for in the input folders, in this order.
const CONCEPT_FILES: &[&str] = &[
    "concepts.json",
    "concepts.txt",
    "domain-concepts.json",
    "domain-concepts.txt",
];

/// Read every input found under `path` into an index. `path` is a project folder, a review-desk data
/// folder, or a single ontology file. Ontology files are read in parallel.
pub fn load(path: &Path) -> Result<Index, String> {
    let mut index = Index::default();
    let mut owl_files: Vec<PathBuf> = Vec::new();
    if path.is_file() {
        if !owl::is_ontology_file(path) {
            return Err(format!(
                "{}: not an ontology file ({})",
                path.display(),
                owl::EXTENSIONS.join(", ")
            ));
        }
        owl_files.push(path.to_path_buf());
    } else {
        for rel in INPUT_DIRS {
            let d = if rel.is_empty() {
                path.to_path_buf()
            } else {
                path.join(rel)
            };
            if !d.is_dir() {
                continue;
            }
            desk::load(&d, &mut index)?;
            let decisions = d.join("decisions.json");
            if decisions.is_file() {
                let text = fs::read_to_string(&decisions)
                    .map_err(|e| format!("{}: {e}", decisions.display()))?;
                let parsed: DecisionsFile = serde_json::from_str(&text)
                    .map_err(|e| format!("{}: {e}", decisions.display()))?;
                index
                    .decisions
                    .extend(parsed.pitfalls.into_iter().chain(parsed.findings));
                index.inputs.push(decisions.display().to_string());
            }
            let specs = d.join("releases.toml");
            if specs.is_file() {
                index.release_specs.extend(releases::load_specs(&specs)?);
                index.inputs.push(specs.display().to_string());
            }
            if index.domain.is_none() {
                if let Some(c) = CONCEPT_FILES
                    .iter()
                    .map(|f| d.join(f))
                    .find(|p| p.is_file())
                {
                    index.domain = Some(corpus::load_concepts(&c)?);
                    index.inputs.push(c.display().to_string());
                }
            }
            let mut found: Vec<PathBuf> = fs::read_dir(&d)
                .map_err(|e| format!("{}: {e}", d.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| owl::is_ontology_file(p))
                .collect();
            found.sort();
            owl_files.extend(found);
        }
    }
    let loaded: Vec<Result<owl::LoadedOntology, String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = owl_files
            .iter()
            .map(|p| scope.spawn(move || owl::load(p)))
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err("the OWL reader failed on this file".to_string()))
            })
            .collect()
    });
    for (p, lo) in owl_files.iter().zip(loaded) {
        match lo {
            Ok(lo) => {
                index.inputs.push(lo.path.display().to_string());
                index.ontologies.push(lo);
            }
            // One unreadable file does not stop the report; it becomes a finding.
            Err(e) => index.unreadable.push((p.display().to_string(), e)),
        }
    }
    let owl_terms = owl::terms(&index.ontologies);
    index.terms.splice(0..0, owl_terms);
    if index.inputs.is_empty() {
        if let Some((p, e)) = index.unreadable.first() {
            return Err(format!("{p}: could not be read: {e}"));
        }
        return Err(format!(
            "{}: no inputs found (expected ontology files: {}; or review-desk data: g0.json, groups.json, check.json)",
            path.display(),
            owl::EXTENSIONS.join(", ")
        ));
    }
    Ok(index)
}

#[derive(serde::Deserialize)]
struct DecisionsFile {
    #[serde(default)]
    pitfalls: Vec<model::Decision>,
    #[serde(default)]
    findings: Vec<model::Decision>,
}

/// What a report can use beyond the index: embeddings and source releases, both fetched by the caller.
pub struct Context<'a> {
    pub embeddings: corpus::Embeddings<'a>,
    pub releases: Option<&'a releases::Releases>,
}

impl Default for Context<'_> {
    fn default() -> Self {
        Context {
            embeddings: Err("Run without a sentence-embedding model.".to_string()),
            releases: None,
        }
    }
}

/// Run every section on the index, without embeddings or releases: the coherence section is marked not
/// run, corpus fit reports coverage and relevance only, and reuse fidelity checks only identifiers.
pub fn report(index: &Index) -> Report {
    report_with(index, &Context::default())
}

/// Run every section on the index. Sections are independent and run in parallel.
pub fn report_with(index: &Index, ctx: &Context) -> Report {
    let emb = &ctx.embeddings;
    let rels = ctx.releases;
    type Run = fn(&Index) -> Section;
    let runs: [Run; 5] = [
        sections::logic,
        sections::structure,
        sections::provenance,
        sections::traceability,
        sections::review,
    ];
    let mut sections: Vec<Section> = std::thread::scope(|scope| {
        let mut handles: Vec<_> = runs.iter().map(|f| scope.spawn(move || f(index))).collect();
        handles.insert(3, scope.spawn(move || reuse::reuse(index, rels)));
        handles.push(scope.spawn(move || corpus::coherence(index, emb)));
        handles.push(scope.spawn(move || corpus::corpus_fit(index, emb)));
        handles
            .into_iter()
            .map(|h| h.join().expect("section thread"))
            .collect()
    });
    apply_decisions(index, &mut sections);
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

/// Mark findings the project has recorded a decision about (decisions.json, by check name and element).
/// Pitfalls are matched by their code in the consistency section itself.
fn apply_decisions(index: &Index, sections: &mut [Section]) {
    if index.decisions.is_empty() {
        return;
    }
    for s in sections.iter_mut() {
        let mut decided = 0;
        for f in &mut s.findings {
            if f.check.starts_with("pitfall ") {
                continue;
            }
            let Some(d) = index
                .decisions
                .iter()
                .find(|d| d.covers(&f.check, &f.element))
            else {
                continue;
            };
            decided += 1;
            let why = if d.reference.is_empty() {
                d.reason.clone()
            } else {
                format!("{} ({})", d.reason, d.reference)
            };
            f.detail = format!("{}: {why}. Found: {}", d.decision, f.detail);
            f.status = Some(d.decision.clone());
        }
        if decided > 0 {
            s.notes.push(format!(
                "{decided} finding(s) carry a decision recorded in decisions.json; their status shows the decision."
            ));
        }
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
        help: None,
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
            help: None,
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
        help: None,
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
            *by_check
                .entry(f.check.as_str())
                .or_default()
                .entry(src)
                .or_default() += 1;
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
    for k in &mut kpis {
        k.help = help::about(&k.label);
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
        help: None,
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
