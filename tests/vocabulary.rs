//! The module's own vocabulary, `src/vocabulary.ttl`: every term under its namespace the crate
//! emits or names in `src/` is defined there, and the ontology carries the crate's version.

use std::collections::BTreeSet;

use ikigai_knowledge::{
    ABSTAINED, CURRENT_PASSAGE, DOCUMENT_BASE, MAX_CHARACTERS, REMOVED_CLAIMS, REVISION, TRUNCATED,
    VOCABULARY, VOCABULARY_NAMESPACE,
};
use oxrdf::{Literal, NamedNode, NamedOrBlankNode, Term, Triple};
use oxttl::TurtleParser;

/// Every Rust source of the crate, compiled in so the test opens no file. A new module adds
/// itself here.
const SOURCES: &[(&str, &str)] = &[
    ("src/lib.rs", include_str!("../src/lib.rs")),
    ("src/graph.rs", include_str!("../src/graph.rs")),
    ("src/trail.rs", include_str!("../src/trail.rs")),
    ("src/passage.rs", include_str!("../src/passage.rs")),
    ("src/document.rs", include_str!("../src/document.rs")),
    ("src/revision.rs", include_str!("../src/revision.rs")),
    ("src/draft.rs", include_str!("../src/draft.rs")),
    ("src/verdict.rs", include_str!("../src/verdict.rs")),
];

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";
const OWL_VERSION_INFO: &str = "http://www.w3.org/2002/07/owl#versionInfo";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

fn vocabulary() -> Vec<Triple> {
    TurtleParser::new()
        .for_slice(VOCABULARY.as_bytes())
        .collect::<Result<_, _>>()
        .expect("the vocabulary is well-formed Turtle")
}

fn objects(triples: &[Triple], subject: &str, predicate: &str) -> Vec<Term> {
    triples
        .iter()
        .filter(|t| {
            matches!(&t.subject, NamedOrBlankNode::NamedNode(s) if s.as_str() == subject)
                && t.predicate.as_str() == predicate
        })
        .map(|t| t.object.clone())
        .collect()
}

fn named(iri: &str) -> Term {
    Term::NamedNode(NamedNode::new(iri).expect("an IRI"))
}

/// The terms under the namespace a text names: `{namespace}{local}`, the local name ending at
/// the first character that cannot be in one. The bare namespace (or a `{term}` placeholder
/// after it) names no term.
fn named_terms(text: &str) -> BTreeSet<String> {
    text.match_indices(VOCABULARY_NAMESPACE)
        .filter_map(|(at, _)| {
            let rest = &text[at + VOCABULARY_NAMESPACE.len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(rest.len());
            (end > 0).then(|| format!("{VOCABULARY_NAMESPACE}{}", &rest[..end]))
        })
        .collect()
}

#[test]
fn the_vocabulary_is_an_ontology_at_the_crate_version() {
    let triples = vocabulary();
    let ontology = VOCABULARY_NAMESPACE.trim_end_matches('#');
    assert!(objects(&triples, ontology, RDF_TYPE).contains(&named(OWL_ONTOLOGY)));
    assert_eq!(
        objects(&triples, ontology, OWL_VERSION_INFO),
        [Term::Literal(Literal::new_simple_literal(env!(
            "CARGO_PKG_VERSION"
        )))],
        "owl:versionInfo moves with the crate version"
    );
    assert!(
        triples
            .iter()
            .all(|t| !matches!(t.subject, NamedOrBlankNode::BlankNode(_))
                && !matches!(t.object, Term::BlankNode(_))),
        "the vocabulary names every node"
    );
}

#[test]
fn truncated_is_a_boolean_property_with_a_label_and_a_comment() {
    let triples = vocabulary();
    assert_eq!(TRUNCATED, format!("{VOCABULARY_NAMESPACE}truncated"));
    assert!(objects(&triples, TRUNCATED, RDF_TYPE).contains(&named(RDF_PROPERTY)));
    assert_eq!(
        objects(&triples, TRUNCATED, RDFS_RANGE),
        [named(XSD_BOOLEAN)]
    );
    assert_eq!(objects(&triples, TRUNCATED, RDFS_LABEL).len(), 1);
    assert_eq!(objects(&triples, TRUNCATED, RDFS_COMMENT).len(), 1);
}

#[test]
fn the_trail_terms_are_properties_with_a_range_a_label_and_a_comment() {
    let triples = vocabulary();
    const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
    const XSD_POSITIVE: &str = "http://www.w3.org/2001/XMLSchema#positiveInteger";
    const RDFS_RESOURCE: &str = "http://www.w3.org/2000/01/rdf-schema#Resource";
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    for (term, range) in [
        (ABSTAINED, XSD_BOOLEAN),
        (REMOVED_CLAIMS, XSD_INTEGER),
        (MAX_CHARACTERS, XSD_POSITIVE),
        (DOCUMENT_BASE, RDFS_RESOURCE),
        (REVISION, XSD_STRING),
    ] {
        assert!(term.starts_with(VOCABULARY_NAMESPACE), "{term}");
        assert!(objects(&triples, term, RDF_TYPE).contains(&named(RDF_PROPERTY)));
        assert_eq!(
            objects(&triples, term, RDFS_RANGE),
            [named(range)],
            "{term}"
        );
        assert_eq!(objects(&triples, term, RDFS_LABEL).len(), 1, "{term}");
        assert_eq!(objects(&triples, term, RDFS_COMMENT).len(), 1, "{term}");
    }
}

#[test]
fn the_current_passage_is_a_property_with_a_label_and_a_comment() {
    let triples = vocabulary();
    assert!(CURRENT_PASSAGE.starts_with(VOCABULARY_NAMESPACE));
    assert!(objects(&triples, CURRENT_PASSAGE, RDF_TYPE).contains(&named(RDF_PROPERTY)));
    assert_eq!(objects(&triples, CURRENT_PASSAGE, RDFS_LABEL).len(), 1);
    assert_eq!(objects(&triples, CURRENT_PASSAGE, RDFS_COMMENT).len(), 1);
}

#[test]
fn every_term_the_crate_names_is_defined() {
    let triples = vocabulary();
    let mut named_anywhere = BTreeSet::new();
    for (path, text) in SOURCES {
        for term in named_terms(text) {
            assert!(
                !objects(&triples, &term, RDF_TYPE).is_empty(),
                "{path} names {term}, which src/vocabulary.ttl does not define"
            );
            named_anywhere.insert(term);
        }
    }
    assert!(
        named_anywhere.contains(TRUNCATED),
        "the scan sees the term the answers emit"
    );
}

#[test]
fn the_scan_catches_an_undefined_term() {
    let triples = vocabulary();
    let stray = named_terms(&format!("\"{VOCABULARY_NAMESPACE}notATerm\""));
    assert_eq!(
        stray.iter().map(String::as_str).collect::<Vec<_>>(),
        [format!("{VOCABULARY_NAMESPACE}notATerm").as_str()]
    );
    assert!(objects(&triples, stray.first().unwrap(), RDF_TYPE).is_empty());
}
