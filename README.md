# ikigai-knowledge

Grounded, cited answers over an organization's documents, as ikigai resources.

Ask a question of your repositories, docs and work ledger, and get an answer you can check:

- **Cited.** Every claim names the passage it came from: a document, a page or a character
  range. A claim with no citation is refused before the answer leaves, and so is a citation to a
  passage that does not exist.
- **Capability-scoped.** An answer is composed only from what the CALLER may read. The model
  never sees a graph the caller cannot, so there is nothing to filter afterward. A citation the
  caller may not read is `Denied`, never quietly dropped.
- **Disagreement shown.** When two sources say different things about the same fact, both are
  listed with their citations and their authority. Nothing is silently ranked away, and source
  authority is data you can read and argue with.
- **Fresh by golden threads.** A changed file, a new ledger comment or a merged pull request
  re-derives only what depended on it. There is no scheduled re-ingest.
- **Deterministic and cached.** The same question under the same capability over the same graph
  is a cache hit.

Every step is a resource with a name: a source, a document's lift, a passage, a model's draft, a
person's verdict, an entity, a question and its answer. Model output is provisional and
provenanced: it lands in its own graph, carrying the model that wrote it, and becomes accepted
only by a person's verdict or a stated rule.

**Status: a scaffold.** One resource exists today, the source declaration. The map below is the
contract the rest is built to; each planned row names the ledger item that builds it.

## Use it today

`urn:knowledge:source:name:{name}` serves a **source declaration**: a Turtle file in the config
home that says what a corpus ingests, how it is lifted and how much authority it carries.

```text
<config home>/knowledge/source/{name}.ttl
```

```turtle
@prefix dcterms: <http://purl.org/dc/terms/> .

<> dcterms:title "The work ledger" ;
   dcterms:source <urn:iki:ledger:item> .
```

`<>` names the source itself (`urn:knowledge:source:name:ledger` for `ledger.ttl`). The terms a
declaration uses are not settled yet; the endpoint serves whatever the file states, as long as it
is valid Turtle with no blank nodes.

```text
source urn:knowledge:source:name:ledger
source urn:knowledge:source:name:ledger as=text/turtle
```

- **Faces.** `text/plain` by default: a count, then each subject with its statements beneath it.
  `as=text/turtle` for the graph, every IRI absolute.
- **Authority.** Reading the source `{name}` takes `urn:cap:knowledge:read:source:name:{name}`.
  The declaration is read through the kernel, so the caller also needs read authority over the
  file (with `ikigai-fs`, a `urn:cap:fs:read:` grant).
- **Errors.** A name with nothing behind it is `NotFound`, naming the file it looked for. A
  malformed declaration, or one with a blank node, is refused naming the file.
- **Caching.** The answer is cached under the file's golden thread, so editing the file through
  the kernel, or a watcher over the mount, refreshes it.

### Mounting it in a host

The space is configuration-free and names itself `urn:iki:space:knowledge`. This crate opens no
file: the host binds a file module at `urn:knowledge:file:path:{path}`, jailed to the config
home's `knowledge` directory, beside the space.

```rust,ignore
use std::sync::Arc;
use ikigai_core::{EndpointSpace, Fallback, Kernel, Space, UriTemplate};

let mount = EndpointSpace::new().bind(
    UriTemplate::parse(ikigai_knowledge::FILE_TEMPLATE)?,
    ikigai_fs::FileEndpoint::new(config_home.join("knowledge")).cacheable(),
);
let spaces: Vec<Arc<dyn Space>> = vec![Arc::new(ikigai_knowledge::space()), Arc::new(mount)];
let kernel = Kernel::new(Arc::new(Fallback::new(spaces)));
```

## The resource map

Names follow one rule: singular, the type first, and every partition by a named key. The first
corpus is our own: repositories, docs and the work ledger.

**Kinds.** An **ATOM** holds state. Everything else is composed: an alias, a transreption, a
small pure function, a SPARQL composer or a call to a model.

### ikigai-knowledge

