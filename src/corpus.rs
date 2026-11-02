//! Semantic coherence and corpus fit: the measures of Zaitoun, Sagi and Hose, "Automated Ontology
//! Evaluation: Evaluating Coverage and Correctness using a Domain Corpus" (WWW '23 Companion).
//!
//! Corpus fit compares the ontology's classes with a list of concepts extracted from a domain corpus:
//!
//! ```text
//! domain coverage        = S / D    S: shared concepts, D: domain concepts
//! ontology relevance     = S / O    O: classes in the ontology
//! sub-ontology relevance = S / H    H: classes in the domain-relevant subset
//! ```
//!
//! A concept is shared when a class label and a domain concept are equal after lower-casing and
//! reducing plural nouns to the singular. The domain-relevant subset holds the classes with a shared
//! label, all their ancestors, and their direct children.
//!
//! Coherence scores each concept family (a parent with two or more direct children) with the cosine
//! similarity of sentence embeddings:
//!
//! ```text
//! CSS = mean similarity over every pair of siblings
//! PSS = mean similarity between the parent and each child
//! PDA = 1 - sample standard deviation of the parent-child similarities
//! ```
//!
//! A child "may not belong" when removing it from its family raises the family's scores: the 2023
//! validation protocol computes diff = 0.05·(PDA with − PDA without) + 0.95·(CSS with − CSS without)
//! and predicts membership when diff ≥ −0.012. On the 326 expert-labelled ENVO pairs of 2023 that rule
//! agrees with the experts 85% of the time with all-MiniLM-L6-v2 on labels and 89% on labels with
//! definitions (AUC 0.92 and 0.95).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::config::TextForm;
use crate::embed::Vectors;
use crate::model::{Index, Origin, Term};
use crate::report::{bars, Chart, Direction, Finding, Row, Section};
use crate::sections::{in_scope, origin_name, status};

/// The 2023 decision threshold on diff.
pub const MEMBERSHIP_THRESHOLD: f64 = -0.012;

// ── Domain concepts ────────────────────────────────────────────────────────

/// Concepts extracted from a domain corpus, keyed by normalized form.
#[derive(Debug, Clone)]
pub struct DomainConcepts {
    pub path: String,
    /// Normalized form -> (papers or other frequency, the concept as written).
    pub concepts: BTreeMap<String, (u64, String)>,
    /// Entries in the file before normalization merged any.
    pub entries: usize,
}

/// Read a domain-concept file. Accepted forms:
/// - the 2022 extraction output, `{"concepts": [{"concept_name": ..., "occurrences_in_papers": ...}]}`;
/// - a JSON list of strings, or of objects with `concept` (or `name`) and `frequency` (or `papers`);
/// - text with one concept per line, optionally followed by a tab and a frequency; `#` starts a comment.
pub fn load_concepts(path: &Path) -> Result<DomainConcepts, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut raw: Vec<(String, u64)> = Vec::new();
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        let v: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let list = match &v {
            Value::Object(o) => o.get("concepts").and_then(|c| c.as_array()),
            Value::Array(a) => Some(a),
            _ => None,
        }
        .ok_or_else(|| format!("{}: expected a list of concepts", path.display()))?;
        for item in list {
            match item {
                Value::String(s) => raw.push((s.clone(), 1)),
                Value::Object(o) => {
                    let name = ["concept_name", "concept", "name"]
                        .iter()
                        .find_map(|k| o.get(*k).and_then(|x| x.as_str()));
                    let freq = ["occurrences_in_papers", "papers", "frequency", "count"]
                        .iter()
                        .find_map(|k| o.get(*k).and_then(|x| x.as_u64()))
                        .unwrap_or(1);
                    if let Some(n) = name {
                        raw.push((n.to_string(), freq));
                    }
                }
                _ => {}
            }
        }
    } else {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (name, freq) = match line.split_once('\t') {
                Some((n, f)) => (n, f.trim().parse::<f64>().map(|x| x as u64).unwrap_or(1)),
                None => (line, 1),
            };
            raw.push((name.to_string(), freq));
        }
    }
    let entries = raw.len();
    let mut concepts: BTreeMap<String, (u64, String)> = BTreeMap::new();
    for (name, freq) in raw {
        let n = normalize(&name);
        if n.is_empty() {
            continue;
        }
        let e = concepts.entry(n).or_insert((0, name.clone()));
        if freq > e.0 {
            *e = (freq, name);
        }
    }
    if concepts.is_empty() {
        return Err(format!("{}: no concepts found", path.display()));
    }
    Ok(DomainConcepts {
        path: path.display().to_string(),
        concepts,
        entries,
    })
}

