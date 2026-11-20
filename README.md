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

**Status: a scaffold.** Four resources exist today: the source declaration, and over the work
ledger, the first source, a corpus's graphs, an entity and a bounded neighborhood. The map below is the
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

### The work ledger, the first source

The ledger's items, links, comments and labels are already RDF in ikigai-store, one named graph
per ledger (`urn:iki:ledger:graph:{name}`), so its lift is the identity and its entities (items,
people, repositories) are the IRIs it already uses. Declare it, and a corpus holding it:

```turtle
# <config home>/knowledge/source/ledger.ttl
@prefix dcterms: <http://purl.org/dc/terms/> .
<> dcterms:title "The work ledger" ;
   dcterms:hasPart <urn:iki:ledger:graph:default> .

# <config home>/knowledge/corpus/work.ttl
@prefix dcterms: <http://purl.org/dc/terms/> .
<> dcterms:title "Our repositories, docs and ledger" ;
   dcterms:hasPart <urn:knowledge:source:name:ledger> .
```

```text
source urn:knowledge:graph:corpus:work
source urn:knowledge:entity:id:urn:iki:ledger:default:item:01m2h5t1z80m3b2f corpus=work
source urn:knowledge:neighborhood:entity:urn:iki:ledger:default:item:01m2h5t1z80m3b2f:depth:2 corpus=work nodes=50
```

- **Graphs.** `urn:knowledge:graph:corpus:{c}` lists the corpus's named graphs that exist and
  that the caller may read: sorted IRIs, one per line, empty when there are none.
- **Entity.** Every statement about `{e}`, or naming it, in those graphs, closed over
  `owl:sameAs`: one SPARQL CONSTRUCT through `urn:iki:store:graph-construct`, at most 1000
  statements.
- **Neighborhood.** The statements within `{n}` hops (1 to 3) of `{e}`, in either direction: a
  breadth-first walk of CONSTRUCTs that stops at a node budget (`nodes=`, 1 to 1000, default
  100), keeping each hop's statements in lexical order until the budget is spent, so the cut is
  deterministic. A class (the object of `rdf:type`) is not a neighbor.
- **Faces.** The graph list is `text/plain`; an entity and a neighborhood are `text/turtle` by
  default, `as=application/n-triples` for one statement per line.
- **Authority.** All three take `urn:cap:knowledge:read:corpus:name:{c}`, read the corpus and
  source files through the kernel (so a `urn:cap:fs:read:` grant with `ikigai-fs`), and read the
  store only under the NARROW grants, `urn:cap:store:read:graph:<iri>`. A graph the caller holds
  no grant for is never named to the store: it is absent from every answer exactly as an
  undeclared graph is, so a caller learns nothing about a ledger it holds no token for beyond
  what `urn:iki:store:graphs` already says. A caller with no store graph grant at all is refused
  by the kernel's floor, naming the grant family and no graph.

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
let spaces: Vec<Arc<dyn Space>> = vec![
    Arc::new(ikigai_knowledge::space()),
    Arc::new(mount),
    // The corpus endpoints read the ledger's graphs through the store, by IRI: `store` is
    // the `ikigai_store::DurableStore` the host opened once.
    Arc::new(ikigai_store::space(store.clone())),
];
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
| corpus | `urn:knowledge:corpus:name:{c}` | list of names | the sources a corpus holds; a corpus is also a corridor (below). Today a Turtle file in the config home, `knowledge/corpus/{c}.ttl`, read by the graph list; not served at its own name yet | planned, ledger #3856 |
| document | `urn:knowledge:document:source:{s}:id:{d}` | alias rule | onto the underlying resource: `urn:file:…`, `urn:iki:ledger:item:N`, `urn:repo:{repo}:file:…` | planned, ledger #3856, #3857 |
| lift | `urn:knowledge:lift:document:{d}` | transreption | the document's structure as RDF: ikigai-markdown's generic lift plus a mapping resource; the ledger's own Turtle as is; HTML through ikigai-html; PDF and slides by a text transreptor that keeps page and offsets; a database by an R2RML/RML mapping resource | planned, ledger #3856, #3857, #3860, #3861 |
| passage | `urn:knowledge:passage:document:{d}:range:{start}-{end}` | tiny pure function | a slice of the lift's text; the chunking rule is a resource (`urn:knowledge:chunking:name:{n}`) | planned, ledger #3857 |
| prompt | `urn:knowledge:prompt:name:{n}` | resource | the extraction and answer prompts are files, versioned, golden-threaded | planned, ledger #3858 |
| ontology | `urn:knowledge:ontology:name:{n}` | resource | SHACL shapes the extracted entities and relations must satisfy | planned, ledger #3858 |
| draft | `urn:knowledge:draft:passage:{p}:model:{m}` | composition | `urn:llm:ask` over passage and prompt; the result SHACL-checked against the ontology; stored as `oa:` annotations with provenance; cacheable by input | planned, ledger #3858 |
| verdict | `urn:knowledge:verdict:draft:{x}` | **ATOM** | a person's accept, reject, correction or `sameAs`, through ikigai-judge's correction loop; sealed from models | planned, ledger #3858 |
| entity | `urn:knowledge:entity:id:{e}` (`corpus=`) | generic composer | SPARQL CONSTRUCT over the corpus's graphs the caller may read, closed over `owl:sameAs` by a property path. Over the work ledger's structured graphs today; accepted drafts and verdicts, and ikigai-reason's closure, are ledger #3858 | **live** (ledger), planned #3858 |
| graph | `urn:knowledge:graph:corpus:{c}` | list of names | the named graphs of a corpus that exist and the caller may read, for `graph=` sources | **live** |
| neighborhood | `urn:knowledge:neighborhood:entity:{e}:depth:{n}` (`corpus=`, `nodes=`) | generic composer | a bounded SPARQL CONSTRUCT walk: a depth (1 to 3) and a node budget (default 100, at most 1000) | **live** |
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

Vector search is its own module, with backends behind one contract. Its design is ledger #3855.

| Set | Name | Kind | How | Status |
|---|---|---|---|---|
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

**As built today:** the source declaration is the one atom. Its endpoint reads the file through
the kernel, parses it with the RDF library (oxttl) and writes each face back with it. The graph
list, the entity and the neighborhood are SPARQL through ikigai-store's narrow doors; the only
code beyond the queries is reading the corpus and source files, and the neighborhood's
breadth-first loop, which keeps the node budget (one CONSTRUCT cannot count nodes as it walks).

## Accessibility

What this crate emits that people read, the rules that output follows and how it is tested are
in [ACCESSIBILITY.md](ACCESSIBILITY.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
