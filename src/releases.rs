//! Source releases: the files a reused term is checked against in the reuse-fidelity section.
//!
//! Which release answers for a term:
//! - a source listed in the project's `releases.toml`, by IRI namespace, or by the source name a review
//!   desk gives the term;
//! - otherwise, for an OBO identifier (`http://purl.obolibrary.org/obo/ENVO_...`), the OBO release of that
//!   ontology, but only when the project records which release it used: the desk's release text gives
//!   the date ("BFO 2019-08-26; RO 2026-09-04", or a date on a source named after the ontology). An
//!   identifier from an ontology the project names no release for is not fetched (NCBITaxon alone is
//!   gigabytes); it is checked against the release of the desk source it was taken with, whose full file
//!   carries the terms that source imports.
//!
//! The full release file is used, not the base file, for that reason. The dated file is tried first;
//! when it cannot be fetched the current release is used and its version is compared with the recorded
//! one. Every download is capped in size (`[releases] max_mb` in the configuration).
//!
//! Each file is downloaded once into `~/.ontoreport/releases` and read with a plain RDF pass: labels,
//! definitions, deprecation, replacements and named parents of every subject.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::model::Index;

const OBO: &str = "http://purl.obolibrary.org/obo/";
const ACCEPT: &str = "text/turtle, application/rdf+xml;q=0.9, application/n-triples;q=0.8";

// ── Project file ───────────────────────────────────────────────────────────

/// A source listed in `releases.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpec {
    /// The source's name as the review desk gives it ("Darwin Core"); terms of that source are checked
    /// against this release when no namespace claims them.
    #[serde(default)]
    pub name: Option<String>,
    /// IRI prefixes the release defines.
    #[serde(default)]
    pub namespaces: Vec<String>,
    /// Release files: URLs, or paths relative to `releases.toml`. All are read into one release.
    pub files: Vec<String>,
    /// The version the project recorded, compared with the version the file states.
    #[serde(default)]
    pub version: Option<String>,
    /// For a release published as a CSV table (Darwin Core's term_versions.csv): which columns hold what.
    #[serde(default)]
    pub csv: Option<CsvColumns>,
}

/// Columns of a CSV release. A term may have several rows (one per version): a row whose status is in
/// `current` describes the term as it stands; a term with no such row is obsolete.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CsvColumns {
    pub iri: String,
    pub label: String,
    #[serde(default)]
    pub definition: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub current: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpecFile {
    #[serde(default)]
    source: Vec<SourceSpec>,
}

/// Read `releases.toml`. Relative file paths are resolved against its folder.
pub fn load_specs(path: &Path) -> Result<Vec<SourceSpec>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let parsed: SpecFile = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    Ok(parsed
        .source
        .into_iter()
        .map(|mut s| {
            for f in &mut s.files {
                if !is_url(f) && Path::new(f).is_relative() {
                    *f = dir.join(&*f).display().to_string();
                }
            }
            s
        })
        .collect())
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

// ── Plans: which files to fetch ────────────────────────────────────────────

/// A release to read: the files to try, in order of preference (OBO), or all of them (project specs).
#[derive(Debug, Clone)]
pub struct Plan {
    pub name: String,
    /// Each entry is a list of alternatives; the first that can be read is used.
    pub files: Vec<Vec<String>>,
    pub namespaces: Vec<String>,
    pub source_name: Option<String>,
    pub expected: Option<String>,
    pub csv: Option<CsvColumns>,
}