| Set | Name | Kind | How | Status |
|---|---|---|---|---|
| source declaration | `urn:knowledge:source:name:{s}` | **ATOM** (config) | a Turtle file in the config home: which resources to ingest, their lift, their authority rank, the corpus they belong to | **live** |
| corpus | `urn:knowledge:corpus:name:{c}` | list of names | the sources a corpus holds; a corpus is also a corridor (below) | planned, ledger #3856 |
| document | `urn:knowledge:document:source:{s}:id:{d}` | alias rule | onto the underlying resource: `urn:file:…`, `urn:iki:ledger:item:N`, `urn:repo:{repo}:file:…` | planned, ledger #3856, #3857 |
| lift | `urn:knowledge:lift:document:{d}` | transreption | the document's structure as RDF: ikigai-markdown's generic lift plus a mapping resource; the ledger's own Turtle as is; HTML through ikigai-html; PDF and slides by a text transreptor that keeps page and offsets; a database by an R2RML/RML mapping resource | planned, ledger #3856, #3857, #3860, #3861 |
| passage | `urn:knowledge:passage:document:{d}:range:{start}-{end}` | tiny pure function | a slice of the lift's text; the chunking rule is a resource (`urn:knowledge:chunking:name:{n}`) | planned, ledger #3857 |
| prompt | `urn:knowledge:prompt:name:{n}` | resource | the extraction and answer prompts are files, versioned, golden-threaded | planned, ledger #3858 |
| ontology | `urn:knowledge:ontology:name:{n}` | resource | SHACL shapes the extracted entities and relations must satisfy | planned, ledger #3858 |
| draft | `urn:knowledge:draft:passage:{p}:model:{m}` | composition | `urn:llm:ask` over passage and prompt; the result SHACL-checked against the ontology; stored as `oa:` annotations with provenance; cacheable by input | planned, ledger #3858 |
| verdict | `urn:knowledge:verdict:draft:{x}` | **ATOM** | a person's accept, reject, correction or `sameAs`, through ikigai-judge's correction loop; sealed from models | planned, ledger #3858 |
| entity | `urn:knowledge:entity:id:{e}` | generic composer | SPARQL CONSTRUCT over accepted drafts, verdicts and the structured lifts, closed over `owl:sameAs` by ikigai-reason | planned, ledger #3856, #3858 |
| graph | `urn:knowledge:graph:corpus:{c}` | list of names | the named graphs of a corpus, for `graph=` sources | planned, ledger #3856 |
| neighborhood | `urn:knowledge:neighborhood:entity:{e}:depth:{n}` | generic composer | a bounded SPARQL CONSTRUCT: a depth and a node budget | planned, ledger #3856 |
| disagreement | `urn:knowledge:disagreement:entity:{e}` | generic composer | SPARQL: functional properties with more than one value, each with its sources and their authority ranks | planned, ledger #3859 |
| question | `urn:knowledge:question:hash:{h}` | resource | the question text and its corpus, content-addressed | planned, ledger #3859 |
| link | `urn:knowledge:link:question:{h}` | composition | the entities a question names: label match first (SPARQL), a model's draft only as residual | planned, ledger #3859 |
| context | `urn:knowledge:context:question:{h}` | list of names | the linked entities' neighborhoods, plus the nearest passages from `urn:vector:nearest` | planned, ledger #3859 |
| answer | `urn:knowledge:answer:question:{h}` | composition | `urn:llm:ask` over context and the answer prompt, then a citation check: every claim names a passage in the context, which the caller may read. Text, Turtle and JSON-LD faces | planned, ledger #3859 |
| citation | `urn:knowledge:citation:answer:{h}` | generic composer | the answer's `oa:` annotations, each targeting a passage with its selectors | planned, ledger #3859 |

A citation is a [Web Annotation](https://www.w3.org/TR/annotation-model/) whose target is a
passage, located by a `TextQuoteSelector` and a `TextPositionSelector` (page, character range).
Lifts conform to the vocabularies the data already speaks (Dublin Core, SKOS, schema.org, BIBO)
rather than inventing one.

### ikigai-vector (a separate module, pluggable)

Vector search is its own module, with backends behind one contract. Its design is ledger #3855,
written up in [docs/vector-design.md](docs/vector-design.md).

| Set | Name | Kind | How | Status |
|---|---|---|---|---|
| index declaration | `urn:vector:declaration:index:{n}` | **ATOM** (config) | a Turtle file in the config home: the index's backend (`flat` first), its embedding model and its dimensions; sealed from models | planned, ledger #3855 |
| index | `urn:vector:index:name:{n}` | **ATOM** (a materialized cache) | the vectors of a set of passages, held by a backend chosen by configuration (an in-process index first; an external service mounted later): one contract, backends behind it. Rebuildable from its inputs at any time | planned, ledger #3855 |
| embedding | `urn:vector:embedding:model:{m}:hash:{h}` | composition | an embedding call through ikigai-llm; pure, cacheable by text hash and model | planned, ledger #3855 |
| member | `urn:vector:member:index:{n}:passage:{p}` | Sink edge | added when a passage appears, removed when its thread is cut | planned, ledger #3855 |
| nearest | `urn:vector:nearest:index:{n}` (`text=`, `k=`) | composition | the backend's query; hangs on the index's thread; answers passage names, never text | planned, ledger #3855 |

### Corridors and sealed names

- **Corpus.** A corridor per corpus, each binding its own store graphs and index. The first is
  repositories, docs and the work ledger; the second is a rebuilt semantic CMS of notes,
  presentations, a reference library and bookmarks (ledger #3862).
- **Capability.** The caller's capability decides which named graphs exist for the request.
  There is nothing to filter afterward.
- **Temporal (as-of).** "What did we know on 2026-10-01": git history and the ledger's
  append-only record make this real for the first corpus, as an as-of corridor over the lifts.
- **Model.** Which model drafts is context, never an argument threaded everywhere.
- **Sealed.** Verdicts (no model writes one), source declarations (configuration, not a model's
  choice), and the citation check (no answer bypasses it).

### Views

Composition produces data (ikigai-ui's renderer-neutral model), never code in a renderer, and a
view is a query plus a renderer. The lenses: an entity card, a passage with its cited range
highlighted, the disagreement list, and an answer with its citations.

### Code that cannot be a composition, and why

- **The chunker:** a tiny pure function; the rule it follows is a resource.
- **The PDF and slide text transreptor:** needs a vetted extraction crate, and must keep page and
  character offsets or citations lose their target.
- **The R2RML/RML engine:** a W3C standard. A vetted crate if one exists; otherwise implemented
  once, in its natural home, with the W3C test cases as vectors.
- **The vector backend:** stateful by nature.
- **The citation check:** a tiny pure function over the answer and the context list.

Everything else is SPARQL, templates, `urn:llm:ask` and lists of names. If the answer assembly
wants Rust, an s-expression program is tried first, and the reason it fell short is written
down.

**As built today:** the source declaration is the one atom, and the only code. Its endpoint reads
the file through the kernel, parses it with the RDF library (oxttl) and writes each face back
with it.

## Accessibility

What this crate emits that people read, the rules that output follows and how it is tested are
in [ACCESSIBILITY.md](ACCESSIBILITY.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