// ── Normalization ──────────────────────────────────────────────────────────

/// Lower-case each word and reduce plural nouns to the singular, keeping punctuation inside a word
/// ("Sea-Waters" -> "sea-water"). The 2023 work used spaCy's lemmatizer on each word; this is a fixed
/// rule set with the irregular plurals common in science, so it needs no language model and gives the
/// same result everywhere.
pub fn normalize(label: &str) -> String {
    label
        .split_whitespace()
        .map(|w| {
            let w = w.to_lowercase();
            let mut out = String::with_capacity(w.len());
            let mut run = String::new();
            for c in w.chars() {
                if c.is_alphabetic() {
                    run.push(c);
                } else {
                    out.push_str(&singular(&run));
                    run.clear();
                    out.push(c);
                }
            }
            out.push_str(&singular(&run));
            out
        })
        .collect::<Vec<_>>()
        .join(" ")
}

const IRREGULAR: &[(&str, &str)] = &[
    ("algae", "alga"),
    ("analyses", "analysis"),
    ("antennae", "antenna"),
    ("apices", "apex"),
    ("appendices", "appendix"),
    ("atlases", "atlas"),
    ("axes", "axis"),
    ("bacteria", "bacterium"),
    ("bases", "basis"),
    ("biases", "bias"),
    ("buses", "bus"),
    ("calves", "calf"),
    ("censuses", "census"),
    ("children", "child"),
    ("cortices", "cortex"),
    ("crises", "crisis"),
    ("criteria", "criterion"),
    ("curricula", "curriculum"),
    ("data", "datum"),
    ("diagnoses", "diagnosis"),
    ("ellipses", "ellipsis"),
    ("emphases", "emphasis"),
    ("feet", "foot"),
    ("foci", "focus"),
    ("formulae", "formula"),
    ("fungi", "fungus"),
    ("gases", "gas"),
    ("geese", "goose"),
    ("genera", "genus"),
    ("halves", "half"),
    ("hypotheses", "hypothesis"),
    ("indices", "index"),
    ("knives", "knife"),
    ("larvae", "larva"),
    ("leaves", "leaf"),
    ("lenses", "lens"),
    ("lice", "louse"),
    ("lives", "life"),
    ("loci", "locus"),
    ("matrices", "matrix"),
    ("maxima", "maximum"),
    ("media", "medium"),
    ("men", "man"),
    ("mice", "mouse"),
    ("minima", "minimum"),
    ("nuclei", "nucleus"),
    ("oases", "oasis"),
    ("optima", "optimum"),
    ("parentheses", "parenthesis"),
    ("phenomena", "phenomenon"),
    ("radii", "radius"),
    ("selves", "self"),
    ("shelves", "shelf"),
    ("spectra", "spectrum"),
    ("stimuli", "stimulus"),
    ("strata", "stratum"),
    ("syntheses", "synthesis"),
    ("teeth", "tooth"),
    ("theses", "thesis"),
    ("vertebrae", "vertebra"),
    ("vertices", "vertex"),
    ("viruses", "virus"),
    ("wives", "wife"),
    ("wolves", "wolf"),
    ("women", "woman"),
];