/// OBO prefix of an IRI ("ENVO" for `.../obo/ENVO_00000001`).
fn obo_prefix(iri: &str) -> Option<&str> {
    let rest = iri.strip_prefix(OBO)?;
    let (prefix, id) = rest.split_once('_')?;
    (!prefix.is_empty()
        && prefix.chars().all(|c| c.is_ascii_alphanumeric())
        && !id.is_empty()
        && !id.contains('/'))
    .then_some(prefix)
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// Ontology name -> (release date, desk source), from the desk's release texts: "BFO 2019-08-26;
/// RO 2026-09-04" gives both pairs, and a bare date on a source named "ENVO" gives ENVO.
fn recorded_dates(index: &Index) -> BTreeMap<String, (String, String)> {
    let mut out = BTreeMap::new();
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    for t in &index.terms {
        let (Some(src), Some(rel)) = (t.source.as_deref(), t.release.as_deref()) else {
            continue;
        };
        if !seen.insert((src, rel)) {
            continue;
        }
        let words: Vec<&str> = rel
            .split(|c: char| c.is_whitespace() || c == ';' || c == ',' || c == '(' || c == ')')
            .filter(|w| !w.is_empty())
            .collect();
        for w in words.windows(2) {
            if is_date(w[1]) && w[0].chars().all(|c| c.is_ascii_alphanumeric()) {
                out.entry(w[0].to_uppercase())
                    .or_insert_with(|| (w[1].to_string(), src.to_string()));
            }
        }
        if is_date(rel.trim()) {
            out.entry(src.trim().to_uppercase())
                .or_insert_with(|| (rel.trim().to_string(), src.to_string()));
        }
    }
    out
}

/// The project's own OBO prefixes, from the IRIs of the ontology files it contains.
fn own_prefixes(index: &Index) -> BTreeSet<String> {
    index
        .ontologies
        .iter()
        .filter_map(|o| o.iri.as_deref())
        .filter_map(|iri| iri.strip_prefix(OBO))
        .map(|rest| {
            rest.trim_end_matches(".owl")
                .split(['/', '.'])
                .next()
                .unwrap_or("")
                .to_uppercase()
        })
        .filter(|p| !p.is_empty())
        .collect()
}

/// The releases the index's terms need.
pub fn plans(index: &Index) -> Vec<Plan> {
    let mut out: Vec<Plan> = index
        .release_specs
        .iter()
        .map(|s| Plan {
            name: match (&s.name, &s.version) {
                (Some(n), Some(v)) => format!("{n} {v}"),
                (Some(n), None) => n.clone(),
                (None, _) => s.files.first().cloned().unwrap_or_default(),
            },
            files: s.files.iter().map(|f| vec![f.clone()]).collect(),
            namespaces: s.namespaces.clone(),
            source_name: s.name.clone(),
            expected: s.version.clone(),
            csv: s.csv.clone(),
        })
        .collect();
    let claimed = |iri: &str| {
        index
            .release_specs
            .iter()
            .any(|s| s.namespaces.iter().any(|n| iri.starts_with(n.as_str())))
    };
    let own = own_prefixes(index);
    let dates = recorded_dates(index);
    let mut prefixes: BTreeSet<String> = BTreeSet::new();
    for t in &index.terms {
        let Some(iri) = t.iri.as_deref() else {
            continue;
        };
        let Some(p) = obo_prefix(iri) else { continue };
        if claimed(iri) || own.contains(&p.to_uppercase()) {
            continue;
        }
        // Only ontologies the project records a release for.
        if dates.contains_key(&p.to_uppercase()) {
            prefixes.insert(p.to_string());
        }
    }
    for p in prefixes {
        let id = p.to_lowercase();
        let (d, source) = dates[&p.to_uppercase()].clone();
        out.push(Plan {
            name: format!("{p} {d}"),
            files: vec![vec![
                format!("{OBO}{id}/releases/{d}/{id}.owl"),
                format!("{OBO}{id}/{d}/{id}.owl"),
                format!("{OBO}{id}.owl"),
            ]],
            namespaces: vec![format!("{OBO}{p}_")],
            source_name: Some(source),
            expected: Some(d),
            csv: None,
        });
    }
    out
}

// ── Reading a release ──────────────────────────────────────────────────────

/// What a release says about one term.
#[derive(Debug, Clone, Default)]
pub struct ReleaseTerm {
    pub labels: Vec<String>,
    pub definitions: Vec<String>,
    pub deprecated: bool,
    pub replaced_by: Vec<String>,
    /// Named parents: rdfs:subClassOf, rdfs:subPropertyOf, skos:broader.
    pub parents: Vec<String>,
}

impl ReleaseTerm {
    pub fn label(&self) -> Option<&str> {
        self.labels.first().map(|s| s.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Release {
    pub name: String,
    /// Files read, as fetched.
    pub urls: Vec<String>,
    /// The version the file states (owl:versionIRI or owl:versionInfo of the ontology or collection).
    pub version: Option<String>,
    pub expected: Option<String>,
    pub namespaces: Vec<String>,
    pub source_name: Option<String>,
    pub terms: HashMap<String, ReleaseTerm>,
}

impl Release {
    /// Whether the file is the version the project recorded (when it recorded one).
    pub fn version_matches(&self) -> bool {
        match (&self.expected, &self.version) {
            (None, _) => true,
            (Some(e), Some(v)) => v.contains(e.as_str()),
            (Some(_), None) => true,
        }
    }
}

/// Every release read for a report, and those that could not be.
#[derive(Debug, Default)]
pub struct Releases {
    pub releases: Vec<Release>,
    pub failed: Vec<(String, String)>,
}

impl Releases {
    /// The release that answers for a term, and what it says about it. In order: the release of the
    /// source the term was taken from, if it has the term (Darwin Core redefines the Dublin Core terms
    /// it uses); the release of the IRI's namespace, if it has it; any release that has it (BFO's
    /// relations are defined in RO). A term no release has is reported against its source's release, or
    /// its namespace's. `None` when no release answers for the term at all.
    pub fn find(
        &self,
        iri: &str,
        source: Option<&str>,
    ) -> Option<(&Release, Option<&ReleaseTerm>)> {
        let by_source: Vec<&Release> = self
            .releases
            .iter()
            .filter(|r| {
                source.is_some_and(|src| {
                    r.source_name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(src))
                })
            })
            .collect();
        let by_ns = self
            .releases
            .iter()
            .filter_map(|r| {
                r.namespaces
                    .iter()
                    .filter(|n| iri.starts_with(n.as_str()))
                    .map(|n| n.len())
                    .max()
                    .map(|len| (len, r))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, r)| r);
        // Releases in order of preference; the one that says most about the term wins (a definition over
        // a label over a bare declaration), the earlier one on a tie. A file that only declares a term
        // it imports (BFO declaring IAO's annotation properties) thus loses to the term's own release.
        let richness = |t: &ReleaseTerm| {
            usize::from(!t.definitions.is_empty()) * 2 + usize::from(!t.labels.is_empty())
        };
        let mut best: Option<(&Release, &ReleaseTerm)> = None;
        let ordered = by_source
            .iter()
            .copied()
            .chain(by_ns)
            .chain(self.releases.iter());
        for r in ordered {
            if let Some(t) = r.terms.get(iri) {
                if best.is_none_or(|(_, b)| richness(t) > richness(b)) {
                    best = Some((r, t));
                }
            }
        }
        if let Some((r, t)) = best {
            return Some((r, Some(t)));
        }
        by_ns.or(by_source.first().copied()).map(|r| (r, None))
    }
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const LABELS: &[&str] = &[
    "http://www.w3.org/2000/01/rdf-schema#label",
    "http://www.w3.org/2004/02/skos/core#prefLabel",
];
/// Definitions, most specific first; rdfs:comment counts only when there is nothing else, since W3C
/// and TDWG vocabularies write their definitions there.
const DEFINITIONS: &[&str] = &[
    "http://purl.obolibrary.org/obo/IAO_0000115",
    // BFO 2020 and earlier give its top classes elucidations, not definitions.
    "http://purl.obolibrary.org/obo/IAO_0000600",
    "http://www.w3.org/2004/02/skos/core#definition",
    "http://www.w3.org/ns/prov#definition",
    "http://purl.org/dc/terms/description",
];
const COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const DEPRECATED: &str = "http://www.w3.org/2002/07/owl#deprecated";
const REPLACED_BY: &[&str] = &[
    "http://purl.obolibrary.org/obo/IAO_0100001",
    "http://purl.org/dc/terms/isReplacedBy",
];
const PARENTS: &[&str] = &[
    "http://www.w3.org/2000/01/rdf-schema#subClassOf",
    "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
    "http://www.w3.org/2004/02/skos/core#broader",
];
const VERSION_IRI: &str = "http://www.w3.org/2002/07/owl#versionIRI";
const VERSION_INFO: &str = "http://www.w3.org/2002/07/owl#versionInfo";
const VERSION_HOLDERS: &[&str] = &[
    "http://www.w3.org/2002/07/owl#Ontology",
    "http://www.w3.org/2004/02/skos/core#Collection",
    "http://www.w3.org/2004/02/skos/core#ConceptScheme",
];

/// The RDF syntax of a downloaded file, judged from its first bytes; `None` for a web page.
fn sniff(bytes: &[u8]) -> Option<oxrdfio::RdfFormat> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]).to_lowercase();
    let start = head.trim_start_matches('\u{feff}').trim_start();
    if start.starts_with('<') {
        if head.contains("<html") || head.contains("<!doctype html") {
            return None;
        }
        if start.starts_with("<?xml")
            || start.starts_with("<rdf")
            || start.starts_with("<!doctype rdf")
        {
            return Some(oxrdfio::RdfFormat::RdfXml);
        }
        // N-Triples start with an IRI.
        return Some(oxrdfio::RdfFormat::NTriples);
    }
    if start.is_empty() {
        return None;
    }
    Some(oxrdfio::RdfFormat::Turtle)
}

#[derive(Default)]
struct Raw {
    labels: Vec<(bool, String)>,
    defs: Vec<(usize, String)>,
    comments: Vec<String>,
    deprecated: bool,
    replaced_by: Vec<String>,
    parents: Vec<String>,
    typed: bool,
    version_holder: bool,
    version_iri: Option<String>,
    version_info: Option<String>,
}

/// Read one release file into `terms`; returns the version it states.
fn read_into(bytes: &[u8], base: &str, terms: &mut HashMap<String, Raw>) -> Result<(), String> {
    let format = sniff(bytes).ok_or("not an RDF file (a web page or an empty file)")?;
    let mut parser = oxrdfio::RdfParser::from_format(format).lenient();
    if is_url(base) {
        if let Ok(p) = parser.clone().with_base_iri(base) {
            parser = p;
        }
    }
    let mut read = 0usize;
    let mut first_error = None;
    for q in parser.for_slice(bytes) {
        let q = match q {
            Ok(q) => q,
            Err(e) => {
                first_error.get_or_insert_with(|| e.to_string());
                continue;
            }
        };
        read += 1;
        let oxrdf::NamedOrBlankNode::NamedNode(s) = &q.subject else {
            continue;
        };
        let p = q.predicate.as_str();
        let named = match &q.object {
            oxrdf::Term::NamedNode(n) => Some(n.as_str()),
            _ => None,
        };
        let lit = match &q.object {
            oxrdf::Term::Literal(l) => Some(l),
            _ => None,
        };
        let r = terms.entry(s.as_str().to_string()).or_default();
        if p == RDF_TYPE {
            r.typed = true;
            if named.is_some_and(|o| VERSION_HOLDERS.contains(&o)) {
                r.version_holder = true;
            }
        } else if LABELS.contains(&p) {
            if let Some(l) = lit {
                let english = matches!(
                    l.language(),
                    None | Some("en") | Some("en-us") | Some("en-gb")
                );
                r.labels.push((english, l.value().trim().to_string()));
            }
        } else if let Some(rank) = DEFINITIONS.iter().position(|d| *d == p) {
            if let Some(l) = lit {
                if matches!(
                    l.language(),
                    None | Some("en") | Some("en-us") | Some("en-gb")
                ) {
                    r.defs.push((rank, l.value().trim().to_string()));
                }
            }
        } else if p == COMMENT {
            if let Some(l) = lit {
                if matches!(
                    l.language(),
                    None | Some("en") | Some("en-us") | Some("en-gb")
                ) {
                    r.comments.push(l.value().trim().to_string());
                }
            }
        } else if p == DEPRECATED {
            if let Some(l) = lit {
                r.deprecated |= matches!(l.value(), "true" | "1");
            }
        } else if REPLACED_BY.contains(&p) {
            match (&q.object, lit) {
                (oxrdf::Term::NamedNode(n), _) => r.replaced_by.push(n.as_str().to_string()),
                (_, Some(l)) => r.replaced_by.push(l.value().to_string()),
                _ => {}
            }
        } else if PARENTS.contains(&p) {
            if let Some(o) = named {
                r.parents.push(o.to_string());
            }
        } else if p == VERSION_IRI {
            if let Some(o) = named {
                r.version_iri = Some(o.to_string());
            }
        } else if p == VERSION_INFO {
            if let Some(l) = lit {
                r.version_info = Some(l.value().to_string());
            }
        }
    }
    if read == 0 {
        return Err(first_error.unwrap_or_else(|| "no statements".into()));
    }
    Ok(())
}

fn read_any(
    bytes: &[u8],
    base: &str,
    csv: Option<&CsvColumns>,
    terms: &mut HashMap<String, Raw>,
) -> Result<(), String> {
    match csv {
        Some(cols) => read_csv(bytes, cols, terms),
        None => read_into(bytes, base, terms),
    }
}

/// Rows of a CSV file (RFC 4180: quoted fields may hold commas, line breaks and doubled quotes).
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn read_csv(
    bytes: &[u8],
    cols: &CsvColumns,
    terms: &mut HashMap<String, Raw>,
) -> Result<(), String> {
    let text = String::from_utf8_lossy(bytes);
    let mut rows = csv_rows(text.trim_start_matches('\u{feff}')).into_iter();
    let header = rows.next().ok_or("empty CSV file")?;
    let col = |name: &str| header.iter().position(|h| h.trim() == name);
    let iri = col(&cols.iri).ok_or_else(|| format!("no column {}", cols.iri))?;
    let label = col(&cols.label).ok_or_else(|| format!("no column {}", cols.label))?;
    let def = cols.definition.as_deref().and_then(col);
    let status = cols.status.as_deref().and_then(col);
    // Per term: the last current row, else the last row.
    let mut best: HashMap<String, (bool, Vec<String>)> = HashMap::new();
    for r in rows {
        let Some(id) = r
            .get(iri)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let current = match status.and_then(|i| r.get(i)) {
            Some(s) if !cols.current.is_empty() => cols.current.iter().any(|c| c == s.trim()),
            _ => true,
        };
        let keep = best
            .get(&id)
            .is_none_or(|(was_current, _)| current || !was_current);
        if keep {
            best.insert(id, (current, r));
        }
    }
    if best.is_empty() {
        return Err("no rows".into());
    }
    for (id, (current, r)) in best {
        let t = terms.entry(id).or_default();
        t.typed = true;
        t.deprecated |= !current;
        if let Some(l) = r.get(label).filter(|l| !l.trim().is_empty()) {
            t.labels.push((true, l.trim().to_string()));
        }
        if let Some(d) = def.and_then(|i| r.get(i)).filter(|d| !d.trim().is_empty()) {
            t.defs.push((0, d.trim().to_string()));
        }
    }
    Ok(())
}

fn finish(name: String, plan: &Plan, urls: Vec<String>, raw: HashMap<String, Raw>) -> Release {
    let mut version = None;
    let mut terms = HashMap::with_capacity(raw.len());
    for (iri, mut r) in raw {
        if r.version_holder && version.is_none() {
            version = r.version_iri.clone().or(r.version_info.clone());
        }
        if r.labels.is_empty() && r.defs.is_empty() && !r.typed {
            continue;
        }
        // English or untagged labels first, then alphabetical, so the first label is stable.
        r.labels
            .sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let mut labels: Vec<String> = r.labels.into_iter().map(|(_, l)| l).collect();
        labels.dedup();
        r.defs.sort();
        let mut definitions: Vec<String> = r.defs.into_iter().map(|(_, d)| d).collect();
        definitions.extend(r.comments);
        definitions.dedup();
        let obsolete_label = labels
            .first()
            .is_some_and(|l| l.to_lowercase().starts_with("obsolete"));
        r.parents.sort();
        r.parents.dedup();
        terms.insert(
            iri,
            ReleaseTerm {
                labels,
                definitions,
                deprecated: r.deprecated || obsolete_label,
                replaced_by: r.replaced_by,
                parents: r.parents,
            },
        );
    }
    Release {
        name,
        urls,
        version,
        expected: plan.expected.clone(),
        namespaces: plan.namespaces.clone(),
        source_name: plan.source_name.clone(),
        terms,
    }
}

/// Read a local release file (used by tests and by `releases.toml` entries that name files).
pub fn read_local(plan: &Plan) -> Result<Release, String> {
    let mut raw = HashMap::new();
    let mut urls = Vec::new();
    for alts in &plan.files {
        let f = alts.first().ok_or("no file")?;
        let bytes = fs::read(f).map_err(|e| format!("{f}: {e}"))?;
        read_any(&bytes, f, plan.csv.as_ref(), &mut raw).map_err(|e| format!("{f}: {e}"))?;
        urls.push(f.clone());
    }
    Ok(finish(plan.name.clone(), plan, urls, raw))
}

/// Where a URL is cached.
fn cache_path(dir: &Path, url: &str) -> PathBuf {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let safe: String = rest
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(safe)
}

/// Fetch (unless cached or `offline`) and read every planned release, in parallel.
pub fn fetch_all(plans: &[Plan], cache_dir: &Path, offline: bool, max_mb: u64) -> Releases {
    let results: Vec<Result<Release, (String, String)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = plans
            .iter()
            .map(|plan| scope.spawn(move || fetch_one(plan, cache_dir, offline, max_mb)))
            .collect();
        handles
            .into_iter()
            .zip(plans)
            .map(|(h, p)| {
                h.join()
                    .unwrap_or_else(|_| Err((p.name.clone(), "the reader failed".into())))
            })
            .collect()
    });
    let mut out = Releases::default();
    for r in results {
        match r {
            Ok(r) => out.releases.push(r),
            Err(e) => out.failed.push(e),
        }
    }
    out.releases.sort_by(|a, b| a.name.cmp(&b.name));
    out.failed.sort();
    out
}

