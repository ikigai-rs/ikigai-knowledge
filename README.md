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

**Status: a scaffold.** These resources exist today: the source declaration; over the work
ledger, the first source, a corpus's graphs, an entity, a bounded neighborhood and a decision
trail, the first cited answer; and over files, a source's document by its id, a chunking
rule, a document's passages under it, one passage, a document's revision and a passage as of a
revision, which a persistent HTTP IRI reaches through ikigai-name. The map below is the
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
  statements, the first 1000 in a stable order.
- **Neighborhood.** The statements within `{n}` hops (1 to 3) of `{e}`, in either direction: a
  breadth-first walk of CONSTRUCTs that stops at a node budget (`nodes=`, 1 to 1000, default
  100), keeping each hop's statements in lexical order until the budget is spent, so the cut is
  deterministic. A class (the object of `rdf:type`) is not a neighbor. Each hop asks the store
  for at most 10 statements per node of budget, in a stable order, so a hub node (a state or a
  person that thousands of items name) shortens the walk instead of failing it on the store's
  own limits.
- **Cut answers say so.** An entity past its 1000 statements, or a neighborhood cut by its
  budget or a hop's limit, carries one more statement on the answer's own IRI,
  `<{answer}> <https://ikigai-rs.dev/ns/knowledge#truncated> true`. A complete answer carries
  none.
- **Vocabulary.** `knowledge:truncated` is defined in the module's own vocabulary,
  [`src/vocabulary.ttl`](src/vocabulary.ttl) (the `VOCABULARY` constant): an `owl:Ontology` for
  `https://ikigai-rs.dev/ns/knowledge` whose `owl:versionInfo` is the crate version. A test
  holds every term under that namespace the crate names to a definition there. The vocabulary is
  published at its namespace once deployed.
- **Faces.** The graph list is `text/plain`; an entity and a neighborhood are `text/turtle` by
  default, `as=application/n-triples` for one statement per line. Entity and neighborhood
  answers omit statements with a blank subject or object before applying statement limits.
- **Authority.** All three take `urn:cap:knowledge:read:corpus:name:{c}`, read a source's graphs
  only under `urn:cap:knowledge:read:source:name:{s}` (a source the caller may not read
  contributes nothing, as an undeclared one), read the corpus and
  source files through the kernel (so a `urn:cap:fs:read:` grant with `ikigai-fs`), and read the
  store only under the NARROW grants, `urn:cap:store:read:graph:<iri>`. A graph the caller holds
  no grant for is never named to the store: it is absent from every answer exactly as an
  undeclared graph is, so a caller learns nothing about a ledger it holds no token for beyond
  what `urn:iki:store:graphs` already says. A caller with no store graph grant at all is refused
  by the kernel's floor, naming the grant family and no graph.

### A decision trail, the first cited answer

"Who decided X, where, and what is still open", over a structured corpus. The ledger is
already RDF, so the answer needs no extraction, no drafts and no vector index:

```text
source urn:knowledge:trail:corpus:work topic=oxigraph
source urn:knowledge:trail:corpus:work topic=oxigraph as=text/turtle
```

1. **Link.** The topic is matched, case-insensitively and as a literal, against `dcterms:title`,
   `rdfs:label` and `skos:prefLabel` in the corpus's graphs the caller may read: one SPARQL
   CONSTRUCT, the first 5 entities in IRI order.
2. **Context.** Each linked entity's two-hop neighborhood (an item, its comments and what they
   name), walked exactly as the neighborhood endpoint walks, under the CALLER's capability, with
   its node budget (`nodes=`, default 100). A passage is a subject with a title or a body: an
   item's text is its title and its body joined by a blank line, a comment's is its body.
3. **Draft.** `urn:llm:ask` (ikigai-llm, which the host binds), given the passages only, fenced
   as data, and asked for claim blocks: `CLAIM:`, `CITE:` a passage's IRI, `QUOTE:` its exact
   words.
4. **Check.** A claim is kept only if it cites a passage of the context and its quote is found
   verbatim in that passage's text (at least 8 characters, or the whole passage). Every other
   claim is removed and counted. Literal quotation marks are kept when the whole quote matches.
   With none left, or a topic that links to nothing, the answer abstains, and a topic that links
   to nothing never reaches the model.

