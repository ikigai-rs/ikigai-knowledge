//! `urn:knowledge:source:name:{name}`: a declaration in the config home, read through the
//! kernel from the file mount the host binds, in two faces, under the caller's capability.

mod common;

use common::*;
use ikigai_core::{Capability, Error};
use oxrdf::{NamedNode, Term};
use oxttl::TurtleParser;

const SOURCE: &str = "urn:knowledge:source:name:ledger";

#[test]
fn a_declared_source_answers_in_the_text_face_by_default() {
    let scratch = Scratch::new("text");
    scratch.declare("ledger", LEDGER);
    let repr = issue(
        &kernel(&scratch),
        source("ledger", None),
        &Capability::root(),
    )
    .expect("a declared source");
    assert_eq!(repr.repr_type.media_type, "text/plain");
    assert_eq!(
        text(&repr),
        [
            "source ledger: 3 statements",
            "<urn:knowledge:source:name:ledger>",
            "  <http://purl.org/dc/terms/title> \"The work ledger\"",
            "  <http://purl.org/dc/terms/source> <urn:iki:ledger:item>",
            "<urn:iki:ledger:item>",
            "  <http://www.w3.org/2000/01/rdf-schema#label> \"ledger items\"",
            "",
        ]
        .join("\n")
    );
    // `as=text/plain` is the same face.
    let asked = issue(
        &kernel(&scratch),
        source("ledger", Some("text/plain")),
        &Capability::root(),
    )
    .expect("the text face");
    assert_eq!(text(&asked), text(&repr));
}

#[test]
fn a_declared_source_answers_in_the_turtle_face_with_its_own_name_resolved() {
    let scratch = Scratch::new("turtle");
    scratch.declare("ledger", LEDGER);
    let repr = issue(
        &kernel(&scratch),
        source("ledger", Some("text/turtle")),
        &Capability::root(),
    )
    .expect("the Turtle face");
    assert_eq!(repr.repr_type.media_type, "text/turtle");
    let turtle = text(&repr);
    // It keeps the prefixes the file spelled.
    assert!(
        turtle.contains("@prefix dcterms: <http://purl.org/dc/terms/>"),
        "{turtle}"
    );
    // A consumer parses it with no base: every IRI is absolute.
    let triples: Vec<_> = TurtleParser::new()
        .for_slice(turtle.as_bytes())
        .collect::<Result<_, _>>()
        .expect("valid Turtle with no base");
    assert_eq!(triples.len(), 3);
    let title = triples
        .iter()
        .find(|t| t.predicate == NamedNode::new_unchecked("http://purl.org/dc/terms/title"))
        .expect("the title");
    assert_eq!(title.subject.to_string(), format!("<{SOURCE}>"));
    assert!(matches!(&title.object, Term::Literal(l) if l.value() == "The work ledger"));
}

