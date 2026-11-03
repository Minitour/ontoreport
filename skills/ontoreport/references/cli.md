# ontoreport command line and inputs

```
ontoreport <project-folder | ontology-file> [--out report.html] [--json-only]
           [--concepts FILE] [--config FILE] [--no-embeddings] [--offline] [--include-out]
```

| Flag | Effect |
|---|---|
| `--out FILE` | HTML report path (default `report.html`); the JSON goes next to it with `.json` |
| `--json-only` | write only the JSON |
| `--concepts FILE` | domain-concept list for corpus fit (overrides one found in the project) |
| `--config FILE` | read this configuration instead of `~/.ontoreport/config.toml` |
| `--no-embeddings` | skip the embedding model: no coherence, corpus fit without CSS/PSS/PDA |
| `--offline` | download nothing; use the cached model and releases only |
| `--include-out` | also check desk candidates marked Out (by default only In and Debatable are checked) |

Environment: `ONTOREPORT_HOME` moves `~/.ontoreport`; `ONTOREPORT_TIMING=1` prints the time of each
check to stderr; `HF_TOKEN` is sent to Hugging Face for gated models.

Exit codes: `0` report written; `2` input or configuration error (message on stderr). Findings never
change the exit code.

## Where inputs are looked for

The project folder and its `data/`, `ontology/`, `ontologies/`, `src/ontology/`, `qc/` and `corpus/`
subfolders.

| File | Read as |
|---|---|
| `*.owl`, `*.rdf`, `*.xml` | RDF/XML ontology |
| `*.ttl`, `*.nt`, `*.ofn` | Turtle, N-Triples, OWL functional syntax |
| `g0.json`, `groups.json`, `check.json` | review desk: statements of an ontology file, candidate terms, the desk's self-check |
| `decisions.json` | recorded decisions (`pitfalls` and `findings` lists) |
| `releases.toml` | source releases (below) |
| `concepts.json`, `concepts.txt`, `domain-concepts.json`, `domain-concepts.txt` | domain concepts |

An ontology file that cannot be parsed becomes a finding ("file could not be read"); the rest of the
report goes ahead.

## releases.toml

One `[[source]]` per source. A reused term is matched to a source by its IRI namespace, or by the source
name a review desk gives it.

```toml
[[source]]
name = "PROV-O"                              # as the desk names the source (optional)
namespaces = ["http://www.w3.org/ns/prov#"]  # IRI prefixes the source defines
files = ["https://www.w3.org/ns/prov-o.ttl"] # URLs or paths relative to this file; all are read
version = "2013-04-30"                       # the version the project used (optional)

[[source]]
name = "NVS L22"
namespaces = ["http://vocab.nerc.ac.uk/collection/L22/current/"]
files = ["http://vocab.nerc.ac.uk/collection/L22/current/?_profile=nvs&_mediatype=text/turtle"]

[[source]]
name = "Darwin Core"                         # a CSV term table: one row per term version
namespaces = ["http://rs.tdwg.org/dwc/terms/", "http://rs.tdwg.org/dwc/iri/"]
files = ["https://raw.githubusercontent.com/tdwg/dwc/<commit>/vocabulary/term_versions.csv"]
csv = { iri = "term_iri", label = "label", definition = "definition", status = "status", current = ["recommended"] }
```

RDF releases may be RDF/XML, Turtle or N-Triples; the format is detected from the content, and a web
page served instead of a file is rejected. From each release ontoreport reads labels (`rdfs:label`,
`skos:prefLabel`), definitions (`IAO:0000115`, `IAO:0000600`, `skos:definition`, `prov:definition`,
`dcterms:description`, else `rdfs:comment`), deprecation (`owl:deprecated`, or a label starting
"obsolete"), replacements (`IAO:0100001`, `dcterms:isReplacedBy`) and named parents (`rdfs:subClassOf`,
`rdfs:subPropertyOf`, `skos:broader`).

OBO ontologies need no entry when the desk records their release date: ontoreport tries
`obo/<id>/releases/<date>/<id>.owl`, then `obo/<id>/<date>/<id>.owl`, then the current `obo/<id>.owl`
(and reports when its version differs). Add an entry to override this, for example to point at a local
copy.

Downloads go to `~/.ontoreport/releases/`, at most `[releases] max_mb` each (100 MB by default). A URL
that does not exist is remembered (a `.missing` file) and not asked for again; delete the cache folder to
start over.

## config.toml

Written on first run with every setting commented:

```toml
[model]
repo = "sentence-transformers/all-MiniLM-L6-v2"   # BERT-family model with model.safetensors
revision = "1110a243fdf4706b3f48f1d95db1a4f5529b4d41"
pooling = "mean"          # or "cls" (BGE models)
max_tokens = 128
batch_size = 8
# dir = "D:/models"       # default ~/.ontoreport/models

[text]
form = "label+definition" # or "label"

[releases]
max_mb = 100
```

Changing the model or the text form changes coherence scores; compare reports only under the same
settings (each report's coherence notes name the model and text form).

## Domain-concept files

- the 2022 extraction output: `{"concepts": [{"concept_name": "...", "occurrences_in_papers": 12}]}`;
- a JSON list of strings, or of objects with `concept` (or `name`) and `frequency` (or `papers`);
- text, one concept per line, optionally a tab and a frequency; `#` starts a comment.

Concepts and class labels are compared after lower-casing and reducing plural nouns to the singular.
