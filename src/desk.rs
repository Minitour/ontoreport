//! Reader for review-desk data: `g0.json` (statements of an ontology file), `groups.json` (candidate terms)
//! and `check.json` (the desk's own self-check). Any of the three may be missing.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::model::{GroupRef, Index, InputCheck, Origin, RecordedFinding, Term};

#[derive(Debug, Deserialize)]
struct Page {
    title: String,
    #[serde(default)]
    iri: Option<String>,
    #[serde(default)]
    cu: Option<String>,
    #[serde(default)]
    k: Option<String>,
    #[serde(default)]
    lines: Vec<Line>,
}

#[derive(Debug, Deserialize)]
struct Line {
    #[serde(default)]
    pl: Option<String>,
    #[serde(default)]
    ol: Option<String>,
    #[serde(default)]
    k: Option<String>,
    #[serde(default)]
    lv: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct Groups {
    #[serde(default)]
    sources: Vec<Source>,
    #[serde(default)]
    chain: BTreeMap<String, ChainEntry>,
    #[serde(default)]
    root_errors: Vec<RootError>,
}

#[derive(Debug, Deserialize)]
struct Source {
    source: String,
    #[serde(default)]
    release: Option<String>,
    #[serde(default)]
    categories: Vec<Category>,
}

#[derive(Debug, Deserialize)]
struct Category {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    groups: Vec<Group>,
}

#[derive(Debug, Deserialize)]
struct Group {
    id: String,
    name: String,
    #[serde(default)]
    terms: Vec<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    serves: Option<String>,
    #[serde(default)]
    why: Option<String>,
    #[serde(default)]
    info: BTreeMap<String, Info>,
}

#[derive(Debug, Deserialize)]
struct Info {
    #[serde(default)]
    i: Option<String>,
    #[serde(default)]
    u: Option<String>,
    #[serde(default)]
    d: Option<String>,
    #[serde(default)]
    ds: Option<Value>,
    #[serde(default)]
    p: Vec<String>,
    #[serde(default)]
    k: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChainEntry {
    #[serde(default)]
    p: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RootError {
    term: String,
    #[serde(default)]
    section: String,
    #[serde(default)]
    why: String,
}

#[derive(Debug, Deserialize)]
struct Check {
    #[serde(default)]
    checks: Vec<CheckItem>,
}

#[derive(Debug, Deserialize)]
struct CheckItem {
    name: String,
    ok: bool,
    #[serde(default)]
    summary: String,
}

/// Definition-like predicates. BFO classes carry an elucidation instead of a definition.
const DEFINITION_PREDICATES: &[&str] = &[
    "definition",
    "elucidation",
    "IAO:0000115",
    "skos:definition",
];

/// Load every desk file found in `dir` into `index`. Returns the names of the files read.
pub fn load(dir: &Path, index: &mut Index) -> Result<Vec<String>, String> {
    let mut read = Vec::new();
    let g0 = dir.join("g0.json");
    if g0.is_file() {
        let pages: Vec<Page> = parse(&g0)?;
        index
            .terms
            .extend(pages.into_iter().filter_map(statement_term));
        read.push(g0.display().to_string());
    }
    let groups = dir.join("groups.json");
    if groups.is_file() {
        let g: Groups = parse(&groups)?;
        load_groups(g, index);
        read.push(groups.display().to_string());
    }
    let check = dir.join("check.json");
    if check.is_file() {
        let c: Check = parse(&check)?;
        index
            .input_checks
            .extend(c.checks.into_iter().map(|c| InputCheck {
                name: c.name,
                ok: c.ok,
                summary: c.summary,
            }));
        read.push(check.display().to_string());
    }
    index.inputs.extend(read.iter().cloned());
    Ok(read)
}

fn parse<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn statement_term(p: Page) -> Option<Term> {
    let kind = p.k.unwrap_or_default();
    if kind == "ontology" {
        return None;
    }
    let mut parents = Vec::new();
    let mut definitions = Vec::new();
    let mut definition_sources = Vec::new();
    for l in &p.lines {
        let pl = l.pl.as_deref().unwrap_or("");
        if pl == "rdfs:subClassOf" || pl == "rdfs:subPropertyOf" {
            if let Some(o) = &l.ol {
                parents.push(o.to_lowercase());
            }
        } else if l.k.as_deref() == Some("definition") || DEFINITION_PREDICATES.contains(&pl) {
            definitions.push(value_text(l));
        } else if pl == "definition source" || pl == "IAO:0000119" {
            definition_sources.push(value_text(l));
        }
    }
    Some(Term {
        key: p.title.to_lowercase(),
        labels: vec![p.title.clone()],
        parent_iris: Vec::new(),
        label: p.title,
        curie: p.cu,
        iri: p.iri,
        kind,
        definitions,
        definition_sources,
        parents,
        origin: Origin::Statements,
        source: None,
        release: None,
        group: None,
    })
}

fn value_text(l: &Line) -> String {
    match &l.lv {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => l.ol.clone().unwrap_or_default(),
    }
}

fn load_groups(g: Groups, index: &mut Index) {
    for s in g.sources {
        for c in s.categories {
            for gr in &c.groups {
                let group = GroupRef {
                    id: gr.id.clone(),
                    name: gr.name.clone(),
                    category: c.name.clone(),
                    status: gr.status.clone(),
                    serves: gr.serves.clone().filter(|x| !x.trim().is_empty()),
                    why: gr.why.clone().filter(|x| !x.trim().is_empty()),
                };
                for label in &gr.terms {
                    let info = gr.info.get(label);
                    index.terms.push(Term {
                        key: label.to_lowercase(),
                        labels: vec![label.clone()],
                        parent_iris: Vec::new(),
                        label: label.clone(),
                        curie: info.and_then(|i| i.i.clone()),
                        iri: info.and_then(|i| i.u.clone()),
                        kind: info.and_then(|i| i.k.clone()).unwrap_or_default(),
                        definitions: info
                            .and_then(|i| i.d.clone())
                            .filter(|d| !d.trim().is_empty())
                            .into_iter()
                            .collect(),
                        definition_sources: info
                            .map(|i| strings(i.ds.as_ref()))
                            .unwrap_or_default(),
                        parents: info
                            .map(|i| i.p.iter().map(|p| p.to_lowercase()).collect())
                            .unwrap_or_default(),
                        origin: Origin::Candidates,
                        source: Some(s.source.clone()),
                        release: s.release.clone(),
                        group: Some(group.clone()),
                    });
                }
            }
        }
    }
    for (label, entry) in g.chain {
        index.extra_parents.insert(
            label.to_lowercase(),
            entry.p.iter().map(|p| p.to_lowercase()).collect(),
        );
    }
    index
        .recorded
        .extend(g.root_errors.into_iter().map(|e| RecordedFinding {
            check: "no path to the top class".into(),
            element: e.term,
            detail: format!("{} ({})", e.why, e.section),
        }));
}

fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .filter(|s| !s.trim().is_empty())
            .collect(),
        Some(Value::String(s)) if !s.trim().is_empty() => vec![s.clone()],
        _ => Vec::new(),
    }
}
