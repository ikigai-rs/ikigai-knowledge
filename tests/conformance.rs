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
//!   refused by the module's own rule, with a typed `Denied`.
//! - `space()` is self-named `urn:iki:space:knowledge`: it takes no parameters and reads
//!   nothing while it is built.

mod common;

use std::sync::Arc;

use common::*;
use ikigai_conformance::{Expected, Fixture, Suite};
use ikigai_core::Verb;

const OWN: [&str; 4] = [
    ikigai_knowledge::SOURCE_ID,
    ikigai_knowledge::GRAPH_ID,
    ikigai_knowledge::ENTITY_ID,
    ikigai_knowledge::NEIGHBORHOOD_ID,
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

#[test]
fn conforms() {
    let scratch = Scratch::new("conformance");
    scratch.declare("example", LEDGER);
    scratch.declare("ledger", LEDGER_SOURCE);
    scratch.declare_corpus("example", WORK);
    let space = Arc::new(ikigai_knowledge::space());
    let kernel = kernel_with_store_over(space.clone(), &scratch, seeded_store());
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
        .self_named_space("knowledge", ikigai_knowledge::space)
        // The work ledger's own vocabulary, which the entity and neighborhood faces carry
        // as is: the ledger's lift is the identity.
        .namespace("https://ikigai-rs.dev/ns/ledger#")
        .cacheable(ikigai_knowledge::SOURCE_ID)
        .cacheable(ikigai_knowledge::GRAPH_ID)
        .cacheable(ikigai_knowledge::ENTITY_ID)
        .cacheable(ikigai_knowledge::NEIGHBORHOOD_ID)
        .fixture(Fixture::new(ikigai_knowledge::SOURCE_ID, Verb::Source).binding("name", "example"))
        .fixture(Fixture::new(ikigai_knowledge::GRAPH_ID, Verb::Source).binding("c", "example"))
        .fixture(entity(ikigai_knowledge::ENTITY_ID))
        .fixture(hood(ikigai_knowledge::NEIGHBORHOOD_ID))
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
        .enforced_under(
            Fixture::new(ikigai_knowledge::SOURCE_ID, Verb::Source)
                .binding("name", "example")
                .capability(reader(&scratch, "another")),
        )
        .run_blocking(&kernel);
    eprintln!("{report}");
    report.assert_clean_with(&Expected::new().walked(OWN).nothing_unprobed());
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
