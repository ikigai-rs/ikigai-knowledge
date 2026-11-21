# Disagreement: one shape for ikigai-knowledge and ikigai-judge

Design note for ledger #3891. **Design only:** nothing here is built, no code lands with it, and
no resource in either module changes until the decisions at the end are made. Related: #3859
(ikigai-knowledge's question, answer and disagreement resources), #2677 (ikigai-judge's design),
#2728.

## 1. The situation both modules describe

Two modules say "these disagree", about different things, in different shapes.

- **ikigai-knowledge** plans `urn:knowledge:disagreement:entity:{e}` (README, the resource map,
  ledger #3859): "SPARQL: functional properties with more than one value, each with its sources
  and their authority ranks." Its principle: "When two sources say different things about the
  same fact, both are listed with their citations and their authority. Nothing is silently
  ranked away."
- **ikigai-judge** serves `urn:iki:judge:disagreements:{q}` (README, `src/lib.rs`
  `disagreements`, `src/endpoints.rs` `disagreements_door`): of the items piped in, every one
  where the deciding verdict and another producer's verdict name different labels. A person's
  correction decides, so it resolves the item and the item drops out.

Underneath, both are the same thing: **about one subject, on one aspect, two or more attributed
positions that cannot all hold, each with its evidence, and optionally a person's resolution.**

| | ikigai-knowledge (planned) | ikigai-judge (as built, 0.2.0) |
|---|---|---|
| subject | an entity `{e}` | an item: its IRI, or `urn:iki:judge:text:{digest}` when none is given |
| aspect | a functional property | the question `{q}` (its SKOS concept scheme) |
| a position's value | a property value | a label concept |
| who holds it | a source (its declaration) | a producer: `person`, `person-shared`, `mapping`, `model`, `stale`, `bayes` |
| its standing | the source's authority rank | the producer's rank (person, mapping, model, stale, bayes) |
| its evidence | citations: `oa:` annotations targeting passages | provenance pairs: the atom resource, agent, created; bayes clues |
| which one counts | none chosen: "nothing is silently ranked away" | one decides (`decide`), the rest are listed against it |
| resolution | a person's verdict (`urn:knowledge:verdict:draft:{x}`, planned, for drafts) | a person's correction (`urn:iki:judge:correct:{q}`), per corridor |
| when resolved | not designed | drops out of the list |
| faces | Turtle (like every knowledge answer) | `text/plain`, `application/json`; no Turtle |
| shape | not designed | one row per pair: `deciding` and `other` |

What this note fixes: one vocabulary, one resource name pattern, and one way a resolution is
recorded, so a single ikigai-ui lens (the README's "disagreement list" view) presents either
module's disagreement without knowing which module served it.

## 2. The shape

A disagreement is a node with a subject, an aspect, and two or more positions. It is a
composition in both modules, never an atom: it is recomputed from the positions that exist, and
cached on their threads. Only a resolution is stored, and it is stored where each module already
stores a person's word.

```turtle
@prefix knowledge: <https://ikigai-rs.dev/ns/knowledge#> .
@prefix oa: <http://www.w3.org/ns/oa#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix dcterms: <http://purl.org/dc/terms/> .

<urn:iki:judge:disagreement:contact:4f1c…> a knowledge:Disagreement ;
    knowledge:about <urn:example:enquiry:812> ;          # the subject
    knowledge:aspect <urn:example:contact-scheme> ;       # what the positions answer
    knowledge:position <…:position:model>, <…:position:bayes> ;
    knowledge:deciding <…:position:model> ;               # optional: who answers today
    knowledge:status knowledge:open .                     # open | resolved

<…:position:model> a oa:Annotation ;
    oa:hasTarget <urn:example:enquiry:812> ;
    oa:hasBody <urn:example:contact-scheme#spam> ;        # the value it holds
    prov:wasAttributedTo <urn:llm:ollama:ask> ;           # who holds it
    knowledge:producer "model" ;                          # what kind of holder
    knowledge:rank 3 ;                                     # its standing; lower answers first
    prov:wasDerivedFrom <urn:iki:judge:model:contact:…> . # its evidence
```

### Terms

Standard terms first, as both modules already do (judge: "Published terms only"; knowledge:
"conform to the vocabularies the data already speaks").

- **A position is an `oa:Annotation`.** Both modules already store their positions that way: a
  judge verdict is `oa:Annotation`, `oa:motivatedBy oa:classifying`, `oa:hasTarget` the item,
  `oa:hasBody` the label; a knowledge citation is an `oa:Annotation` targeting a passage. The
  position's `oa:hasTarget` is the subject and its `oa:hasBody` the value (an IRI), or
  `oa:bodyValue` for a literal value (a date, a number, a name).
- **Who holds it:** `prov:wasAttributedTo` the agent (a source declaration's IRI, a model
  endpoint, a principal). A person's position also carries `dcterms:creator`, as the judge's
  person atom already does.
- **Its evidence:** `prov:wasDerivedFrom` each thing it rests on: a knowledge citation (which
  itself targets a passage with its selectors), or a judge atom resource. A lens follows these
  to show the cited range or the atom; it never needs the module's own provenance pairs.

Nine new terms, because no standard names them:

| term | kind | meaning |
|---|---|---|
| `knowledge:Disagreement` | class | positions about one subject on one aspect that cannot all hold |
| `knowledge:about` | property | the subject, an IRI |
| `knowledge:aspect` | property | what the positions answer: a functional property (knowledge) or a question's concept scheme (judge) |
| `knowledge:position` | property | one position, at least two per disagreement |
| `knowledge:producer` | property | the kind of holder, a short word a person reads: `source`, `person`, `person-shared`, `mapping`, `model`, `stale`, `bayes` |
| `knowledge:rank` | property | the position's standing as an integer, lower answers first; absent when the module ranks nothing |
| `knowledge:deciding` | property | the position the module answers with today, if it chooses one |
| `knowledge:status` | property | `knowledge:open` or `knowledge:resolved` (two named individuals) |
| `knowledge:resolvedBy` | property | on a resolved disagreement, the person's position that resolved it |

Four of them, with the class, carry the shape (`about`, `aspect`, `position`, `status`); the
rest are optional.

**N-ary, not pairs.** The judge's text and JSON faces emit one row per pair (`deciding`,
`other`). The shared shape is one node per subject and aspect with all its positions, because a
knowledge property can have three values from three sources and none of them is privileged.
`knowledge:deciding` recovers the judge's pairs exactly: each non-deciding position against the
deciding one.

**Ranking is shown, never applied by the lens.** `knowledge:rank` and `knowledge:deciding` are
data a person reads and argues with, which is what "nothing is silently ranked away" asks for.
A knowledge disagreement may carry ranks (its sources' authority) and no `deciding`; a judge one
always carries both.

### Faces

- **Turtle** (`text/turtle`) is the shared face: the shape above, and what the lens reads. The
  judge's disagreement door gains `as=text/turtle`; its JSON and text faces stay as they are, so
  no caller of 0.2.0 breaks.
- **Plain text** follows each repository's ACCESSIBILITY.md: plain lines of words, one fact per
  line, nothing carried by layout or symbol alone. A proposed shared sentence shape, pinned by
  tests when built:

  ```text
  disagreement about urn:example:enquiry:812 on urn:example:contact-scheme: 2 positions, open
    model says spam, rank 3, deciding, from urn:iki:judge:model:contact:…
    bayes says ham, rank 5, from urn:bayes:contact:model
  ```

  This note adds no output, so neither ACCESSIBILITY.md changes with it; the change that builds
  a face adds its lines there.

## 3. Names

Each module keeps its own URN namespace and convention; what is shared is the type word
**`disagreement`**, singular, and that every name partitions by the subject's own key.

| | the list | one disagreement |
|---|---|---|
| ikigai-knowledge | `urn:knowledge:disagreement:entity:{e}` (`corpus=`): every disagreement about `{e}`, one per functional property (as planned) | `urn:knowledge:disagreement:hash:{h}`, `{h}` the bare-hex BLAKE3 of the subject IRI, a newline, and the aspect IRI |
| ikigai-judge | `urn:iki:judge:disagreements:{q}` (`items=`), unchanged | `urn:iki:judge:disagreement:{q}:{digest}`: one item's, `{digest}` the judged text's, as every other judge door is keyed |

Why a hash for knowledge's single name: an entity name is itself an IRI with colons
(`urn:iki:ledger:default:item:…`), and so is a property, so two of them in one URN cannot be split
back apart. A digest of the pair is stable as positions come and go, so a lens link or a person's
note can name the disagreement before and after it is resolved.

The judge's plural list name predates the knowledge naming rule and is kept for compatibility;
the singular name is new. Whether to rename the list is decision 3.

## 4. Resolution

**A resolution is a person's position on the subject, not a statement about the disagreement.**
The person says what the value is; the disagreement composition sees a position whose holder is
a person, and reports itself `resolved`, `knowledge:resolvedBy` that position. This is exactly
what the judge does today (a correction is a person verdict; `disagreements` returns nothing
when a person decides), and it means no module stores a disagreement or its status.

- **Where it is stored:** where each module already keeps a person's word, sealed from models.
  The judge: the person atom written by `urn:iki:judge:correct:{q}`, unchanged. Knowledge: the
  verdict atom. Today that is planned only for drafts (`urn:knowledge:verdict:draft:{x}`); a
  disagreement between two structured sources (two ledger statements) has no draft to judge, so
  knowledge needs a person's position on an entity and property: decision 5.
- **What it may say:** one of the positions' values, or a value no position held. Either way
  the losing positions are kept and shown: a resolution never deletes evidence.
- **Whose it is:** a resolution is a person's, in a corridor. The judge already separates a
  reader's own corridor from the shared (curator's) view; a reader's correction resolves the
  disagreement for that reader only. The shared shape carries that by the resolving position's
  `knowledge:producer` (`person` or `person-shared`) and `dcterms:creator`.
- **After it:** the judge drops a resolved item from its list. Knowledge's principle argues for
  keeping it visible. Decision 4.

## 5. One lens

The lens (ikigai-ui's renderer-neutral model; a query plus a renderer) reads only the Turtle
face, and needs nothing module-specific:

1. one heading per `knowledge:Disagreement`: its subject, its aspect, its status;
2. one row per position, ordered by `knowledge:rank` and then by IRI, so the order is
   deterministic: the holder (`knowledge:producer`, `prov:wasAttributedTo`), the value, whether it
   is `deciding`;
3. its evidence, each `prov:wasDerivedFrom` followed: a citation opens the passage lens with the
   cited range highlighted; any other resource is shown by name;
4. when resolved, the resolving person and their value, beside the positions, never instead of
   them.

The lens reads under the caller's capability like every other read: a position whose evidence
the caller may not read shows that evidence as `Denied`, never quietly dropped.

## 6. What this does not change

- No judge resource is renamed or removed, and its JSON and text faces keep their shape.
- `decide`, the judge's rank order, and what counts as a disagreement there stay as built.
- Knowledge's planned detection rule (a functional property with more than one value) is
  unchanged; this note fixes only how a detected disagreement is written.

## Decisions for Brian

1. **One term, `disagreement`, and the N-ary shape in section 2** (a subject, an aspect, two or
   more `oa:Annotation` positions, optional `deciding` and `rank`), rather than the judge's
   pairs. *Recommendation:* adopt it. Pairs cannot represent three sources without choosing one
   to pair against, which is the silent ranking knowledge rules out; `knowledge:deciding`
   recovers the judge's pairs exactly.
2. **Where the new terms live.** Options: ikigai-knowledge's vocabulary
   (`https://ikigai-rs.dev/ns/knowledge#`, `src/vocabulary.ttl`), a new neutral namespace
   (`https://ikigai-rs.dev/ns/disagreement#`), or ikigai-judge's own. *Recommendation:* the
   knowledge vocabulary. It is the only ikigai vocabulary with a definition file and a test that
   holds every term to it; the judge names the IRIs as constants and links no crate. The cost is
   that judge output carries `knowledge:` terms; if that reads wrong, the neutral namespace is the
   alternative, at the price of a second ontology to publish.
3. **The judge's list name.** Keep `urn:iki:judge:disagreements:{q}` and add the singular
   `urn:iki:judge:disagreement:{q}:{digest}`, or rename the list to the singular rule.
   *Recommendation:* keep the list name and add the singular door with the Turtle face. The
   judge has its own positional convention, a rename breaks callers, and the lens needs the
   shape, not the name.
4. **A resolved disagreement: listed or gone.** The judge drops it today. *Recommendation:*
   both modules list open disagreements by default, and a resolved one stays readable at its
   single name with `knowledge:status knowledge:resolved` and its `knowledge:resolvedBy`, so
   the record of who disagreed and who settled it is never lost. The judge's list keeps its
   current behavior.
5. **How a person resolves a knowledge disagreement between two structured sources**, where
   there is no draft for `urn:knowledge:verdict:draft:{x}` to judge. *Recommendation:* widen the
   planned verdict atom to a person's position on an entity and property (an `oa:Annotation` by
   the person, sealed from models, in the caller's corridor), built with ledger #3858's
   verdicts, not a separate resolution atom.
6. **Whose resolution counts for whom.** *Recommendation:* the judge's rule for both: a
   reader's resolution holds in their own corridor; a curator's is the shared view. The shape
   says which by `knowledge:producer` and `dcterms:creator`.
7. **Single disagreement names in knowledge** by `urn:knowledge:disagreement:hash:{h}` (a
   digest of subject and aspect). *Recommendation:* adopt it; two IRIs cannot be split back out
   of one URN, and the digest stays stable while positions change.
8. **The shared Turtle face first, then the lens.** *Recommendation:* the judge's Turtle face
   lands first (it has real disagreements today, and gives the lens test data), then
   knowledge's disagreement resource in #3859 writes the same shape, then the lens.
