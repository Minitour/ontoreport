//! Plain-language explanations of every metric, KPI and chart: what it is and how it is measured.
//! Shown behind the (?) icons in the HTML report and included in the JSON next to each metric.

/// The explanation for a metric, KPI or chart title. The origin in parentheses ("(ontology file)"),
/// a "(top N)" suffix and the source in "terms by status: <source>" are ignored when looking up.
pub fn about(name: &str) -> Option<&'static str> {
    let mut key = name.trim().to_lowercase();
    if let Some(i) = key.find(" (") {
        key.truncate(i);
    }
    if key.starts_with("terms by status:") {
        key = "terms by status".into();
    }
    let text = match key.as_str() {
        // ── overview ────────────────────────────────────────────────────
        "terms" => "How many terms the report looked at: those declared in the ontology files it read, plus the \
                    candidate terms listed on the review desk.",
        "sources reused" => "How many different ontologies and vocabularies the review desk takes terms from \
                             (for example IAO, OBI, ENVO).",
        "findings" => "The total number of problems the report found, across all sections. Each finding names \
                       the term or group it concerns.",
        "where the findings are: section › check › source" => {
            "The findings broken down by section, then by the check that raised them, then by the source \
             ontology. Click a row to open the level below."
        }

        // ── structure ───────────────────────────────────────────────────
        "classes reaching the top class" => {
            "The share of classes whose chain of parents leads up to the top class (BFO's 'entity' or \
             owl:Thing). Measured by following each class's parents, by label, through every ontology file and \
             review-desk entry the project has. Higher is better."
        }
        "top class" => {
            "The class every other class should lead up to. When the project uses BFO it is BFO's 'entity', and              a class whose parents stop anywhere else counts as broken. Otherwise it is owl:Thing, and a class              with no recorded parent is simply a root under it."
        }
        "classes checked" => {
            "How many classes the tree check looked at: every class in the ontology files, and the desk \
             candidates that are not marked Out."
        }
        "classes without a path to the top class" => {
            "Classes whose parents never lead to the top class: every route upward ends at a parent the \
             project does not record. Such a class floats outside the hierarchy."
        }
        "tree errors recorded by the desk" => {
            "Problems of the same kind that the review desk had already listed itself. The desk computed them \
             against the full source releases, so the two lists are compared rather than expected to match."
        }
        "classes without a path, by source" => "Where the classes without a path to the top class come from.",
        "where parent chains break" => {
            "The parents at which chains most often stop: labels that are used as a parent but have no recorded \
             parent themselves. Fixing one of these usually repairs many classes at once."
        }

        // ── consistency and pitfalls ────────────────────────────────────
        "ontology files that could not be read" => {
            "Ontology files in the project folder that the reader could not parse. They are listed as findings              with the reason, and everything else is reported without them."
        }
        "ontology files read" => "How many ontology files (OWL, functional syntax, Turtle or N-Triples) were read.",
        "axioms and annotations" => {
            "How many statements the files contain: class and property declarations, logical axioms (such as \
             'A is a kind of B') and annotations (labels, definitions)."
        }
        "consistent under owl 2 el" => {
            "Whether the ontology can be true at all. A reasoner (whelk, OWL 2 EL) checks whether the axioms \
             force the top class, or any named class, to have no members. 'no' means something contradicts."
        }
        "unsatisfiable classes" => {
            "Classes that can never have any members because their axioms contradict each other (the reasoner \
             infers them equal to owl:Nothing). Usually a modelling error such as a class placed under two \
             disjoint parents."
        }
        "satisfiable classes" => {
            "The share of classes that can have members, i.e. are not contradicted by the axioms. Measured with \
             the whelk reasoner over all files read. Higher is better."
        }
        "imports not loaded" => {
            "Ontologies the files import but that were not found in the project folder. Axioms in them are not \
             seen, so consistency and the tree check cover only the files read."
        }
        "pitfall instances, open" => {
            "Occurrences of common modelling mistakes (the OOPS! pitfall catalogue, as checked by OWL-MCP) that \
             the project has not recorded a decision about."
        }
        "pitfall instances, decided" => {
            "Pitfall occurrences the project has recorded a decision about in decisions.json, for example \
             'accepted by design' with a reason."
        }
        "pitfall instances by importance" => {
            "Pitfall occurrences grouped by how serious the OOPS! catalogue rates them (critical, important, \
             minor), split into those still open and those with a recorded decision."
        }
        "pitfall instances by pitfall" => "Which pitfalls occur most often, counting each affected term once per pitfall.",

        // ── provenance ──────────────────────────────────────────────────
        "terms with a definition" => {
            "The share of terms that have a definition: a textual definition (IAO:0000115 or skos:definition) \
             or, for BFO, an elucidation. Individuals are not counted. Higher is better."
        }
        "terms checked" => "How many terms were checked for definitions: classes and properties, not individuals.",
        "without a definition" => "Terms that have no definition text at all.",
        "definitions with their own source annotation" => {
            "Definitions that carry a 'definition source' annotation (IAO:0000119) saying where the text comes \
             from. Counted for information: a reused term is already sourced by the release it was taken from."
        }
        "citing a weak source" => {
            "Definitions whose text or source mentions Wikipedia, which is not a citable scholarly source."
        }
        "terms without a definition, by source" => "Where the terms without a definition come from.",
        "definitions citing a weak source, by source" => "Where the definitions that cite a weak source come from.",

        // ── traceability ────────────────────────────────────────────────
        "groups naming a requirement" => {
            "The share of kept groups on the review desk that say which competency question or use case they \
             serve, by naming an identifier such as A1 or B12. Higher is better."
        }
        "groups checked" => "Groups of terms on the review desk that are kept (In) or still open (Debatable).",
        "groups naming a competency question or use case" => {
            "Groups whose 'serves' note names at least one requirement identifier (such as A1, B12 or X3)."
        }
        "groups with a reason but no question" => {
            "Groups that explain in words why they are included but name no requirement identifier."
        }
        "groups with neither" => "Groups with no requirement and no reason: nothing records why they are there.",
        "distinct requirements named" => "How many different requirement identifiers the groups name in total.",
        "how each group is justified, by source" => {
            "For each source, how many of its kept groups name a requirement, give only a reason, or give neither."
        }
        "groups serving each requirement" => "How many kept groups name each requirement identifier.",

        // ── review status ───────────────────────────────────────────────
        "terms by status" => "How many of this source's candidate terms the review desk lists as In, Debatable or Out.",
        "candidate terms kept" => {
            "The share of candidate terms on the review desk marked In. Neither good nor bad in itself: it shows \
             how much of each source the project chose to reuse."
        }
        "terms by status, by source" => "For each source, how many candidate terms are In, Debatable or Out.",
        "source › category › group" => {
            "The review desk's candidates broken down by source, then the desk's category, then the group, \
             with the In / Debatable / Out counts at each level. Click a row to open the level below."
        }
        _ => return None,
    };
    Some(text)
}