fn fetch_one(
    plan: &Plan,
    cache_dir: &Path,
    offline: bool,
    max_mb: u64,
) -> Result<Release, (String, String)> {
    let fail = |e: String| (plan.name.clone(), e);
    let mut raw = HashMap::new();
    let mut urls = Vec::new();
    for alts in &plan.files {
        let mut last_err = String::from("no file to try");
        let mut got = false;
        for url in alts {
            let bytes = if !is_url(url) {
                match fs::read(url) {
                    Ok(b) => b,
                    Err(e) => {
                        last_err = format!("{url}: {e}");
                        continue;
                    }
                }
            } else {
                let path = cache_path(cache_dir, url);
                let missing = path.with_extension("missing");
                if missing.exists() {
                    last_err = format!("{url}: not available (remembered from an earlier run)");
                    continue;
                }
                if !path.is_file() {
                    if offline {
                        last_err = format!("{url}: not cached, and --offline is set");
                        continue;
                    }
                    if let Err(e) = crate::net::fetch_capped(url, &path, Some(ACCEPT), max_mb) {
                        // Remember files that do not exist, so later runs do not ask again.
                        let _ = fs::write(&missing, &e);
                        last_err = e;
                        continue;
                    }
                }
                match fs::read(&path) {
                    Ok(b) => b,
                    Err(e) => {
                        last_err = format!("{}: {e}", path.display());
                        continue;
                    }
                }
            };
            let mut one = HashMap::new();
            match read_any(&bytes, url, plan.csv.as_ref(), &mut one) {
                Ok(()) => {
                    for (k, v) in one {
                        merge(raw.entry(k).or_default(), v);
                    }
                    urls.push(url.clone());
                    got = true;
                    break;
                }
                Err(e) => {
                    // A web page where a release was expected: drop it and remember the URL.
                    if is_url(url) {
                        let path = cache_path(cache_dir, url);
                        let _ = fs::remove_file(&path);
                        let _ = fs::write(path.with_extension("missing"), &e);
                    }
                    last_err = format!("{url}: {e}");
                }
            }
        }
        if !got {
            return Err(fail(last_err));
        }
    }
    Ok(finish(plan.name.clone(), plan, urls, raw))
}

