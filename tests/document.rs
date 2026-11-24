//! Documents named through a source: `urn:knowledge:document:source:{s}:id:{d}` is an alias
//! onto the resource the source's declaration says its documents live under, read under the
//! source's grant, and a document the chunk list and the passage take like any other, so a
//! changed file re-derives only its own passages under the alias too.

mod common;

use common::*;
use ikigai_core::{Capability, Error, Iri, Request, Verb};

const RULE: &str = "<> <https://ikigai-rs.dev/ns/knowledge#maxCharacters> 40 .\n";

/// A source whose documents are the files under the mount's `docs/` directory.
const DOCS: &str = "@prefix dcterms: <http://purl.org/dc/terms/> .
@prefix knowledge: <https://ikigai-rs.dev/ns/knowledge#> .
<> dcterms:title \"Our docs\" ;
   knowledge:documentBase <urn:knowledge:file:path:docs/> .
";

const A: &str = "# Alpha\n\nThe first file says one thing.\n\nAnd then a second paragraph.\n";
const B: &str = "# Beta\n\nThe second file says another.\n";

fn write(scratch: &Scratch, path: &str, text: &str) {
    let file = scratch.knowledge().join(path);
    std::fs::create_dir_all(file.parent().expect("a parent")).expect("the directory");
    std::fs::write(file, text).expect("write the file");
}

fn get(iri: &str) -> Request {
    Request::new(
        Verb::Source,
        Iri::parse(iri.to_string()).expect("a valid IRI"),
    )
}

fn alias(id: &str) -> String {
    ikigai_knowledge::document_iri("docs", id)
}

fn chunks(document: &str) -> Request {
    get(&format!(
        "{}{document}:chunking:prose",
        ikigai_knowledge::CHUNK_PREFIX
    ))
}

fn home() -> Scratch {
    let scratch = Scratch::new("document");
    scratch.declare("docs", DOCS);
    write(&scratch, "chunking/prose.ttl", RULE);
    write(&scratch, "docs/a.md", A);
    write(&scratch, "docs/b.md", B);
    scratch
}

/// The caller a host would hold: the source's grant, the rule's and the file read.
fn caller(scratch: &Scratch) -> Capability {
    Capability::scoped([
        ikigai_knowledge::source_scope("docs"),
        ikigai_knowledge::chunking_scope("prose"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
    ])
}

fn lines(repr: Result<ikigai_core::Representation, Error>) -> Vec<String> {
    text(&repr.expect("an answer"))
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn the_alias_names_the_document_its_source_declares() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let cap = caller(&scratch);
    assert_eq!(alias("a.md"), "urn:knowledge:document:source:docs:id:a.md");
    let document = issue(&kernel, get(&alias("a.md")), &cap).expect("the document");
    assert_eq!(text(&document), A);
    // A pass-through: the face is the target's, as the file module serves it.
    let file = issue(
        &kernel,
        get(&format!("{}docs/a.md", ikigai_knowledge::FILE_PREFIX)),
        &cap,
    )
    .expect("the file");
    assert_eq!(document.repr_type, file.repr_type);
    assert_eq!(document.repr_type.media_type, "text/markdown");
    // A nested id is a path under the base, as the base's own resource reads it.
    write(&scratch, "docs/guide/intro.md", "Intro.\n");
    let nested = issue(&kernel, get(&alias("guide/intro.md")), &cap).expect("a nested document");
    assert_eq!(text(&nested), "Intro.\n");
}

#[test]
fn an_aliased_document_has_passages_like_any_other() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let cap = caller(&scratch);
    let a = alias("a.md");
    let names = lines(issue(&kernel, chunks(&a), &cap));
    assert_eq!(
        names,
        [
            ikigai_knowledge::passage_iri(&a, 0, 39),
            ikigai_knowledge::passage_iri(&a, 41, 69),
        ]
    );
    let first = issue(&kernel, get(&names[0]), &cap).expect("the first passage");
    assert_eq!(text(&first), "# Alpha\n\nThe first file says one thing.");
}

