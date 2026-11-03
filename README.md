# ontoreport

[![CI](https://github.com/Minitour/ontoreport/actions/workflows/ci.yml/badge.svg)](https://github.com/Minitour/ontoreport/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

A quality report for OWL ontologies that reads the **project record** as well as the ontology:
whether every term traces to a requirement, every definition to a cited source, every reused term
to its source release, and how much of the ontology has been reviewed. One native binary per
platform, written in Rust, distributed on npm and PyPI.

> **Status: early development.** The published 0.0.1 is a placeholder; build from source for the
> current version. Requirements coverage from a requirements file and review decisions from a desk
> export come next.

## Usage

```bash
ontoreport <project-folder | ontology-file> --out report.html [--concepts concepts.json]
```

Writes `report.html` (a dashboard for people) and `report.json` (the same content for agents). The
JSON is deterministic: the same inputs give the same bytes, so reports can be diffed and kept in
version control. Run time goes to stderr; set `ONTOREPORT_TIMING=1` for the time of each check.

Every metric, KPI and chart has a (?) icon that explains in plain words what it is and how it is
measured; the JSON carries the same text in each metric's `help` field.

## Inputs

All inputs are optional; a section whose input is missing is marked "not run".

| Input | Where it is looked for |
|---|---|
| Ontology files: RDF/XML (`.owl`, `.rdf`), OWL functional syntax (`.ofn`), Turtle (`.ttl`), N-Triples (`.nt`) | the folder, `ontology/`, `ontologies/`, `src/ontology/`, or a single file |
| Review-desk data: `g0.json` (statements of an ontology file), `groups.json` (candidate terms), `check.json` (the desk's self-check) | the folder or `data/` |
| Recorded decisions: `decisions.json` | the folder or `qc/` |
| Domain concepts: `concepts.json` or `concepts.txt` (or `--concepts FILE`) | the folder or `corpus/` |
| Source releases that are not OBO ontologies: `releases.toml` | the folder |

A file that cannot be read is reported as a finding and the rest of the report goes ahead. Labels,
definitions and definition sources are read from the RDF directly, so they count even when a file
does not declare its annotation properties.

`decisions.json` records decisions about findings, so the report shows the decision instead of
raising the finding again:

```json
{
  "pitfalls": [
    {"id": "P11", "elements": ["*"], "decision": "accepted",
     "reason": "domain and range left open, as in RO", "ref": "notebook E33"}
  ]
}
```

## Sections

| Section | What it reports |
|---|---|
| Consistency and pitfalls | OWL 2 EL consistency and unsatisfiable classes (whelk); the 31 OOPS!-style pitfalls of OWL-MCP, each with its recorded decision; imports not loaded |
| Structure | Classes without a path to the top class: BFO's *entity* when the project uses BFO, otherwise `owl:Thing` |
| Provenance | Terms without a definition; definitions citing a weak source; definitions with their own source annotation |
| Reuse fidelity | Each reused term against the release it came from: missing, obsolete (with its replacement), under an obsolete parent, a different label or definition, a definition the release does not give; identifiers used under two names |
| Traceability | What each kept group on a review desk says it serves (competency questions, use cases) |
| Review status | Candidate terms by source and status, with a drill-down by source, category and group |
| Semantic coherence | CSS, PSS and PDA of every concept family (a class with two or more subclasses), from sentence embeddings; children whose family scores better without them |
| Corpus fit | Domain coverage, ontology relevance and sub-ontology relevance against a domain-concept list; the most frequent domain concepts the ontology lacks; CSS, PSS and PDA of the domain-relevant subset |

Planned: requirements coverage, and review decisions from a desk export.

### Reuse fidelity and `releases.toml`

A reused term is checked against the release of the source it was taken from. For OBO ontologies
ontoreport finds the release itself, but only for ontologies whose release the project records (a
review desk's release text such as "BFO 2019-08-26; RO 2026-09-04", or a date on a source named after
the ontology): it fetches the full release file of that date, or the current release when the dated
file is not published, and reports when the current release is a different version. Identifiers of
other OBO ontologies (terms a source imports, such as ENVO's NCBITaxon classes) are checked against
the release of the source they were taken with. Other sources are listed in `releases.toml`:

```toml
[[source]]
name = "I-ADOPT"                               # the source's name on the desk
namespaces = ["https://w3id.org/iadopt/ont/"]  # IRIs it defines
files = ["https://w3id.org/iadopt/ont/1.1.0"]  # URLs or paths relative to this file
version = "1.1.0"                              # the version the project used

[[source]]
name = "Darwin Core"                           # a vocabulary published as a CSV table
namespaces = ["http://rs.tdwg.org/dwc/terms/"]
files = ["https://raw.githubusercontent.com/tdwg/dwc/<commit>/vocabulary/term_versions.csv"]
csv = { iri = "term_iri", label = "label", definition = "definition", status = "status", current = ["recommended"] }
```

When a term is in several releases, the one that defines it is used (a definition beats a label beats
a bare declaration), preferring the source the desk names: Darwin Core's own definitions of the Dublin
Core terms it uses, RO for BFO's relations. Desk candidates marked Out are not checked.

Releases are downloaded once into `~/.ontoreport/releases/` (at most `[releases] max_mb` each, 100 MB
by default) and read in parallel; on the ODINI review desk (12,712 terms, 13 releases) the whole
report takes about a second once they are cached. `--offline` uses only what is cached.

### Coherence and corpus fit

These two sections implement the measures of Zaitoun, Sagi and Hose, *Automated Ontology Evaluation:
Evaluating Coverage and Correctness using a Domain Corpus* (WWW '23 Companion). In 2023 the
embeddings came from a BERT model pre-trained on the domain corpus; ontoreport uses a general
sentence-embedding model, all-MiniLM-L6-v2 by default, which agreed better with the 2023 domain
experts (AUC 0.95 against 0.89 on the 326 labelled ENVO pairs, labels with definitions) and runs on
any CPU. A child is flagged as possibly misplaced by the 2023 rule: removing it raises
`0.05·PDA + 0.95·CSS` of its family by more than 0.012.

On the 2023 ontologies and concept list, coverage and relevance reproduce the published table:

| | Classes | Domain-relevant subset | Ontology relevance | Sub-ontology relevance | Domain coverage |
|---|---|---|---|---|---|
| ENVO (2023) | 6,566 | 2,585 | 0.11 | 0.28 | 0.05 |
| ENVO (ontoreport) | 6,566 | 2,555 | 0.108 | 0.279 | 0.048 |
| OMIT (2023) | 87,816 | 5,379 | 0.01 | 0.26 | 0.10 |
| OMIT (ontoreport) | 87,816 | 5,558 | 0.017 | 0.261 | 0.098 |
| SWEET (2023) | 4,533 | 3,241 | 0.34 | 0.48 | 0.11 |
| SWEET (ontoreport) | 4,532 | 3,238 | 0.349 | 0.488 | 0.107 |

The small differences come from normalization: 2023 used spaCy's lemmatizer, ontoreport a fixed rule
set for plural nouns. CSS and PSS are lower than in 2023 by design (a different model); compare
them between ontologies or versions under the same model, not with the 2023 values.

## Configuration

The first run writes `~/.ontoreport/config.toml` (or `$ONTOREPORT_HOME/config.toml`) with every
setting and its default, commented:

```toml
[model]
repo = "sentence-transformers/all-MiniLM-L6-v2"  # any BERT-family model with model.safetensors
revision = "1110a243fdf4706b3f48f1d95db1a4f5529b4d41"  # pinned commit
pooling = "mean"         # or "cls" (BGE)
max_tokens = 128
batch_size = 8
# dir = "D:/models"      # default: ~/.ontoreport/models

[text]
form = "label+definition"  # or "label", as in 2023

[releases]
max_mb = 100             # largest source release to download
```

The model is downloaded with `curl` into `~/.ontoreport/models/<repo>/<revision>/` the first time a
report needs it (about 91 MB for the default), and vectors are cached in `~/.ontoreport/cache/`, so
a second report on the same ontology embeds nothing and is byte-identical to the first. Embedding
runs one batch per CPU core: ENVO's 3,917 class labels take about 2 s on 32 threads, OMIT's 26,255
about 16 s. `--config FILE` reads another configuration file; `--no-embeddings` skips the model;
`--offline` downloads nothing.

## Agent skill

[`skills/ontoreport`](skills/ontoreport) is an agent skill (for Claude Code / Claude) that teaches the
ontology-assessment workflow:
1. lay out the project so every input is found (`releases.toml`, `decisions.json`, a concept list);
2. run the report and read `report.json`;
3. triage the findings into fixes, recorded decisions and questions for a domain expert
   ([`references/findings.md`](skills/ontoreport/references/findings.md) explains every check);
4. record decisions and re-run.

The skill ships its test cases under `evals/`; they need an ontoreport newer than 0.0.1 on the PATH.

## Install

```bash
npx ontoreport --version
```

```bash
pipx run ontoreport --version
```

Or build from source:

```bash
cargo install --git https://github.com/Minitour/ontoreport
```

## Releasing

Releases are published by `.github/workflows/release.yml` from a version tag, with no stored
tokens: npm through trusted publishing (GitHub OIDC) and PyPI through a trusted publisher. The
first npm version has to be published once by hand; see `scripts/npm-bootstrap.sh`.

## License

MIT