/// Words ending in "s" that are not plurals.
const SINGULAR_S: &[&str] = &[
    "afterwards",
    "alias",
    "always",
    "atlas",
    "basis",
    "bias",
    "biomass",
    "canvas",
    "chaos",
    "cosmos",
    "diabetes",
    "downwards",
    "gas",
    "has",
    "herpes",
    "its",
    "lens",
    "mathematics",
    "means",
    "measles",
    "news",
    "pancreas",
    "perhaps",
    "physics",
    "rabies",
    "series",
    "species",
    "status",
    "this",
    "thus",
    "towards",
    "upwards",
    "was",
    "whereas",
];

fn singular(w: &str) -> String {
    if w.len() < 3 {
        return w.to_string();
    }
    if let Ok(i) = IRREGULAR.binary_search_by(|(p, _)| p.cmp(&w)) {
        return IRREGULAR[i].1.to_string();
    }
    if SINGULAR_S.contains(&w) {
        return w.to_string();
    }
    for (suffix, keep) in [
        ("sses", 2),
        ("shes", 2),
        ("ches", 2),
        ("xes", 1),
        ("zzes", 2),
    ] {
        if w.ends_with(suffix) {
            return w[..w.len() - suffix.len() + keep].to_string();
        }
    }
    if w.len() > 4 && w.ends_with("ies") {
        return format!("{}y", &w[..w.len() - 3]);
    }
    if w.ends_with('s')
        && !w.ends_with("ss")
        && !w.ends_with("us")
        && !w.ends_with("is")
        && !w.ends_with("ics")
    {
        return w[..w.len() - 1].to_string();
    }
    w.to_string()
}

// ── The class hierarchy ────────────────────────────────────────────────────

/// The classes of one origin, with parent and child links resolved to node numbers.
pub struct Hierarchy<'a> {
    pub nodes: Vec<&'a Term>,
    pub parents: Vec<Vec<usize>>,
    pub children: Vec<Vec<usize>>,
}

impl<'a> Hierarchy<'a> {
    pub fn new(index: &'a Index, origin: Origin) -> Self {
        let nodes: Vec<&Term> = index
            .terms
            .iter()
            .filter(|t| t.origin == origin && t.kind == "class" && in_scope(t))
            .collect();
        let mut by_iri: HashMap<&str, usize> = HashMap::new();
        let mut by_key: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, t) in nodes.iter().enumerate() {
            if let Some(iri) = &t.iri {
                by_iri.insert(iri.as_str(), i);
            }
            by_key.entry(t.key.as_str()).or_default().push(i);
        }
        let mut parents: Vec<Vec<usize>> = nodes
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut ps: Vec<usize> = if t.parent_iris.is_empty() {
                    t.parents
                        .iter()
                        .flat_map(|k| by_key.get(k.as_str()).into_iter().flatten().copied())
                        .collect()
                } else {
                    t.parent_iris
                        .iter()
                        .filter_map(|p| by_iri.get(p.as_str()).copied())
                        .collect()
                };
                ps.retain(|&p| p != i);
                ps.sort_unstable();
                ps.dedup();
                ps
            })
            .collect();
        let mut children = vec![Vec::new(); nodes.len()];
        for (c, ps) in parents.iter().enumerate() {
            for &p in ps {
                children[p].push(c);
            }
        }
        parents.shrink_to_fit();
        Hierarchy {
            nodes,
            parents,
            children,
        }
    }

    /// Parents with at least two direct children, in node order.
    pub fn families(&self) -> Vec<(usize, Vec<usize>)> {
        self.children
            .iter()
            .enumerate()
            .filter(|(_, c)| c.len() >= 2)
            .map(|(p, c)| (p, c.clone()))
            .collect()
    }

    fn ancestors(&self, start: &BTreeSet<usize>) -> BTreeSet<usize> {
        let mut seen = start.clone();
        let mut stack: Vec<usize> = start.iter().copied().collect();
        while let Some(n) = stack.pop() {
            for &p in &self.parents[n] {
                if seen.insert(p) {
                    stack.push(p);
                }
            }
        }
        seen
    }

    pub fn text(&self, node: usize, form: TextForm) -> String {
        text_of(self.nodes[node], form)
    }
}

