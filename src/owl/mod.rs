//! OWL files: reading (RDF/XML, OWL functional syntax, Turtle and N-Triples), indexing the terms a file
//! declares, consistency with whelk (OWL 2 EL) and the OWL-MCP pitfall scan.
//!
//! RDF is read in lax mode: entities used before (or without) being declared are accepted, as in most
//! published OBO files, instead of failing the whole file.

pub mod pitfalls;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use horned_owl::io::ofn::reader::read_with_build as ofn_read;
use horned_owl::io::rdf::reader::{read_with_build as rdf_read, ConcreteRDFOntology};
use horned_owl::io::{ParserConfiguration, RDFParserConfiguration};
use horned_owl::model::*;
use horned_owl::ontology::set::SetOntology;

use crate::model::{Origin, Term};

pub const EXTENSIONS: &[&str] = &["owl", "rdf", "rdfxml", "xml", "ofn", "ttl", "nt"];

const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
const DEFINITIONS: &[&str] = &[
    "http://purl.obolibrary.org/obo/IAO_0000115", // definition
    "http://purl.obolibrary.org/obo/IAO_0000600", // elucidation (BFO)
    "http://www.w3.org/2004/02/skos/core#definition",
];
const DEFINITION_SOURCES: &[&str] = &[
    "http://purl.obolibrary.org/obo/IAO_0000119", // definition source
];

/// One ontology file as read.
pub struct LoadedOntology {
    pub path: PathBuf,
    pub iri: Option<String>,
    pub imports: Vec<String>,
    pub ontology: SetOntology<ArcStr>,
    /// How many classes had to be declared to read the file (used in class axioms, never declared).
    pub declared_on_read: usize,
    /// How many equivalences to enumerated classes (owl:oneOf) were skipped because the reader cannot
    /// build them from RDF.
    pub nominals_skipped: usize,
    /// Labels, definitions and definition sources read straight from the RDF triples (subject,
    /// property, value). Set for RDF files: these are read whether or not the file declares the
    /// annotation properties, which the OWL reader requires.
    pub annotations: Option<Vec<(String, String, String)>>,
}

impl std::fmt::Debug for LoadedOntology {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedOntology")
            .field("path", &self.path)
            .field("iri", &self.iri)
            .field("components", &self.ontology.iter().count())
            .finish()
    }
}

