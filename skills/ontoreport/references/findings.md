# Reading and acting on ontoreport findings

Each finding has a `check` (what was found), an `element` (the term, usually "label (local name or
CURIE)"), an `origin` (`statements`: an ontology file; `candidates`: a review-desk candidate), the
`source` it came from, a `status` (desk status, or open/decided for pitfalls) and a `detail`.

How to read the "false alarm" column: **rare** means treat it as a problem unless shown otherwise;
**sometimes** means check before acting; **often** means it is a prompt for review, not a verdict.

## Consistency and pitfalls (`logic`)

| Check | Meaning | Usual cause and fix | False alarm |
|---|---|---|---|
| unsatisfiable class | the reasoner (whelk, OWL 2 EL) infers the class equal to owl:Nothing | a class under two disjoint parents, or a restriction that contradicts a disjointness. Find the two axioms and remove the wrong one | rare (EL ignores axioms outside the profile, so it can miss some, not invent them) |
| file could not be read | an ontology file did not parse | syntax error, or punning the reader cannot handle; the detail gives the reader's message | rare |
| pitfall Pxx · name | an OOPS! pitfall (31 are checked) | see the pitfall's description in the detail | often for P04, P08, P11, P13, P41: many projects accept these by design. Record them in decisions.json |

Metrics to quote: "consistent under OWL 2 EL", "unsatisfiable classes", "imports not loaded" (axioms in
missing imports were not seen, so a clean result covers only the files read).

## Structure (`structure`)

| Check | Meaning | Fix | False alarm |
|---|---|---|---|
| no path to the top class | following parents never reaches BFO's entity (when the project uses BFO) or owl:Thing | add the missing parent, or import the ontology that holds it; the detail names where the chain breaks, and the chart "Where parent chains break" shows the parents that would repair most classes at once | sometimes: parents in an import whose release ontoreport cannot read (list it in releases.toml, or put the imported file in the folder) |

## Provenance (`provenance`)

| Check | Meaning | Fix | False alarm |
|---|---|---|---|
| no definition | a class or property with no textual definition (IAO:0000115, skos:definition, or an elucidation for BFO) | write one, with a source (IAO:0000119) | rare |
| definition cites a weak source | the definition or its source points at Wikipedia or a similar general site | cite a primary source | sometimes |

## Reuse fidelity (`reuse`)

The section notes list every release read, with its version. A term is only checked against a release
ontoreport could read; "Desk terms with no release to check against" shows what was skipped.

| Check | Meaning | Fix | False alarm |
|---|---|---|---|
| obsolete in the source release | the source has retired the term; the detail names the replacement when the source gives one | switch to the replacement | rare |
| not in the source release | no release read contains the identifier | check the identifier for a typing error, and that the term comes from the source recorded; add the right source to releases.toml | sometimes (a source whose release file leaves the term out, or a term from a source not configured) |
| under an obsolete class in the source release | in the source, the term's parent or a class above it is retired | nothing to fix in the project; note it, and check whether a newer source release moved the term | rare as a fact, often harmless |
| label differs from the release | the project names the term differently from its source | use the source's label, or record the deliberate change | sometimes (a qualifier in brackets is already ignored) |
| definition differs from the release | the project's definition matches none of the source's | restore the source text, or record why it was changed and cite the source of the new text | sometimes (an older or newer release's wording) |
| definition not in the release | the source gives no definition, yet the project has one | the text was written for the project: give it a definition source | rare |
| identifier with two names | the project uses one identifier under two different labels | pick one label | rare |

Whether a mapping declared exact is really exact (a unit per kilogram of solvent used for seawater) is not
checked: that needs a domain expert.

## Traceability and review status (`traceability`, `review`)

These need a review desk. "group with no question, use case or reason" means a kept group on the desk
records nothing about why it is there; ask the user what it serves. Review status has no findings, only
counts by source and status.

## Semantic coherence (`coherence`)

Concept families are a parent with two or more direct children. CSS is the mean similarity of siblings,
PSS of parent and children, PDA one minus the spread of the parent-child similarities, from sentence
embeddings (the notes name the model).

| Check | Meaning | What to do | False alarm |
|---|---|---|---|
| child may not belong under its parent | removing the child from its family raises the family's sibling similarity and agreement beyond the 2023 threshold | show it to the user as a candidate for review with the detail (similarity with and without it); never move a class on this alone | often: the rule agreed with domain experts 85–89% of the time, so in a large ontology many flags are fine |

Compare coherence means between versions of the same ontology under the same model, not with other
models or with the 2023 published values.

## Corpus fit (`corpus`)

Domain coverage (shared concepts / domain concepts), ontology relevance (shared / classes) and
sub-ontology relevance (shared / classes in the domain-relevant subset). The chart of frequent missing
concepts lists candidates to add; filter out general words ("data", "analysis") before suggesting any.
There are no findings: these are measures, to compare between versions or between candidate ontologies.

## decisions.json

```json
{
  "pitfalls": [{"id": "P11", "elements": ["*"], "decision": "accepted", "reason": "...", "ref": "..."}],
  "findings": [{"id": "label differs from the release", "elements": ["alternative term (IAO:0000118)"],
                "decision": "accepted", "reason": "the project keeps IAO's older label", "ref": "..."}]
}
```

`id` is the pitfall code or the check name; `elements` are as the report prints them, or `"*"`. Decided
pitfalls show their decision instead of "open".
