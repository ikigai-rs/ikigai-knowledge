//! The work ledger as the first source (ledger #3856): a corpus's graphs, an entity and a
//! bounded neighborhood, read through ikigai-store's narrow doors under published grants.
//!
//! Every refusal here is paired with the permitted read coming back WITH its statements:
//! empty is a legitimate answer, so a confinement wrong in the safe direction would
//! otherwise be invisible.

mod common;

use common::*;
use ikigai_core::{Capability, Error, Verb};
use ikigai_knowledge::{corpus_scope, ENTITY_PREFIX, GRAPH_PREFIX, NEIGHBORHOOD_PREFIX};
use ikigai_store::cap_read_graph;

fn setup(tag: &str) -> (Scratch, ikigai_core::Kernel) {
    let scratch = Scratch::new(tag);
    scratch.declare("ledger", LEDGER_SOURCE);
    scratch.declare_corpus("work", WORK);
    let kernel = kernel_with_store(&scratch, seeded_store());
    (scratch, kernel)
}

/// The narrow grants: the corpus, the files, and the store graphs named.
fn grants(scratch: &Scratch, graphs: &[&str]) -> Capability {
    let mut scopes = vec![
        corpus_scope("work"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
    ];
    scopes.extend(graphs.iter().map(|g| cap_read_graph(g)));
    Capability::scoped(scopes)
}

fn graphs(kernel: &ikigai_core::Kernel, cap: &Capability) -> Result<String, Error> {
    issue(
        kernel,
        request(Verb::Source, &format!("{GRAPH_PREFIX}work"), &[]),
        cap,
    )
    .map(|r| text(&r))
}

fn entity(kernel: &ikigai_core::Kernel, e: &str, cap: &Capability) -> Result<String, Error> {
    let iri = format!("{ENTITY_PREFIX}{e}");
    let args = [("corpus", "work"), ("as", "application/n-triples")];
    issue(kernel, request(Verb::Source, &iri, &args), cap).map(|r| text(&r))
}

fn neighborhood(
    kernel: &ikigai_core::Kernel,
    e: &str,
    depth: &str,
    nodes: Option<&str>,
    cap: &Capability,
) -> Result<String, Error> {
    let iri = format!("{NEIGHBORHOOD_PREFIX}{e}:depth:{depth}");
    let mut args = vec![("corpus", "work"), ("as", "application/n-triples")];
    if let Some(n) = nodes {
        args.push(("nodes", n));
    }
    issue(kernel, request(Verb::Source, &iri, &args), cap).map(|r| text(&r))
}

// ------------------------------------------------------------------------ the graph list

#[test]
fn root_lists_every_declared_graph_the_store_holds() {
    let (_s, kernel) = setup("graphs-root");
    // `empty` is declared but the store never created it, so it is not listed.
    assert_eq!(
        graphs(&kernel, &Capability::root()).unwrap(),
        format!("{ACME}\n{DEFAULT}\n")
    );
}

#[test]
fn a_narrow_caller_lists_only_the_graphs_it_may_read() {
    let (s, kernel) = setup("graphs-narrow");
    assert_eq!(
        graphs(&kernel, &grants(&s, &[DEFAULT])).unwrap(),
        format!("{DEFAULT}\n")
    );
    // A grant for a declared graph the store does not hold lists nothing more than the
    // store already says.
    assert_eq!(
        graphs(&kernel, &grants(&s, &[DEFAULT, EMPTY])).unwrap(),
        format!("{DEFAULT}\n")
    );
}

#[test]
fn a_caller_without_the_corpus_grant_is_denied_alike_for_any_corpus() {
    let (s, kernel) = setup("graphs-denied");
    let cap = Capability::scoped([
        format!("urn:cap:fs:read:{}", s.knowledge().display()),
        cap_read_graph(DEFAULT),
        corpus_scope("other"),
    ]);
    let declared = graphs(&kernel, &cap).unwrap_err();
    let iri = format!("{GRAPH_PREFIX}nowhere");
    let undeclared = issue(&kernel, request(Verb::Source, &iri, &[]), &cap).unwrap_err();
    assert!(matches!(declared, Error::Denied(_)), "{declared:?}");
    assert!(matches!(undeclared, Error::Denied(_)), "{undeclared:?}");
    // Paired: the same caller, granted the corpus, lists its graph.
    assert_eq!(
        graphs(&kernel, &grants(&s, &[DEFAULT])).unwrap(),
        format!("{DEFAULT}\n")
    );
}

// ------------------------------------------------------------------------ the entity

#[test]
fn an_entity_is_its_statements_both_ways_closed_over_same_as() {
    let (s, kernel) = setup("entity");
    let got = entity(&kernel, ITEM1, &grants(&s, &[DEFAULT])).unwrap();
    assert!(
        got.contains(&format!(
            "<{ITEM1}> <http://purl.org/dc/terms/title> \"Wire the ledger\""
        )),
        "{got}"
    );
    assert!(
        got.contains(&format!(
            "<{ITEM1}> <https://ikigai-rs.dev/ns/ledger#blocks> <{ITEM2}>"
        )),
        "{got}"
    );
    // Closed over owl:sameAs: the alias's own statement comes with it.
    assert!(got.contains("\"Port the importer\""), "{got}");
    // Never the other ledger's statement naming it: the caller holds no token for acme.
    assert!(!got.contains(SECRET), "{got}");
    // Root reads both ledgers, so the acme link is there: the confinement is the grant.
    let all = entity(&kernel, ITEM1, &Capability::root()).unwrap();
    assert!(all.contains(SECRET), "{all}");
}

#[test]
fn a_ledger_without_its_read_token_gives_nothing_and_no_error() {
    let (s, kernel) = setup("entity-acme");
    // Granted acme only: the acme item comes back.
    let acme = entity(&kernel, SECRET, &grants(&s, &[ACME])).unwrap();
    assert!(acme.contains("\"Their Q4 migration\""), "{acme}");
    // Granted default only: the same item is exactly as empty as one that does not exist.
    let hidden = entity(&kernel, SECRET, &grants(&s, &[DEFAULT])).unwrap();
    let absent = entity(
        &kernel,
        "urn:iki:ledger:acme:item:404",
        &grants(&s, &[DEFAULT]),
    )
    .unwrap();
    assert_eq!(hidden, "");
    assert_eq!(hidden, absent);
    // No store grant at all: the kernel's floor refuses before the endpoint runs, the same
    // sentence for a real and a made-up entity (apart from the name the caller sent), and
    // it names the grant family, never a graph.
    let none = grants(&s, &[]);
    let made_up_iri = "urn:iki:ledger:acme:item:404";
    let real = format!("{:?}", entity(&kernel, SECRET, &none).unwrap_err());
    let made_up = format!("{:?}", entity(&kernel, made_up_iri, &none).unwrap_err());
    assert!(real.starts_with("Denied("), "{real}");
    assert_eq!(
        real.replace(SECRET, "{e}"),
        made_up.replace(made_up_iri, "{e}")
    );
    assert!(!real.contains(ACME), "{real}");
}

#[test]
fn an_entity_that_is_not_an_iri_is_refused() {
    let (s, kernel) = setup("entity-bad");
    let err = entity(&kernel, "notaniri", &grants(&s, &[DEFAULT])).unwrap_err();
    assert!(
        matches!(err, Error::InvalidArgument { ref name, .. } if name == "e"),
        "{err:?}"
    );
}

#[test]
fn the_turtle_face_is_the_default() {
    let (s, kernel) = setup("entity-turtle");
    let iri = format!("{ENTITY_PREFIX}{ITEM2}");
    let repr = issue(
        &kernel,
        request(Verb::Source, &iri, &[("corpus", "work")]),
        &grants(&s, &[DEFAULT]),
    )
    .unwrap();
    assert_eq!(repr.repr_type.media_type, "text/turtle");
    assert!(
        text(&repr).contains("\"Publish the store\""),
        "{}",
        text(&repr)
    );
}

// ------------------------------------------------------------------------ the neighborhood

#[test]
fn depth_bounds_the_walk() {
    let (s, kernel) = setup("hood-depth");
    let cap = grants(&s, &[DEFAULT]);
    let one = neighborhood(&kernel, ITEM1, "1", None, &cap).unwrap();
    assert!(
        one.contains(&format!(
            "<{ITEM1}> <https://ikigai-rs.dev/ns/ledger#blocks> <{ITEM2}>"
        )),
        "{one}"
    );
    assert!(!one.contains("\"Publish the store\""), "{one}");
    // Two hops reach item 2's statements, and its author; three reach item 3 through him.
    let two = neighborhood(&kernel, ITEM1, "2", None, &cap).unwrap();
    assert!(two.contains("\"Publish the store\""), "{two}");
    assert!(!two.contains(ITEM3), "{two}");
    let three = neighborhood(&kernel, ITEM1, "3", None, &cap).unwrap();
    assert!(
        three.contains(&format!(
            "<{ITEM3}> <https://ikigai-rs.dev/ns/ledger#author> <{BRIAN}>"
        )),
        "{three}"
    );
    // A class is not a neighbor: item 3 is never reached through `a ledger:Item`.
    assert!(!two.contains(ITEM3));
    // Never another ledger's statement.
    assert!(!three.contains(SECRET), "{three}");
    for bad in ["0", "4", "x"] {
        let err = neighborhood(&kernel, ITEM1, bad, None, &cap).unwrap_err();
        assert!(
            matches!(err, Error::InvalidArgument { ref name, .. } if name == "n"),
            "{err:?}"
        );
    }
}

#[test]
fn the_node_budget_bounds_the_walk() {
    let (s, kernel) = setup("hood-budget");
    let cap = grants(&s, &[DEFAULT]);
    let full = neighborhood(&kernel, ITEM1, "3", None, &cap).unwrap();
    // Two nodes: item 1 and one more.
    let small = neighborhood(&kernel, ITEM1, "3", Some("2"), &cap).unwrap();
    let mut nodes: Vec<&str> = small
        .split(['<', '>'])
        .filter(|w| {
            w.starts_with("urn:iki:ledger:default:")
                || w.starts_with("urn:kata:")
                || w.starts_with("urn:iki:person:")
        })
        .collect();
    nodes.sort();
    nodes.dedup();
    assert!(nodes.len() <= 2, "{nodes:?}\n{small}");
    assert!(small.len() < full.len(), "{small}\n{full}");
    // The cut is deterministic.
    assert_eq!(
        small,
        neighborhood(&kernel, ITEM1, "3", Some("2"), &cap).unwrap()
    );
    let err = neighborhood(&kernel, ITEM1, "2", Some("0"), &cap).unwrap_err();
    assert!(
        matches!(err, Error::InvalidArgument { ref name, .. } if name == "nodes"),
        "{err:?}"
    );
}

#[test]
fn a_neighborhood_without_the_ledger_token_is_empty() {
    let (s, kernel) = setup("hood-acme");
    let granted = neighborhood(&kernel, SECRET, "2", None, &grants(&s, &[ACME])).unwrap();
    assert!(granted.contains("\"Their Q4 migration\""), "{granted}");
    let hidden = neighborhood(&kernel, SECRET, "2", None, &grants(&s, &[DEFAULT])).unwrap();
    assert_eq!(hidden, "");
}