pub fn is_ontology_file(p: &Path) -> bool {
    p.is_file()
        && p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Read one ontology file.
pub fn load(path: &Path) -> Result<LoadedOntology, String> {
    let err = |e: String| format!("{}: {e}", path.display());
    let bytes = fs::read(path).map_err(|e| err(e.to_string()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let build = Build::new_arc();
    let mut declared_on_read = 0;
    let mut nominals_skipped = 0;
    let mut annotations = None;
    let mut rdf = |format: oxrdfio::RdfFormat| -> Result<SetOntology<ArcStr>, String> {
        // A plain RDF pass first: it reports syntax errors cleanly (the OWL reader panics on them) and
        // collects the annotations.
        annotations = Some(scan_annotations(&bytes, format)?);
        match read_rdf(&bytes, &build, format) {
            Err(e) if e.contains("Unknown entity") => {
                let (fixed, n, k) = repair_for_reader(&bytes, format)?;
                declared_on_read = n;
                nominals_skipped = k;
                read_rdf(&fixed, &build, oxrdfio::RdfFormat::NTriples)
            }
            r => r,
        }
    };
    let ontology = match ext.as_str() {
        "ofn" => read_ofn(&bytes, &build).map_err(err)?,
        "ttl" => rdf(oxrdfio::RdfFormat::Turtle).map_err(err)?,
        "nt" => rdf(oxrdfio::RdfFormat::NTriples).map_err(err)?,
        // .owl files are RDF/XML in most projects, but functional syntax also uses the extension.
        _ => match rdf(oxrdfio::RdfFormat::RdfXml) {
            Ok(o) => o,
            Err(rdf_err) => read_ofn(&bytes, &build).map_err(|_| err(rdf_err))?,
        },
    };
    let mut iri = None;
    let mut imports = Vec::new();
    for ac in ontology.iter() {
        match &ac.component {
            Component::OntologyID(oid) => {
                if let Some(i) = &oid.iri {
                    iri = Some(i.to_string());
                }
            }
            Component::Import(imp) => imports.push(imp.0.to_string()),
            _ => {}
        }
    }
    Ok(LoadedOntology {
        path: path.to_path_buf(),
        iri,
        imports,
        ontology,
        declared_on_read,
        nominals_skipped,
        annotations,
    })
}

/// The label, definition and definition-source triples of an RDF file.
fn scan_annotations(
    bytes: &[u8],
    format: oxrdfio::RdfFormat,
) -> Result<Vec<(String, String, String)>, String> {
    let mut out = Vec::new();
    for q in oxrdfio::RdfParser::from_format(format).for_reader(Cursor::new(bytes)) {
        let q = q.map_err(|e| format!("{format}: {e}"))?;
        let p = q.predicate.as_str();
        if p != RDFS_LABEL && !DEFINITIONS.contains(&p) && !DEFINITION_SOURCES.contains(&p) {
            continue;
        }
        let oxrdf::NamedOrBlankNode::NamedNode(subject) = &q.subject else {
            continue;
        };
        let value = match &q.object {
            oxrdf::Term::Literal(l) => l.value().to_string(),
            oxrdf::Term::NamedNode(n) => n.as_str().to_string(),
            _ => continue,
        };
        out.push((subject.as_str().to_string(), p.to_string(), value));
    }
    Ok(out)
}

/// Make a file the RDF reader rejects readable, changing as little as possible:
/// - an IRI used in a class axiom (subclass, equivalence, disjointness) but never declared a class, often
///   because it is declared as an individual (punning), is declared a class;
/// - an equivalence to an enumerated class (owl:oneOf, as in UO's units) is dropped with its blank nodes,
///   since the reader cannot build it.
///
/// Returns N-Triples, the number of declarations added and the number of equivalences dropped.
fn repair_for_reader(
    bytes: &[u8],
    format: oxrdfio::RdfFormat,
) -> Result<(Vec<u8>, usize, usize), String> {
    use oxrdfio::{RdfParser, RdfSerializer};
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
    const CLASS_AXIOMS: &[&str] = &[
        "http://www.w3.org/2000/01/rdf-schema#subClassOf",
        "http://www.w3.org/2002/07/owl#equivalentClass",
        "http://www.w3.org/2002/07/owl#disjointWith",
    ];
    let mut triples = Vec::new();
    for q in RdfParser::from_format(format).for_reader(Cursor::new(bytes)) {
        triples.push(q.map_err(|e| format!("{format}: {e}"))?);
    }
    let mut typed: BTreeSet<String> = BTreeSet::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    for q in &triples {
        let s = match &q.subject {
            oxrdf::NamedOrBlankNode::NamedNode(n) => Some(n.as_str().to_string()),
            _ => None,
        };
        let p = q.predicate.as_str();
        if p == RDF_TYPE {
            // Only a class type counts: OWL 2 punning lets an IRI be an individual and a class at once.
            let is_class =
                matches!(&q.object, oxrdf::Term::NamedNode(o) if o.as_str() == OWL_CLASS);
            if let (Some(s), true) = (s, is_class) {
                typed.insert(s);
            }
        } else if CLASS_AXIOMS.contains(&p) {
            used.extend(s);
            if let oxrdf::Term::NamedNode(o) = &q.object {
                used.insert(o.as_str().to_string());
            }
        }
    }
    // Classes also appear as members of intersections, unions and complements, and as the filler of
    // existential and universal restrictions (datatypes excluded).
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST_: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const LIST_OPERATORS: &[&str] = &[
        "http://www.w3.org/2002/07/owl#intersectionOf",
        "http://www.w3.org/2002/07/owl#unionOf",
    ];
    const FILLERS: &[&str] = &[
        "http://www.w3.org/2002/07/owl#someValuesFrom",
        "http://www.w3.org/2002/07/owl#allValuesFrom",
        "http://www.w3.org/2002/07/owl#complementOf",
    ];
    let is_datatype = |i: &str| {
        i.starts_with("http://www.w3.org/2001/XMLSchema#")
            || i.starts_with("http://www.w3.org/2000/01/rdf-schema#")
            || i.starts_with("http://www.w3.org/1999/02/22-rdf-syntax-ns#")
    };
    let mut first: HashMap<String, String> = HashMap::new();
    let mut rest: HashMap<String, String> = HashMap::new();
    let mut heads: Vec<String> = Vec::new();
    for q in &triples {
        let p = q.predicate.as_str();
        let subj = match &q.subject {
            oxrdf::NamedOrBlankNode::BlankNode(b) => Some(b.as_str().to_string()),
            _ => None,
        };
        match (&q.object, p) {
            (oxrdf::Term::NamedNode(o), _) if FILLERS.contains(&p) && !is_datatype(o.as_str()) => {
                used.insert(o.as_str().to_string());
            }
            (oxrdf::Term::BlankNode(o), _) if LIST_OPERATORS.contains(&p) => {
                heads.push(o.as_str().to_string())
            }
            (oxrdf::Term::NamedNode(o), RDF_FIRST) => {
                if let Some(sb) = subj {
                    first.insert(sb, o.as_str().to_string());
                }
            }
            (oxrdf::Term::BlankNode(o), RDF_REST_) => {
                if let Some(sb) = subj {
                    rest.insert(sb, o.as_str().to_string());
                }
            }
            _ => {}
        }
    }
    for h in heads {
        let mut node = Some(h);
        let mut steps = 0;
        while let Some(n) = node {
            if let Some(m) = first.get(&n) {
                if !is_datatype(m) {
                    used.insert(m.clone());
                }
            }
            node = rest.get(&n).cloned();
            steps += 1;
            if steps > 10_000 {
                break;
            }
        }
    }
    let missing: Vec<String> = used.difference(&typed).cloned().collect();

    // Blank nodes of enumerated classes and the RDF lists under them.
    const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
    const OWL_EQUIVALENT: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    let bnode = |t: &oxrdf::Term| match t {
        oxrdf::Term::BlankNode(b) => Some(b.as_str().to_string()),
        _ => None,
    };
    let mut drop_nodes: BTreeSet<String> = BTreeSet::new();
    let mut frontier: Vec<String> = Vec::new();
    for q in &triples {
        if let (oxrdf::NamedOrBlankNode::BlankNode(b), OWL_ONE_OF) =
            (&q.subject, q.predicate.as_str())
        {
            if drop_nodes.insert(b.as_str().to_string()) {
                frontier.extend(bnode(&q.object));
            }
        }
    }
    while let Some(n) = frontier.pop() {
        if drop_nodes.insert(n.clone()) {
            for q in &triples {
                if matches!(&q.subject, oxrdf::NamedOrBlankNode::BlankNode(b) if b.as_str() == n)
                    && q.predicate.as_str() == RDF_REST
                {
                    frontier.extend(bnode(&q.object));
                }
            }
        }
    }
    let mut dropped_equivalences = 0;
    let mut ser = RdfSerializer::from_format(oxrdfio::RdfFormat::NTriples).for_writer(Vec::new());
    for q in &triples {
        let subject_dropped = matches!(&q.subject, oxrdf::NamedOrBlankNode::BlankNode(b) if drop_nodes.contains(b.as_str()));
        let to_nominal = q.predicate.as_str() == OWL_EQUIVALENT
            && bnode(&q.object).is_some_and(|b| drop_nodes.contains(&b));
        if to_nominal {
            dropped_equivalences += 1;
        }
        if subject_dropped || to_nominal {
            continue;
        }
        ser.serialize_triple(q.as_ref())
            .map_err(|e| format!("re-serializing: {e}"))?;
    }
    for iri in &missing {
        let t = oxrdf::Triple::new(
            oxrdf::NamedNode::new_unchecked(iri),
            oxrdf::NamedNode::new_unchecked(RDF_TYPE),
            oxrdf::NamedNode::new_unchecked(OWL_CLASS),
        );
        ser.serialize_triple(&t)
            .map_err(|e| format!("re-serializing: {e}"))?;
    }
    let out = ser.finish().map_err(|e| format!("re-serializing: {e}"))?;
    Ok((out, missing.len(), dropped_equivalences))
}

fn read_ofn(bytes: &[u8], build: &Build<ArcStr>) -> Result<SetOntology<ArcStr>, String> {
    let (onto, _pm): (SetOntology<ArcStr>, _) = ofn_read(BufReader::new(Cursor::new(bytes)), build)
        .map_err(|e| format!("functional syntax: {e}"))?;
    Ok(onto)
}

fn read_rdf(
    bytes: &[u8],
    build: &Build<ArcStr>,
    format: oxrdfio::RdfFormat,
) -> Result<SetOntology<ArcStr>, String> {
    let mut reader = BufReader::new(Cursor::new(bytes));
    let config = ParserConfiguration {
        rdf: RDFParserConfiguration {
            lax: true,
            format: Some(format),
        },
        ..Default::default()
    };
    let (rdf, _incomplete): (
        ConcreteRDFOntology<ArcStr, Arc<AnnotatedComponent<ArcStr>>>,
        _,
    ) = rdf_read(&mut reader, build, config).map_err(|e| format!("{format}: {e}"))?;
    Ok(rdf.into())
}

// ── terms ──────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Facts {
    kind: Option<&'static str>,
    labels: Vec<String>,
    definitions: Vec<String>,
    definition_sources: Vec<String>,
    parents: BTreeSet<String>,
}

/// The terms the ontologies declare, as index records. Parents are named by label when any loaded file
/// labels them, otherwise by IRI, so the tree can follow them across files and desk data.
pub fn terms(ontologies: &[LoadedOntology]) -> Vec<Term> {
    let mut facts: HashMap<String, Facts> = HashMap::new();
    let note = |facts: &mut HashMap<String, Facts>, subject: &str, prop: &str, text: String| {
        let f = facts.entry(subject.to_string()).or_default();
        if prop == RDFS_LABEL {
            f.labels.push(text);
        } else if DEFINITIONS.contains(&prop) {
            f.definitions.push(text);
        } else if DEFINITION_SOURCES.contains(&prop) {
            f.definition_sources.push(text);
        }
    };
    for lo in ontologies {
        if let Some(anns) = &lo.annotations {
            for (sub, prop, val) in anns {
                note(&mut facts, sub, prop, val.clone());
            }
        }
        for ac in lo.ontology.iter() {
            match &ac.component {
                Component::DeclareClass(d) => set_kind(&mut facts, &d.0 .0, "class"),
                Component::DeclareObjectProperty(d) => {
                    set_kind(&mut facts, &d.0 .0, "object property")
                }
                Component::DeclareDataProperty(d) => set_kind(&mut facts, &d.0 .0, "data property"),
                Component::DeclareAnnotationProperty(d) => {
                    set_kind(&mut facts, &d.0 .0, "annotation property")
                }
                Component::DeclareNamedIndividual(d) => set_kind(&mut facts, &d.0 .0, "individual"),
                Component::SubClassOf(ax) => {
                    if let (ClassExpression::Class(sub), ClassExpression::Class(sup)) =
                        (&ax.sub, &ax.sup)
                    {
                        facts
                            .entry(sub.0.to_string())
                            .or_default()
                            .parents
                            .insert(sup.0.to_string());
                    }
                }
                Component::SubObjectPropertyOf(ax) => {
                    if let (
                        SubObjectPropertyExpression::ObjectPropertyExpression(
                            ObjectPropertyExpression::ObjectProperty(sub),
                        ),
                        ObjectPropertyExpression::ObjectProperty(sup),
                    ) = (&ax.sub, &ax.sup)
                    {
                        facts
                            .entry(sub.0.to_string())
                            .or_default()
                            .parents
                            .insert(sup.0.to_string());
                    }
                }
                // RDF files already gave their annotations from the plain RDF pass.
                Component::AnnotationAssertion(aa) if lo.annotations.is_none() => {
                    let AnnotationSubject::IRI(subject) = &aa.subject else {
                        continue;
                    };
                    let Some(text) = annotation_text(&aa.ann.av) else {
                        continue;
                    };
                    note(&mut facts, subject.as_ref(), aa.ann.ap.0.as_ref(), text);
                }
                _ => {}
            }
        }
    }
    // A label for every IRI that has one; English or untagged labels first.
    let label_of: HashMap<String, String> = facts
        .iter()
        .filter_map(|(iri, f)| best_label(&f.labels).map(|l| (iri.clone(), l)))
        .collect();
    let key_of = |iri: &str| -> String {
        if iri == OWL_THING {
            return "owl:thing".into();
        }
        label_of
            .get(iri)
            .cloned()
            .unwrap_or_else(|| iri.to_string())
            .to_lowercase()
    };
    let mut out: Vec<Term> = facts
        .iter()
        .filter(|(iri, f)| {
            f.kind.is_some() && iri.as_str() != OWL_THING && iri.as_str() != OWL_NOTHING
        })
        .map(|(iri, f)| {
            let label = label_of
                .get(iri)
                .cloned()
                .unwrap_or_else(|| local_name(iri));
            Term {
                key: key_of(iri),
                label,
                curie: Some(curie(iri)),
                iri: Some(iri.clone()),
                kind: f.kind.unwrap_or("").to_string(),
                definitions: f.definitions.clone(),
                definition_sources: f.definition_sources.clone(),
                parents: f.parents.iter().map(|p| key_of(p)).collect(),
                origin: Origin::Statements,
                source: None,
                release: None,
                group: None,
            }
        })
        .collect();
    out.sort_by(|a, b| a.iri.cmp(&b.iri));
    out
}

fn set_kind(facts: &mut HashMap<String, Facts>, iri: &IRI<ArcStr>, kind: &'static str) {
    let f = facts.entry(iri.to_string()).or_default();
    if f.kind.is_none() {
        f.kind = Some(kind);
    }
}

fn annotation_text(av: &AnnotationValue<ArcStr>) -> Option<String> {
    match av {
        AnnotationValue::Literal(Literal::Simple { literal })
        | AnnotationValue::Literal(Literal::Language { literal, .. })
        | AnnotationValue::Literal(Literal::Datatype { literal, .. }) => Some(literal.clone()),
        AnnotationValue::IRI(i) => Some(i.to_string()),
        _ => None,
    }
}

fn best_label(labels: &[String]) -> Option<String> {
    labels.iter().min().cloned()
}

fn local_name(iri: &str) -> String {
    iri.rsplit(['#', '/']).next().unwrap_or(iri).to_string()
}

/// OBO-style CURIE where the IRI follows the OBO pattern, otherwise the local name.
fn curie(iri: &str) -> String {
    if let Some(rest) = iri.strip_prefix("http://purl.obolibrary.org/obo/") {
        if let Some((prefix, id)) = rest.split_once('_') {
            return format!("{prefix}:{id}");
        }
    }
    local_name(iri)
}

// ── consistency ────────────────────────────────────────────────────────────

pub struct Consistency {
    pub consistent: bool,
    pub unsatisfiable: Vec<String>,
}

/// OWL 2 EL consistency of all loaded files together, with whelk (as OWL-MCP's check_consistency):
/// inconsistent when owl:Thing or any named class is unsatisfiable.
pub fn consistency(ontologies: &[LoadedOntology]) -> Consistency {
    use whelk::whelk::model::ConceptData;
    use whelk::whelk::owl::translate_ontology;
    use whelk::whelk::reasoner::assert as whelk_assert;

    let mut merged = SetOntology::new();
    for lo in ontologies {
        for ac in lo.ontology.iter() {
            merged.insert(ac.clone());
        }
    }
    let state = whelk_assert(&translate_ontology(&merged));
    let bottom = state.interner.bottom();
    let mut unsatisfiable: Vec<String> = state
        .closure_subs_by_superclass
        .get(&bottom)
        .map(|subs| {
            subs.iter()
                .filter_map(|&id| match state.interner.concept_data(id) {
                    ConceptData::AtomicConcept(iri)
                        if iri.as_str() != OWL_NOTHING && iri.as_str() != OWL_THING =>
                    {
                        Some(iri.to_string())
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    unsatisfiable.sort();
    unsatisfiable.dedup();
    let thing_unsat = state.is_subclass_of(state.interner.top(), bottom);
    Consistency {
        consistent: !thing_unsat && unsatisfiable.is_empty(),
        unsatisfiable,
    }
}

/// The pitfall scan over all loaded files together.
pub fn scan_pitfalls(ontologies: &[LoadedOntology]) -> pitfalls::PitfallReport {
    let mut merged = SetOntology::new();
    for lo in ontologies {
        for ac in lo.ontology.iter() {
            merged.insert(ac.clone());
        }
    }
    pitfalls::scan(&merged, None)
}
