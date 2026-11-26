//! The module recipe as one test: `ikigai-conformance` walks every endpoint
//! [`ikigai_knowledge::space`] binds and reports every violation at once, with no check
//! skipped.
//!
//! - The kernel is the one a host builds: the module's space, then the file mount
//!   (`ikigai-fs`, cacheable) at `urn:knowledge:file:path:{path}` beside it, over a config
//!   home this test owns. The mount is the host's, so the walk owns only the module's
//!   endpoint and inherits the mount's.
//! - `cacheable`: the answer is marked cacheable and hangs from the file's thread, so a cut
//!   of that thread recomputes it (RECOMPUTE) and nothing else does.
//! - `enforced_under`: a caller holding the file read and a grant for ANOTHER source is
//!   refused by the module's own rule, with a typed `Denied`. The trail's caller also holds a
//!   `urn:cap:llm:use:` grant, as it declares, so the kernel's floor is not what refuses it.
//! - `space()` is self-named `urn:iki:space:knowledge`: it takes no parameters and reads
//!   nothing while it is built.

mod common;

use std::sync::Arc;

use common::*;
use ikigai_conformance::{Expected, Fixture, Suite};
use ikigai_core::Verb;

const OWN: [&str; 11] = [
    ikigai_knowledge::SOURCE_ID,
    ikigai_knowledge::GRAPH_ID,
    ikigai_knowledge::ENTITY_ID,
    ikigai_knowledge::NEIGHBORHOOD_ID,
    ikigai_knowledge::TRAIL_ID,
    ikigai_knowledge::CHUNKING_ID,
    ikigai_knowledge::CHUNK_ID,
    ikigai_knowledge::PASSAGE_ID,
    ikigai_knowledge::DOCUMENT_ID,
    ikigai_knowledge::REVISION_ID,
    ikigai_knowledge::PASSAGE_AT_ID,
];

/// The store's endpoints, bound beside the module.
const STORE: [&str; 16] = [
    "store-select",
    "store-graph-select",
    "store-ask",
    "store-graph-ask",
    "store-construct",
    "store-graph-construct",
    "store-describe",
    "store-graph-describe",
    "store-info",
    "store-graphs",
    "store-graph-info",
    "store-dump",
    "store-update",
    "store-graph-update",
    "store-load",
    "store-graph-written",
];

/// `cap` and one provider's `urn:cap:llm:use:` grant, which ikigai-llm takes at urn:llm:ask.
fn with_model(cap: ikigai_core::Capability) -> ikigai_core::Capability {
    let mut scopes: Vec<String> = cap
        .scopes()
        .expect("a scoped capability")
        .iter()
        .cloned()
        .collect();
    scopes.push("urn:cap:llm:use:anthropic".to_string());
    ikigai_core::Capability::scoped(scopes)
}