/// What is embedded for a class: its label, or its label and first definition.
pub fn text_of(t: &Term, form: TextForm) -> String {
    match (form, t.definitions.first()) {
        (TextForm::LabelDefinition, Some(d)) if !d.trim().is_empty() => {
            format!("{}: {}", t.label, d.trim())
        }
        _ => t.label.clone(),
    }
}

fn origins(index: &Index) -> Vec<Origin> {
    [Origin::Statements, Origin::Candidates]
        .into_iter()
        .filter(|o| index.has(*o))
        .collect()
}

/// Every text the coherence and corpus-fit sections will ask the model for.
pub fn texts_needed(index: &Index, form: TextForm) -> Vec<String> {
    let mut out = BTreeSet::new();
    for o in origins(index) {
        let h = Hierarchy::new(index, o);
        for (p, kids) in h.families() {
            out.insert(h.text(p, form));
            for c in kids {
                out.insert(h.text(c, form));
            }
        }
    }
    out.into_iter().collect()
}

// ── Family scores ──────────────────────────────────────────────────────────

/// CSS, PSS and PDA of one family, with each child's diff when the family has three or more children.
#[derive(Debug, Clone)]
pub struct FamilyScore {
    pub css: f64,
    pub pss: f64,
    pub pda: f64,
    /// Per child: diff = 0.05·(PDA with − PDA without) + 0.95·(CSS with − CSS without), and the CSS
    /// without that child. Empty for two-child families, where removing a child leaves no pair.
    pub diffs: Vec<(f64, f64)>,
}

fn dot(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum()
}

fn std_sample(sum: f64, sumsq: f64, n: f64) -> f64 {
    if n < 2.0 {
        return 0.0;
    }
    ((sumsq - sum * sum / n) / (n - 1.0)).max(0.0).sqrt()
}

/// Score a family from unit vectors: `parent` and its `kids` (at least two).
pub fn score_family(parent: &[f32], kids: &[&[f32]]) -> FamilyScore {
    let n = kids.len();
    let mut row = vec![0.0f64; n];
    let mut pair_sum = 0.0;
    for i in 0..n {
        for j in i + 1..n {
            let s = dot(kids[i], kids[j]);
            row[i] += s;
            row[j] += s;
            pair_sum += s;
        }
    }
    let pairs = |m: usize| (m * (m - 1) / 2) as f64;
    let css = pair_sum / pairs(n);
    let ps: Vec<f64> = kids.iter().map(|k| dot(parent, k)).collect();
    let (psum, psq) = ps.iter().fold((0.0, 0.0), |(a, b), x| (a + x, b + x * x));
    let nf = n as f64;
    let pss = psum / nf;
    let pda = 1.0 - std_sample(psum, psq, nf);
    let mut diffs = Vec::new();
    if n >= 3 {
        for i in 0..n {
            let css_wo = (pair_sum - row[i]) / pairs(n - 1);
            let pda_wo = 1.0 - std_sample(psum - ps[i], psq - ps[i] * ps[i], nf - 1.0);
            diffs.push((0.05 * (pda - pda_wo) + 0.95 * (css - css_wo), css_wo));
        }
    }
    FamilyScore {
        css,
        pss,
        pda,
        diffs,
    }
}