- **Faces.** `text/plain` by default: the topic, then each kept claim with its passage, its
  character range and its quote, then how many claims were removed. `as=text/turtle` is one
  [Web Annotation](https://www.w3.org/TR/annotation-model/) per claim, `oa:bodyValue` the claim,
  targeting the passage with an `oa:TextQuoteSelector` and an `oa:TextPositionSelector`
  (`oa:start` inclusive, `oa:end` exclusive, in characters). The answer's own IRI carries
  `knowledge:abstained true` when it abstains, `knowledge:removedClaims n` when the check removed
  any, and `knowledge:truncated true` when a neighborhood was cut.
- **Authority.** As the neighborhood: `urn:cap:knowledge:read:corpus:name:{c}`, source grants,
  the files, and the narrow store graph grants. A graph the caller may not read is never in
  the context, so the model never sees it, and a claim citing it is removed. The model call is
  the caller's too, so the trail also declares `urn:cap:llm:use:*`, the family ikigai-llm
  enforces at `urn:llm:ask`: a caller with no `urn:cap:llm:use:<provider>` is `Denied` before
  any model call, and `urn:kernel:actions` does not offer it the trail. ikigai-llm still judges
  which provider and its own network grant.
- **Caching.** None: a model drafts it, so it is live by decision. The prompt is a constant in
  the crate until the prompt resources land (ledger #3858).
- **Errors.** A topic that is empty, longer than 200 characters or on more than one line is
  `InvalidArgument` naming `topic`; a kernel with nothing at `urn:llm:ask` is `NotFound` saying
  how a host binds one.

### Documents and passages

A document's passages come from a **chunking rule**, which is itself a resource: a Turtle file
in the config home.

```turtle
# <config home>/knowledge/chunking/prose.ttl
<> <https://ikigai-rs.dev/ns/knowledge#maxCharacters> 1200 .
```

```text
source urn:knowledge:chunking:name:prose
source urn:knowledge:chunk:document:urn:knowledge:file:path:docs/README.md:chunking:prose
source urn:knowledge:passage:document:urn:knowledge:file:path:docs/README.md:range:0-412
```

- **Rule.** A paragraph is a run of lines that are not blank. Paragraphs are packed into one
  passage while it spans at most `maxCharacters` characters (1 to 100000); a longer paragraph
  is cut into pieces of that many. A passage runs from its first paragraph's first character to
  its last paragraph's last, in characters, start inclusive and end exclusive, as a
  `TextPositionSelector` counts them.
- **Documents.** `{d}` is the document's own IRI: a file under the config home's `knowledge`
  directory, `urn:knowledge:file:path:{path}`, read through the kernel from the mount the host
  binds, or a source's document, `urn:knowledge:document:source:{s}:id:{id}` (below). A path
  that leaves the mount is `InvalidArgument` naming `d`. Repository trees through
  `urn:repo:*`, ikigai-markdown's lift and browse's archived explanations are not built yet.
- **Faces.** All three are `text/plain`: the rule as one sentence, the chunk list as passage
  names one per line in document order (empty for a document with no text), and a passage as
  its text exactly.
- **Authority.** The rule and the chunk list take `urn:cap:knowledge:read:chunking:name:{n}`;
  all three read the files through the kernel, so the caller also needs read authority over
  them (with `ikigai-fs`, a `urn:cap:fs:read:` grant).
- **Errors.** A document whose path holds `:range:` or ends in `:range` is `InvalidArgument`
  naming `d`: a passage name splits `{d}` at its first `:range:`, so its passages would be
  listed under names that do not read back. A range that ends before it starts is
  `InvalidArgument` naming `end`; one past the text is `NotFound` saying how many characters
  the document has. A missing document or rule is `NotFound` naming it; a rule that does not
  state `maxCharacters` exactly once, as a whole number in range, is refused naming its file.
- **Golden threads.** Each answer is cached under the threads of the files it read: a passage
  and a chunk list under the document's file, a chunk list also under the rule's. A changed
  file re-derives only its own passages, and every other document's stay cached
  (`tests/passage.rs` edits one file and shows it); a changed rule re-derives every chunk list
  under it.

### A source's documents

A source that holds documents says where they live, once, in its declaration:

```turtle
# <config home>/knowledge/source/docs.ttl
@prefix dcterms: <http://purl.org/dc/terms/> .
@prefix knowledge: <https://ikigai-rs.dev/ns/knowledge#> .
<> dcterms:title "Our docs" ;
   knowledge:documentBase <urn:knowledge:file:path:docs/> .
```

```text
source urn:knowledge:document:source:docs:id:guide/intro.md
source urn:knowledge:chunk:document:urn:knowledge:document:source:docs:id:guide/intro.md:chunking:prose
```

- **Alias.** `urn:knowledge:document:source:{s}:id:{id}` answers what the base followed by
  `{id}` answers (`urn:knowledge:file:path:docs/guide/intro.md` here), byte for byte and in
  its media type. `{id}` is a relative path: no empty, `.` or `..` segment, no whitespace,
  `?` or `#`, so it names only something beneath the base.
- **Passages.** The chunk list and the passage take an alias as their `{d}`, like a file. A
  passage of an aliased document is named under the alias, so a citation names the source it
  rests on.
- **Authority.** The alias takes `urn:cap:knowledge:read:source:name:{s}`, refused with
  `Denied` before anything is read, and the declaration's file read; the target is read
  through the kernel under the caller's own capability, so the resource there judges it.
- **Errors.** An id that is not a relative path is `InvalidArgument` naming `d`. A source not
  declared is `NotFound` naming its file, and one that does not state `knowledge:documentBase`
  exactly once, as an IRI, is refused naming its file. A document the base does not hold is
  `NotFound` naming the alias and the target.
- **Golden threads.** The alias is cached under the declaration's file and the target's
  thread. A changed file re-derives only the documents and passages over it
  (`tests/document.rs` edits one file and shows the other stays cached); a changed
  declaration re-derives every document of that source.
- **Vocabulary.** `knowledge:documentBase` is defined in [`src/vocabulary.ttl`](src/vocabulary.ttl).

### Citations that stay put

A passage name is a range of the document's CURRENT text, so it moves when the document
changes: a citation made with it can later point at different words, and nothing says so.
A citation meant to be checked later names the document's **revision** as well:

```text
sink   urn:knowledge:revision:document:urn:knowledge:file:path:docs/README.md
source urn:knowledge:passage:revision:{r}:document:urn:knowledge:file:path:docs/README.md:range:0-412
source urn:knowledge:passage:revision:{r}:document:urn:knowledge:file:path:docs/README.md:range:0-412 as=text/turtle
```

- **Revision.** `urn:knowledge:revision:document:{d}` answers the document's revision: the
  BLAKE3 digest of its text, named with its algorithm (`blake3-` and 64 lowercase hex
  digits, so a later algorithm can sit beside it), one line. `Source` only reads it (cached
  under the document's thread). `Sink` also **archives** the text, content-addressed, at
  `urn:knowledge:file:path:revision/{r}.txt` (the config home's `knowledge/revision/{r}.txt`),
  written through the kernel with `if-none-match=*`, so an archive is never overwritten.
  Whoever cites sinks first, then mints the name (`passage_at_iri`).
- **Passage as of a revision.** `urn:knowledge:passage:revision:{r}:document:{d}:range:{start}-{end}`
  is the text of revision `{r}` from `{start}` to `{end}`. The text face reads the archive
  when there is one, so an edit to the document never recomputes it, and the document only
  while it is still that revision and nothing is archived; the Turtle face also reads the
  current document, to find the current passage. An archive whose digest is
  not `{r}` is refused, never served as that revision. `text/plain` is the text exactly.
- **Its Turtle face** (`as=text/turtle`) is the [Web Annotation](https://www.w3.org/TR/annotation-model/)
  target: an `oa:SpecificResource` whose `oa:hasSource` is the document, with an
  `oa:TextQuoteSelector` (`oa:exact`, and up to 32 characters of `oa:prefix` and `oa:suffix`)
  and an `oa:TextPositionSelector` (`oa:start` inclusive, `oa:end` exclusive, characters of
  that revision), and `knowledge:revision`. **The current version is one link away:**
  `knowledge:currentPassage` names the passage of the current text holding the same words,
  found by the quote selector (exact with its prefix and suffix, else exact where it occurs
  once), and is absent when the words are gone; `oa:hasSource` is the current document.
- **A persistent HTTP IRI.** ikigai-name answers it with a 302, as data in its registry: a
  `redirect` claim whose rule maps `passage/revision/{revision}/range/{range}/path/{path*}`
  onto the URL the host serves the knowledge kernel's resources at. Every level of the path
  is partitioned by a named key. `persistent_path` gives the path under the prefix, each
  segment of `{path}` percent-encoded by core's RFC 6570 expansion (the encoding ikigai-name
  gives a capture), so
  `https://{name host}/knowledge/passage/revision/{r}/range/{start}-{end}/path/{path}`
  redirects to the passage as of `{r}`:

  ```json
  {"prefix": "knowledge", "owner": "urn:cap:name:admin:knowledge",
   "strategy": "redirect", "target": "https://knowledge.example/", "status": 302,
   "rules": [{"match": "passage/revision/{revision}/range/{range}/path/{path*}",
              "target": "https://knowledge.example/urn:knowledge:passage:revision:{revision}:document:urn:knowledge:file:path:{path}:range:{range}"}]}
  ```

  `tests/revision.rs` resolves such an IRI through ikigai-name's own space, after the document
  has changed, and follows the 302 to the exact cited words. 302 because the name is logical
  and its target may move; the name, `{r}` and the range never do.
- **Authority.** Every face takes the file read (with `ikigai-fs`, a `urn:cap:fs:read:`
  grant). Archiving also takes the write, `urn:cap:fs:write:*`, judged by path where the
  archive is written; a later reader needs only the read.
- **Errors.** A revision that is not `blake3-` and 64 lowercase hex digits (a bare digest
  included) is `InvalidArgument` naming `r`;
  the range refusals are a passage's, counted in the revision's characters. A revision that
  is neither the document's current one nor archived is `NotFound`, saying so and naming the
  current document. A Sink that finds a different text already archived under the revision
  is `Conflict`, saying it was not archived and naming the digest found; the archive is
  left as it was.
- **Not built yet.** Revisions of documents other than files in the config home (a commit of a
  repository tree through `urn:repo:*`, the ledger's append-only record) and the as-of corridor
  over lifts (ledger #3049); the trail still cites ledger subjects, which are not files, and
  the planned `answer` (ledger #3859) is what mints these names for files.

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
    // the `ikigai_store::DurableStore` the host opened once. A host that already binds a
    // store space (for the ledger, say) reuses THAT one and adds none: a second space over
    // the same store turns its caching off, and one bound after a cached read refuses
    // every request.
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
| corpus | `urn:knowledge:corpus:name:{c}` | list of names | the sources a corpus holds; a corpus is also a corridor (below). Today a Turtle file in the config home, `knowledge/corpus/{c}.ttl`, read by the graph list; not served at its own name yet | planned, ledger #3857 |
| document | `urn:knowledge:document:source:{s}:id:{d}` | alias rule | onto the underlying resource: the source declaration's `knowledge:documentBase` followed by `{d}`, a pass-through in the target's media type; cached under the declaration's and the target's threads. Over files in the config home today (`urn:knowledge:file:path:…`); `urn:iki:ledger:item:N` and `urn:repo:{repo}:file:…` targets are ledger #3857 | **live** (files), planned, ledger #3857 |
| lift | `urn:knowledge:lift:document:{d}` | transreption | the document's structure as RDF: ikigai-markdown's generic lift plus a mapping resource; the ledger's own Turtle as is; HTML through ikigai-html; PDF and slides by a text transreptor that keeps page and offsets; a database by an R2RML/RML mapping resource | planned, ledger #3857, #3860, #3861 |
| chunking rule | `urn:knowledge:chunking:name:{n}` | **ATOM** (config) | a Turtle file in the config home, `knowledge/chunking/{n}.ttl`: paragraphs packed into passages of at most `knowledge:maxCharacters` | **live** |
| chunk list | `urn:knowledge:chunk:document:{d}:chunking:{n}` | list of names | a document's passages under a rule, in document order; cached under the document's and the rule's threads. Over files in the config home and source documents over them today; repository trees and lifts are ledger #3857 | **live** (files), planned, ledger #3857 |
| passage | `urn:knowledge:passage:document:{d}:range:{start}-{end}` | tiny pure function | a slice of the document's text, in characters, end exclusive. Over files in the config home today; a slice of the lift's text once lifts land | **live** (files), planned, ledger #3857 |
| revision | `urn:knowledge:revision:document:{d}` | tiny pure function, and a Sink | the BLAKE3 digest of the document's text, tagged `blake3-{hex}`; `Sink` archives the text, content-addressed, at `urn:knowledge:file:path:revision/{r}.txt`, never overwriting one. Over files in the config home today; a repository commit is ledger #3049 | **live** (files) |
| passage as of a revision | `urn:knowledge:passage:revision:{r}:document:{d}:range:{start}-{end}` | tiny pure function | the text of revision `{r}`, from the document or its archive, checked against `{r}`; its Turtle face a Web Annotation target with quote and position selectors and `knowledge:currentPassage`; behind a persistent HTTP IRI that ikigai-name 302s to it (above) | **live** (files) |
| prompt | `urn:knowledge:prompt:name:{n}` | resource | the extraction and answer prompts are files, versioned, golden-threaded | planned, ledger #3858 |
| ontology | `urn:knowledge:ontology:name:{n}` | resource | SHACL shapes the extracted entities and relations must satisfy | planned, ledger #3858 |
| draft | `urn:knowledge:draft:passage:{p}:model:{m}` | composition | `urn:llm:ask` over passage and prompt; the result SHACL-checked against the ontology; stored as `oa:` annotations with provenance; cacheable by input | planned, ledger #3858 |
| verdict | `urn:knowledge:verdict:draft:{x}` | **ATOM** | a person's accept, reject, correction or `sameAs`, through ikigai-judge's correction loop; sealed from models | planned, ledger #3858 |
| entity | `urn:knowledge:entity:id:{e}` (`corpus=`) | generic composer | SPARQL CONSTRUCT over the corpus's graphs the caller may read, closed over `owl:sameAs` by a property path. Over the work ledger's structured graphs today; accepted drafts and verdicts, and ikigai-reason's closure, are ledger #3858 | **live** (ledger), planned #3858 |
| graph | `urn:knowledge:graph:corpus:{c}` | list of names | the named graphs of a corpus that exist and the caller may read, for `graph=` sources | **live** |
| neighborhood | `urn:knowledge:neighborhood:entity:{e}:depth:{n}` (`corpus=`, `nodes=`) | generic composer | a bounded SPARQL CONSTRUCT walk: a depth (1 to 3) and a node budget (default 100, at most 1000) | **live** |
| decision trail | `urn:knowledge:trail:corpus:{c}` (`topic=`, `nodes=`) | composition | over a structured corpus: label match (SPARQL), the linked entities' neighborhoods, `urn:llm:ask` over their passages, then the citation check; claims as `oa:` annotations with quote and position selectors; abstains when nothing is grounded. The question-addressed `answer` below generalizes it to prose sources | **live** (ledger #3888) |
| trail annotation and its components | `urn:knowledge:trail:corpus:{c}#claim-{digest}-{n}` with `-target`, `-quote` and `-position` suffixes for its components | output IRIs | the digest covers the topic, claim text, cited passage IRI, exact quote and character range; `n` is the claim's one-based position, distinguishing duplicates | **live** (in Turtle output) |
| disagreement | `urn:knowledge:disagreement:entity:{e}` | generic composer | SPARQL: functional properties with more than one value, each with its sources and their authority ranks; one shape shared with ikigai-judge, designed in [docs/design/disagreement.md](docs/design/disagreement.md) (ledger #3891) | planned, ledger #3859 |
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
  A passage as of a revision is the first piece: one cited passage, pinned by its document's
  digest and archived when it is cited.
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
breadth-first loop, which keeps the node budget (one CONSTRUCT cannot count nodes as it walks). The
decision trail adds the citation check, a small pure function over the draft and the
passages, and the parsing of the draft's claim blocks; its link is a SPARQL CONSTRUCT and its
context is the neighborhood's walk.

## Accessibility

What this crate emits that people read, the rules that output follows and how it is tested are
in [ACCESSIBILITY.md](ACCESSIBILITY.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