#[test]
fn an_undeclared_source_is_not_found_naming_where_it_looked() {
    let scratch = Scratch::new("missing");
    let err = issue(
        &kernel(&scratch),
        source("absent", None),
        &Capability::root(),
    )
    .expect_err("nothing is declared");
    match err {
        Error::NotFound(why) => {
            assert!(why.contains("no source named `absent`"), "{why}");
            assert!(why.contains("knowledge/source/absent.ttl"), "{why}");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn a_kernel_with_no_file_mount_serves_no_source_and_says_why() {
    let kernel = ikigai_core::Kernel::new(std::sync::Arc::new(ikigai_knowledge::space()));
    let err = issue(&kernel, source("ledger", None), &Capability::root())
        .expect_err("no mount, no declaration");
    match err {
        Error::NotFound(why) => assert!(why.contains("binding a file module"), "{why}"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn installing_a_declaration_after_a_miss_is_seen() {
    let scratch = Scratch::new("install");
    let kernel = kernel(&scratch);
    assert!(matches!(
        issue(&kernel, source("ledger", None), &Capability::root()),
        Err(Error::NotFound(_))
    ));
    scratch.declare("ledger", LEDGER);
    // A miss is never cached, so the next read finds the file.
    let repr = issue(&kernel, source("ledger", None), &Capability::root())
        .expect("installed after the miss");
    assert!(text(&repr).starts_with("source ledger: 3 statements"));
}

#[test]
fn a_caller_without_the_source_grant_is_denied() {
    let scratch = Scratch::new("denied");
    scratch.declare("ledger", LEDGER);
    let kernel = kernel(&scratch);
    // No grants at all: the kernel's floor refuses, before the endpoint is entered.
    let none = Capability::scoped(Vec::<String>::new());
    assert!(matches!(
        issue(&kernel, source("ledger", None), &none),
        Err(Error::Denied(_))
    ));
    // A grant for ANOTHER source, and the file read: the module's own rule refuses.
    let other = reader(&scratch, "notes");
    match issue(&kernel, source("ledger", None), &other) {
        Err(Error::Denied(why)) => assert!(
            why.contains("urn:cap:knowledge:read:source:name:ledger"),
            "{why}"
        ),
        other => panic!("expected Denied, got {other:?}"),
    }
    // ...and refuses an undeclared name the same way, so a refusal says nothing about
    // whether a source exists.
    assert!(matches!(
        issue(&kernel, source("absent", None), &other),
        Err(Error::Denied(_))
    ));
}

#[test]
fn a_caller_with_the_source_grant_and_the_file_read_reads_it() {
    let scratch = Scratch::new("granted");
    scratch.declare("ledger", LEDGER);
    let repr = issue(
        &kernel(&scratch),
        source("ledger", None),
        &reader(&scratch, "ledger"),
    )
    .expect("the narrow grants are enough");
    assert!(text(&repr).starts_with("source ledger: 3 statements"));
}

#[test]
fn a_source_grant_without_the_file_read_is_denied_by_the_floor() {
    let scratch = Scratch::new("no-fs");
    scratch.declare("ledger", LEDGER);
    let only = Capability::scoped([ikigai_knowledge::source_scope("ledger")]);
    assert!(matches!(
        issue(&kernel(&scratch), source("ledger", None), &only),
        Err(Error::Denied(_))
    ));
}

#[test]
fn a_malformed_declaration_is_refused_naming_the_file() {
    let scratch = Scratch::new("malformed");
    scratch.declare("ledger", "<> <http://purl.org/dc/terms/title> \"unclosed .");
    match issue(
        &kernel(&scratch),
        source("ledger", None),
        &Capability::root(),
    ) {
        Err(Error::Endpoint(why)) => {
            assert!(why.contains("knowledge/source/ledger.ttl"), "{why}");
            assert!(why.contains("not valid Turtle"), "{why}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_blank_node_is_refused() {
    let scratch = Scratch::new("blank");
    scratch.declare(
        "ledger",
        "<> <http://purl.org/dc/terms/creator> [ <http://xmlns.com/foaf/0.1/name> \"A\" ] .",
    );
    match issue(
        &kernel(&scratch),
        source("ledger", None),
        &Capability::root(),
    ) {
        Err(Error::Endpoint(why)) => assert!(why.contains("blank node"), "{why}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_name_that_is_not_one_path_segment_is_an_invalid_argument() {
    let scratch = Scratch::new("name");
    for bad in ["..", ".hidden", "a%2Fb"] {
        match issue(&kernel(&scratch), source(bad, None), &Capability::root()) {
            Err(Error::InvalidArgument { name, .. }) => assert_eq!(name, "name"),
            other => panic!("`{bad}`: expected InvalidArgument, got {other:?}"),
        }
    }
}

#[test]
fn an_unknown_face_is_an_invalid_argument_naming_both_faces() {
    let scratch = Scratch::new("face");
    scratch.declare("ledger", LEDGER);
    match issue(
        &kernel(&scratch),
        source("ledger", Some("application/json")),
        &Capability::root(),
    ) {
        Err(Error::InvalidArgument { name, detail }) => {
            assert_eq!(name, "as");
            assert!(
                detail.contains("text/plain") && detail.contains("text/turtle"),
                "{detail}"
            );
        }
        other => panic!("expected InvalidArgument, got {other:?}"),
    }
}

#[test]
fn the_space_names_itself() {
    assert_eq!(
        ikigai_core::space_iri("knowledge").as_str(),
        ikigai_knowledge::SPACE_ID
    );
}

#[test]
fn the_text_face_is_plain_lines_with_no_terminal_control_sequences() {
    let scratch = Scratch::new("controls");
    // A title carrying an ANSI color sequence and a line break.
    scratch.declare(
        "ledger",
        "<> <http://purl.org/dc/terms/title> \"red \\u001B[31m text\\nsecond line\" .",
    );
    let body = text(
        &issue(
            &kernel(&scratch),
            source("ledger", None),
            &Capability::root(),
        )
        .expect("a declared source"),
    );
    assert!(
        body.chars().all(|c| c == '\n' || !c.is_control()),
        "{body:?}"
    );
    // Escaped as N-Triples writes them, so the statement stays on one line.
    assert!(
        body.contains(r#""red \u001B[31m text\nsecond line""#),
        "{body}"
    );
    assert_eq!(body.lines().count(), 3, "{body}");
}