#[test]
fn a_changed_file_re_derives_only_its_own_passages_under_the_alias() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let cap = caller(&scratch);
    let a = alias("a.md");
    let b = alias("b.md");
    let before_a = lines(issue(&kernel, chunks(&a), &cap));
    let before_b = lines(issue(&kernel, chunks(&b), &cap));
    let passage_a = get(&before_a[0]);
    let passage_b = get(&before_b[0]);
    issue(&kernel, passage_a.clone(), &cap).expect("a's passage");
    issue(&kernel, passage_b.clone(), &cap).expect("b's passage");
    for request in [
        get(&a),
        get(&b),
        chunks(&a),
        chunks(&b),
        passage_a.clone(),
        passage_b.clone(),
    ] {
        assert!(kernel.is_cached(&request, &cap), "{}", request.target);
    }

    // The FILE under a's alias changes, and its thread is cut, as a host's watcher cuts it.
    write(&scratch, "docs/a.md", "# Alpha\n\nRewritten.\n");
    kernel.cut(format!("{}docs/a.md", ikigai_knowledge::FILE_PREFIX).as_str());

    // Only a's answers are gone, through the alias; b's stay cached.
    assert!(!kernel.is_cached(&get(&a), &cap));
    assert!(!kernel.is_cached(&chunks(&a), &cap));
    assert!(!kernel.is_cached(&passage_a, &cap));
    assert!(kernel.is_cached(&get(&b), &cap));
    assert!(kernel.is_cached(&chunks(&b), &cap));
    assert!(kernel.is_cached(&passage_b, &cap));
    assert_eq!(
        lines(issue(&kernel, chunks(&a), &cap)),
        [ikigai_knowledge::passage_iri(&a, 0, 19)]
    );
    assert_eq!(lines(issue(&kernel, chunks(&b), &cap)), before_b);
}

#[test]
fn a_changed_declaration_re_derives_every_document_under_it() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let cap = caller(&scratch);
    write(&scratch, "elsewhere/a.md", "Elsewhere.\n");
    assert_eq!(
        text(&issue(&kernel, get(&alias("a.md")), &cap).expect("a")),
        A
    );
    scratch.declare(
        "docs",
        "<> <https://ikigai-rs.dev/ns/knowledge#documentBase> <urn:knowledge:file:path:elsewhere/> .\n",
    );
    kernel.cut(format!("{}source/docs.ttl", ikigai_knowledge::FILE_PREFIX).as_str());
    assert!(!kernel.is_cached(&get(&alias("a.md")), &cap));
    assert!(!kernel.is_cached(&chunks(&alias("a.md")), &cap));
    assert_eq!(
        text(&issue(&kernel, get(&alias("a.md")), &cap).expect("a")),
        "Elsewhere.\n"
    );
}

#[test]
fn a_caller_without_the_source_grant_is_denied() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let other = Capability::scoped([
        ikigai_knowledge::source_scope("another"),
        ikigai_knowledge::chunking_scope("prose"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
    ]);
    for request in [get(&alias("a.md")), chunks(&alias("a.md"))] {
        match issue(&kernel, request, &other) {
            Err(Error::Denied(why)) => assert!(
                why.contains("urn:cap:knowledge:read:source:name:docs"),
                "{why}"
            ),
            other => panic!("expected Denied, got {other:?}"),
        }
    }
}

#[test]
fn refusals_name_what_was_wrong() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let cap = caller(&scratch);
    // An id that climbs out of the base, or is not a plain path beneath it.
    for id in [
        "../source/docs.ttl",
        "/etc/passwd",
        "a//b.md",
        "./a.md",
        "a.md?x",
    ] {
        match issue(&kernel, get(&alias(id)), &cap) {
            Err(Error::InvalidArgument { name, .. }) => assert_eq!(name, "d", "{id}"),
            other => panic!("expected InvalidArgument for `{id}`, got {other:?}"),
        }
    }
    // A document the base does not hold.
    match issue(&kernel, get(&alias("absent.md")), &cap) {
        Err(Error::NotFound(why)) => {
            assert!(
                why.contains("urn:knowledge:file:path:docs/absent.md"),
                "{why}"
            )
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
    // A source not declared.
    let undeclared = Capability::scoped([
        ikigai_knowledge::source_scope("nowhere"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
    ]);
    let nowhere = get(&ikigai_knowledge::document_iri("nowhere", "a.md"));
    match issue(&kernel, nowhere, &undeclared) {
        Err(Error::NotFound(why)) => assert!(why.contains("knowledge/source/nowhere.ttl"), "{why}"),
        other => panic!("expected NotFound, got {other:?}"),
    }
    // A source that declares no document base.
    scratch.declare("ledger", LEDGER);
    let ledger = Capability::scoped([
        ikigai_knowledge::source_scope("ledger"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
    ]);
    match issue(
        &kernel,
        get(&ikigai_knowledge::document_iri("ledger", "a.md")),
        &ledger,
    ) {
        Err(Error::Endpoint(why)) => {
            assert!(why.contains("knowledge/source/ledger.ttl"), "{why}");
            assert!(why.contains("documentBase"), "{why}");
        }
        other => panic!("expected a refusal naming the file, got {other:?}"),
    }
}