fn merge(into: &mut Raw, from: Raw) {
    into.labels.extend(from.labels);
    into.defs.extend(from.defs);
    into.comments.extend(from.comments);
    into.deprecated |= from.deprecated;
    into.replaced_by.extend(from.replaced_by);
    into.parents.extend(from.parents);
    into.typed |= from.typed;
    into.version_holder |= from.version_holder;
    into.version_iri = into.version_iri.take().or(from.version_iri);
    into.version_info = into.version_info.take().or(from.version_info);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obo_prefixes() {
        assert_eq!(
            obo_prefix("http://purl.obolibrary.org/obo/ENVO_00000015"),
            Some("ENVO")
        );
        assert_eq!(obo_prefix("http://purl.obolibrary.org/obo/envo.owl"), None);
        assert_eq!(obo_prefix("http://example.org/x_1"), None);
    }

    #[test]
    fn web_pages_are_not_releases() {
        assert_eq!(sniff(b"<!DOCTYPE html><html>"), None);
        assert_eq!(
            sniff(b"<?xml version=\"1.0\"?><rdf:RDF"),
            Some(oxrdfio::RdfFormat::RdfXml)
        );
        assert_eq!(sniff(b"@prefix : <x#> ."), Some(oxrdfio::RdfFormat::Turtle));
    }

    #[test]
    fn csv_fields_may_hold_commas_quotes_and_line_breaks() {
        let rows = csv_rows("a,b\n\"x, \"\"y\"\"\",\"two\nlines\"\nlast,row");
        assert_eq!(rows[1], vec!["x, \"y\"", "two\nlines"]);
        assert_eq!(rows[2], vec!["last", "row"]);
    }

    #[test]
    fn dates() {
        assert!(is_date("2026-10-23"));
        assert!(!is_date("2026-10-2x"));
    }
}