fn r3(x: f64) -> Value {
    Value::from((x * 1000.0).round() / 1000.0)
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        f64::NAN
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

/// Histogram buckets for scores in [0, 1] (anything outside is clamped), as chart rows.
fn buckets(scores_by_series: &[Vec<f64>]) -> Vec<Row> {
    (0..10)
        .rev()
        .map(|b| Row {
            label: format!("{:.1}–{:.1}", b as f64 / 10.0, (b + 1) as f64 / 10.0),
            values: scores_by_series
                .iter()
                .map(|xs| {
                    xs.iter()
                        .filter(|x| ((x.clamp(0.0, 0.9999) * 10.0) as usize) == b)
                        .count() as u64
                })
                .collect(),
        })
        .collect()
}

/// The embeddings a report runs with, or why it has none.
pub type Embeddings<'a> = Result<(&'a Vectors, TextForm), String>;

/// How the vectors were made. Whether they came from the cache is left out, so a rerun gives the same
/// report; the command line prints it instead.
fn vectors_note(v: &Vectors, form: TextForm) -> String {
    format!(
        "Embeddings: {} on {} texts; classes are embedded as {}.",
        v.model,
        v.by_text.len(),
        form.name()
    )
}

// ── Section: semantic coherence ────────────────────────────────────────────

pub fn coherence(index: &Index, emb: &Embeddings) -> Section {
    const ID: &str = "coherence";
    const TITLE: &str = "Semantic coherence";
    let (vectors, form) = match emb {
        Ok((v, f)) => (*v, *f),
        Err(why) => return Section::not_run(ID, TITLE, why.clone()),
    };
    let mut s = Section::new(ID, TITLE);
    let mut series = Vec::new();
    let mut css_all = Vec::new();
    let mut flagged_by: BTreeMap<String, u64> = BTreeMap::new();
    for origin in origins(index) {
        let h = Hierarchy::new(index, origin);
        let fams = h.families();
        if fams.is_empty() {
            continue;
        }
        let o = origin_name(origin);
        let (mut css, mut pss, mut pda) = (Vec::new(), Vec::new(), Vec::new());
        let mut flagged = 0usize;
        for (p, kids) in &fams {
            let Some(pv) = vectors.get(&h.text(*p, form)) else {
                continue;
            };
            let kv: Option<Vec<&[f32]>> = kids
                .iter()
                .map(|&c| vectors.get(&h.text(c, form)))
                .collect();
            let Some(kv) = kv else { continue };
            let sc = score_family(pv, &kv);
            css.push(sc.css);
            pss.push(sc.pss);
            pda.push(sc.pda);
            for (k, (diff, css_wo)) in kids.iter().zip(&sc.diffs) {
                if *diff < MEMBERSHIP_THRESHOLD {
                    flagged += 1;
                    let child = h.nodes[*k];
                    let parent = h.nodes[*p];
                    *flagged_by.entry(parent.label.clone()).or_default() += 1;
                    s.findings.push(Finding {
                        check: "child may not belong under its parent".into(),
                        element: child.label.clone(),
                        origin,
                        source: child.source.clone(),
                        status: status(child),
                        detail: format!(
                            "under '{}' ({} children): without it, sibling similarity rises from {:.2} to {:.2}; diff {:.3}, below the 2023 threshold of {}",
                            parent.label,
                            kids.len(),
                            sc.css,
                            css_wo,
                            diff,
                            MEMBERSHIP_THRESHOLD
                        ),
                    });
                }
            }
        }
        s.metric(format!("concept families scored ({o})"), css.len());
        s.metric(format!("mean CSS ({o})"), r3(mean(&css)));
        s.metric(format!("mean PSS ({o})"), r3(mean(&pss)));
        s.metric(format!("mean PDA ({o})"), r3(mean(&pda)));
        s.metric(format!("children that may not belong ({o})"), flagged);
        series.push(o.to_string());
        css_all.push(css);
    }
    if series.is_empty() {
        return Section::not_run(
            ID,
            TITLE,
            "No class has two or more direct children, so there is no concept family to score.",
        );
    }
    s.charts.push(Chart::Stacked {
        title: "Concept families by sibling similarity (CSS)".into(),
        rows: buckets(&css_all),
        series,
    });
    s.charts.push(bars(
        "Parents with the most children that may not belong (top 12)",
        flagged_by,
        12,
    ));
    s.notes.push(vectors_note(vectors, form));
    s.notes.push(
        "A concept family is a parent with two or more direct children. A child is flagged when taking it out \
         of its family raises the family's sibling similarity and agreement (the 2023 diff below −0.012); \
         families with two children have no such test. Flagged children are candidates for review, not errors: \
         the rule agreed with domain experts on 85–89% of the 2023 ENVO pairs."
            .into(),
    );
    s.finish()
}

