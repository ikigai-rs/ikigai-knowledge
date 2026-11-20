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

#[test]
fn conforms() {
    let scratch = Scratch::new("conformance");
    scratch.declare("example", LEDGER);
    let space = Arc::new(ikigai_knowledge::space());
    let kernel = kernel_over(space.clone(), &scratch);
    let report = Suite::new()
        .body_space(space)
        .own_all([ikigai_knowledge::SOURCE_ID])
        .inherit(
            "file",
            "ikigai-fs, the file mount a host binds at urn:knowledge:file:path:{path} over the \
             config home's knowledge directory; its own conformance test covers it",
        )
        .self_named_space("knowledge", ikigai_knowledge::space)
        .cacheable(ikigai_knowledge::SOURCE_ID)
        .fixture(Fixture::new(ikigai_knowledge::SOURCE_ID, Verb::Source).binding("name", "example"))
        .enforced_under(
            Fixture::new(ikigai_knowledge::SOURCE_ID, Verb::Source)
                .binding("name", "example")
                .capability(reader(&scratch, "another")),
        )
        .run_blocking(&kernel);
    eprintln!("{report}");
    report.assert_clean_with(
        &Expected::new()
            .walked([ikigai_knowledge::SOURCE_ID])
            .nothing_unprobed(),
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