#[test]
fn conforms() {
    let scratch = Scratch::new("conformance");
    scratch.declare("example", LEDGER);
    scratch.declare("ledger", LEDGER_SOURCE);
    scratch.declare_corpus("example", WORK);
    // A chunking rule and one document, for the passage endpoints.
    let knowledge = scratch.knowledge();
    std::fs::create_dir_all(knowledge.join("chunking")).expect("chunking dir");
    std::fs::create_dir_all(knowledge.join("docs")).expect("docs dir");
    std::fs::write(
        knowledge.join("chunking/example.ttl"),
        "<> <https://ikigai-rs.dev/ns/knowledge#maxCharacters> 40 .\n",
    )
    .expect("write the rule");
    std::fs::write(
        knowledge.join("docs/example.md"),
        "# Example\n\nOne paragraph.\n",
    )
    .expect("write the document");
    let document = format!("{}docs/example.md", ikigai_knowledge::FILE_PREFIX);
    // A source whose documents are the files under docs/, for the document alias.
    scratch.declare(
        "docs",
        "<> <https://ikigai-rs.dev/ns/knowledge#documentBase> <urn:knowledge:file:path:docs/> .\n",
    );
    let aliased = || {
        Fixture::new(ikigai_knowledge::DOCUMENT_ID, Verb::Source)
            .binding("s", "docs")
            .binding("d", "example.md")
    };
    let revision = ikigai_knowledge::revision_of("# Example\n\nOne paragraph.\n");
    let chunking = |cap: Option<ikigai_core::Capability>| {
        let f = Fixture::new(ikigai_knowledge::CHUNKING_ID, Verb::Source).binding("n", "example");
        match cap {
            Some(cap) => f.capability(cap),
            None => f,
        }
    };
    let chunks = || {
        Fixture::new(ikigai_knowledge::CHUNK_ID, Verb::Source)
            .binding("d", &document)
            .binding("n", "example")
    };
    let another_rule = ikigai_core::Capability::scoped([
        ikigai_knowledge::chunking_scope("another"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
    ]);
    let space = Arc::new(ikigai_knowledge::space());
    // The model the trail drafts with: a stub at urn:llm:ask, which a host fills with
    // ikigai-llm. Its draft cites the example item's title, so the check keeps it.
    let (llm, _) = llm_stub(&format!(
        "CLAIM: The item is about wiring the ledger.\nCITE: {ITEM}\nQUOTE: Wire the ledger\n"
    ));
    let kernel = {
        let spaces: Vec<Arc<dyn ikigai_core::Space>> = vec![
            space.clone(),
            Arc::new(mount(&scratch)),
            Arc::new(ikigai_store::space(seeded_store())),
            Arc::new(llm),
        ];
        ikigai_core::Kernel::new(Arc::new(ikigai_core::Fallback::new(spaces)))
    };
    let trail = || {
        Fixture::new(ikigai_knowledge::TRAIL_ID, Verb::Source)
            .binding("c", "example")
            .arg("topic", "ledger")
    };
    let entity = |id: &str| {
        Fixture::new(id, Verb::Source)
            .binding("e", ITEM)
            .arg("corpus", "example")
    };
    let hood = |id: &str| entity(id).binding("n", "2");
    let mut suite = Suite::new();
    for id in STORE {
        suite = suite.inherit(
            id,
            "ikigai-store, bound beside the module by the host; the module reads the ledger's \
             graphs through its narrow doors, and the store's own conformance test covers it",
        );
    }
    let report = suite
        .body_space(space)
        .own_all(OWN)
        .inherit(
            "file",
            "ikigai-fs, the file mount a host binds at urn:knowledge:file:path:{path} over the \
             config home's knowledge directory; its own conformance test covers it",
        )
        .inherit(
            "llm-stub",
            "the model, ikigai-llm's urn:llm:ask, bound beside the module by the host; a stub \
             here, and ikigai-llm's own conformance test covers the real one",
        )
        .self_named_space("knowledge", ikigai_knowledge::space)
        // The work ledger's own vocabulary, which the entity and neighborhood faces carry
        // as is: the ledger's lift is the identity.
        .namespace("https://ikigai-rs.dev/ns/ledger#")
        // The module's own terms, held to the graph it ships rather than waived.
        .vocabulary(ikigai_knowledge::VOCABULARY)
        .cacheable(ikigai_knowledge::SOURCE_ID)
        .cacheable(ikigai_knowledge::GRAPH_ID)
        .cacheable(ikigai_knowledge::ENTITY_ID)
        .cacheable(ikigai_knowledge::NEIGHBORHOOD_ID)
        // A model drafts the trail, so it is live by decision: never served from a cache.
        .live(ikigai_knowledge::TRAIL_ID)
        .cacheable(ikigai_knowledge::CHUNKING_ID)
        .cacheable(ikigai_knowledge::CHUNK_ID)
        .cacheable(ikigai_knowledge::PASSAGE_ID)
        .cacheable(ikigai_knowledge::DOCUMENT_ID)
        .fixture(aliased())
        .enforced_under(aliased().capability(reader(&scratch, "another")))
        .cacheable(ikigai_knowledge::REVISION_ID)
        .cacheable(ikigai_knowledge::PASSAGE_AT_ID)
        .fixture(chunking(None))
        .fixture(chunks())
        .fixture(
            Fixture::new(ikigai_knowledge::PASSAGE_ID, Verb::Source)
                .binding("d", &document)
                .binding("start", "0")
                .binding("end", "9"),
        )
        .fixture(Fixture::new(ikigai_knowledge::REVISION_ID, Verb::Source).binding("d", &document))
        .fixture(
            Fixture::new(ikigai_knowledge::PASSAGE_AT_ID, Verb::Source)
                .binding("r", &revision)
                .binding("d", &document)
                .binding("start", "0")
                .binding("end", "9"),
        )
        .enforced_under(chunking(Some(another_rule.clone())))
        .enforced_under(chunks().capability(another_rule.clone()))
        .fixture(Fixture::new(ikigai_knowledge::SOURCE_ID, Verb::Source).binding("name", "example"))
        .fixture(Fixture::new(ikigai_knowledge::GRAPH_ID, Verb::Source).binding("c", "example"))
        .fixture(entity(ikigai_knowledge::ENTITY_ID))
        .fixture(hood(ikigai_knowledge::NEIGHBORHOOD_ID))
        .fixture(trail())
        .enforced_under(
            Fixture::new(ikigai_knowledge::GRAPH_ID, Verb::Source)
                .binding("c", "example")
                .capability(corpus_reader(&scratch, "another")),
        )
        .enforced_under(
            entity(ikigai_knowledge::ENTITY_ID).capability(corpus_reader(&scratch, "another")),
        )
        .enforced_under(
            hood(ikigai_knowledge::NEIGHBORHOOD_ID).capability(corpus_reader(&scratch, "another")),
        )
        // The trail also declares urn:cap:llm:use:*, so its probe holds a model grant: the
        // kernel's floor admits it, and the module's own corpus rule is what refuses it.
        .enforced_under(trail().capability(with_model(corpus_reader(&scratch, "another"))))
        .enforced_under(
            Fixture::new(ikigai_knowledge::SOURCE_ID, Verb::Source)
                .binding("name", "example")
                .capability(reader(&scratch, "another")),
        )
        .run_blocking(&kernel);
    eprintln!("{report}");
    report.assert_clean_with(&Expected::new().walked(OWN));
    // Exactly three actions are unprobed, each by one check. BODIES for the trail: it is live
    // by decision (a model drafts it), so two independent calls cannot be compared byte for
    // byte. OUTPUTS for the document: a declared pass-through whose face is the target's,
    // which the walk cannot resolve from a binding; `tests/document.rs` holds the served
    // type to the target's. OUTPUTS for the revision's Sink, which archives a revision: it
    // takes no `content` (it writes the document's own text), so the walk never fires it;
    // `tests/revision.rs` fires it and reads what it serves. Every other endpoint is observed
    // by every check.
    let unprobed: Vec<(String, ikigai_conformance::Check)> = report
        .unprobed
        .iter()
        .map(|u| (u.endpoint.clone(), u.check))
        .collect();
    assert_eq!(
        unprobed,
        [
            (
                ikigai_knowledge::TRAIL_ID.to_string(),
                ikigai_conformance::Check::Bodies
            ),
            (
                ikigai_knowledge::DOCUMENT_ID.to_string(),
                ikigai_conformance::Check::Outputs
            ),
            (
                ikigai_knowledge::REVISION_ID.to_string(),
                ikigai_conformance::Check::Outputs
            )
        ],
        "{report}"
    );
    assert_eq!(
        report
            .checks
            .skipped()
            .map(|c| c.to_string())
            .collect::<Vec<_>>(),
        Vec::<String>::new(),
        "every check runs: {report}"
    );
}