// ── Section: corpus fit ────────────────────────────────────────────────────

pub fn corpus_fit(index: &Index, emb: &Embeddings) -> Section {
    const ID: &str = "corpus";
    const TITLE: &str = "Corpus fit";
    let Some(domain) = &index.domain else {
        return Section::not_run(
            ID,
            TITLE,
            "No domain-concept list: add concepts.json or concepts.txt to the project, or pass --concepts.",
        );
    };
    let mut s = Section::new(ID, TITLE);
    let d = domain.concepts.len();
    s.metric("domain concepts", d);
    let mut any = false;
    for origin in origins(index) {
        let h = Hierarchy::new(index, origin);
        if h.nodes.is_empty() {
            continue;
        }
        any = true;
        let o = origin_name(origin);
        // Normalized label -> classes carrying it.
        let mut names: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, t) in h.nodes.iter().enumerate() {
            let labels: &[String] = if t.labels.is_empty() {
                std::slice::from_ref(&t.label)
            } else {
                &t.labels
            };
            for l in labels {
                let n = normalize(l);
                if !n.is_empty() {
                    names.entry(n).or_default().push(i);
                }
            }
        }
        let shared_names: Vec<&String> = names
            .keys()
            .filter(|n| domain.concepts.contains_key(*n))
            .collect();
        let shared: BTreeSet<usize> = shared_names
            .iter()
            .flat_map(|n| names[*n].iter().copied())
            .collect();
        let pool = h.ancestors(&shared);
        let mut subset = pool.clone();
        for &c in &shared {
            subset.extend(h.children[c].iter().copied());
        }
        let (sn, on, hn) = (shared_names.len(), h.nodes.len(), subset.len());
        let ratio = |a: usize, b: usize| {
            if b == 0 {
                Value::Null
            } else {
                r3(a as f64 / b as f64)
            }
        };
        s.metric(format!("classes compared ({o})"), on);
        s.metric(format!("shared concepts ({o})"), sn);
        s.metric(format!("classes in the domain-relevant subset ({o})"), hn);
        s.metric(format!("domain coverage ({o})"), ratio(sn, d));
        s.metric(format!("ontology relevance ({o})"), ratio(sn, on));
        s.metric(format!("sub-ontology relevance ({o})"), ratio(sn, hn));
        s.charts.push(Chart::Ratio {
            title: format!("Domain coverage ({o})"),
            part: sn as u64,
            whole: d as u64,
            direction: Direction::HigherIsBetter,
        });
        s.charts.push(Chart::Ratio {
            title: format!("Ontology relevance ({o})"),
            part: sn as u64,
            whole: on as u64,
            direction: Direction::HigherIsBetter,
        });
        s.charts.push(Chart::Ratio {
            title: format!("Sub-ontology relevance ({o})"),
            part: sn as u64,
            whole: hn as u64,
            direction: Direction::HigherIsBetter,
        });
        let missing = domain
            .concepts
            .iter()
            .filter(|(n, _)| !names.contains_key(*n))
            .map(|(_, (f, written))| (written.clone(), *f));
        s.charts.push(bars(
            format!("Frequent domain concepts the ontology lacks, by papers ({o}) (top 20)"),
            missing,
            20,
        ));

        // Coherence of the domain-relevant subset, as in 2023: a family is a parent in the subset with the
        // children that the subset keeps (all children of a shared class; ancestors' children in the pool).
        if let Ok((vectors, form)) = emb {
            let (mut css, mut pss, mut pda) = (Vec::new(), Vec::new(), Vec::new());
            for &p in &subset {
                let kids: Vec<usize> = h.children[p]
                    .iter()
                    .copied()
                    .filter(|c| shared.contains(&p) || pool.contains(c))
                    .collect();
                if kids.len() < 2 {
                    continue;
                }
                let Some(pv) = vectors.get(&h.text(p, *form)) else {
                    continue;
                };
                let kv: Option<Vec<&[f32]>> = kids
                    .iter()
                    .map(|&c| vectors.get(&h.text(c, *form)))
                    .collect();
                let Some(kv) = kv else { continue };
                let sc = score_family(pv, &kv);
                css.push(sc.css);
                pss.push(sc.pss);
                pda.push(sc.pda);
            }
            s.metric(format!("families in the subset ({o})"), css.len());
            s.metric(format!("mean CSS in the subset ({o})"), r3(mean(&css)));
            s.metric(format!("mean PSS in the subset ({o})"), r3(mean(&pss)));
            s.metric(format!("mean PDA in the subset ({o})"), r3(mean(&pda)));
        }
    }
    if !any {
        return Section::not_run(ID, TITLE, "No classes to compare with the domain concepts.");
    }
    s.notes.push(format!(
        "Domain concepts: {} ({} entries, {} after normalization).",
        crate::html::short_path(&domain.path),
        domain.entries,
        d
    ));
    s.notes.push(
        "A concept is shared when a class label equals a domain concept after lower-casing and reducing plural \
         nouns to the singular. The domain-relevant subset is the classes with a shared label, all their \
         ancestors, and their direct children (Zaitoun, Sagi and Hose, WWW '23 Companion)."
            .into(),
    );
    match emb {
        Ok((v, f)) => s.notes.push(vectors_note(v, *f)),
        Err(why) => s
            .notes
            .push(format!("Coherence of the subset not computed: {why}")),
    }
    s.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn irregular_table_is_sorted() {
        assert!(IRREGULAR.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn normalization_matches_the_2023_examples() {
        assert_eq!(normalize("Temperatures"), "temperature");
        assert_eq!(normalize("Solids"), "solid");
        assert_eq!(normalize("Sea  Waters"), "sea water");
        assert_eq!(normalize("sea-waters"), "sea-water");
        assert_eq!(normalize("processes"), "process");
        assert_eq!(normalize("species"), "species");
        assert_eq!(normalize("Bodies of water"), "body of water");
        assert_eq!(normalize("data"), "datum");
        assert_eq!(normalize("glass"), "glass");
        assert_eq!(normalize("nucleus"), "nucleus");
    }

    #[test]
    fn family_scores_follow_the_formulas() {
        // Parent along x; kids at known angles.
        let p = [1.0f32, 0.0];
        let a = [1.0f32, 0.0];
        let b = [0.0f32, 1.0];
        let c = [
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        ];
        let sc = score_family(&p, &[&a, &b, &c]);
        let r = std::f64::consts::FRAC_1_SQRT_2;
        // Pairs: a·b = 0, a·c = r, b·c = r.
        assert!((sc.css - 2.0 * r / 3.0).abs() < 1e-6);
        // Parent sims: 1, 0, r.
        assert!((sc.pss - (1.0 + r) / 3.0).abs() < 1e-6);
        let m = (1.0 + r) / 3.0;
        let sd = (((1.0 - m).powi(2) + m.powi(2) + (r - m).powi(2)) / 2.0).sqrt();
        assert!((sc.pda - (1.0 - sd)).abs() < 1e-6);
        assert_eq!(sc.diffs.len(), 3);
        // Without b, the remaining pair a, c has similarity r.
        assert!((sc.diffs[1].1 - r).abs() < 1e-6);
        // Two-child families have no leave-one-out test.
        assert!(score_family(&p, &[&a, &b]).diffs.is_empty());
    }
}
