//! A citation that stays put: a passage named by the document's revision and its range
//! resolves to the same words after the document changes, from the archive a Sink wrote
//! through the kernel, and names where those words are now. Its HTTP IRI under a
//! persistent-naming namespace is answered by ikigai-name with a 302 to it.

mod common;

use std::sync::Arc;

use common::*;
use ikigai_core::{
    ArgRef, Capability, Error, Exact, FnEndpoint, Invocation, Iri, Kernel, ReprType,
    Representation, Request, Verb,
};

const A: &str = "# Alpha\n\nThe first file says one thing.\n\nAnd then a second paragraph.\n";
/// A year later: a paragraph inserted above, and the cited words moved down.
const A_LATER: &str = "# Alpha\n\nA new opening paragraph, written later.\n\nThe first file says \
                       one thing.\n\nAnd then a second paragraph.\n";
/// Later still: the cited words are gone.
const A_REWRITTEN: &str = "# Alpha\n\nNothing of the old text remains.\n";

fn write(scratch: &Scratch, path: &str, text: &str) {
    let file = scratch.knowledge().join(path);
    std::fs::create_dir_all(file.parent().expect("a parent")).expect("the directory");
    std::fs::write(file, text).expect("write the file");
}

fn doc(path: &str) -> String {
    format!("{}{path}", ikigai_knowledge::FILE_PREFIX)
}

fn at(verb: Verb, iri: &str) -> Request {
    Request::new(verb, Iri::parse(iri.to_string()).expect("a valid IRI"))
}

fn get(iri: &str) -> Request {
    at(Verb::Source, iri)
}

fn turtle(iri: &str) -> Request {
    get(iri).with_arg("as", ArgRef::Inline(b"text/turtle".to_vec()))
}

/// A citer: reads the files and writes the archive.
fn citer(scratch: &Scratch) -> Capability {
    Capability::scoped([
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
        format!("urn:cap:fs:write:{}", scratch.knowledge().display()),
    ])
}

/// A reader a year later: reads the files, writes nothing.
fn later_reader(scratch: &Scratch) -> Capability {
    Capability::scoped([format!("urn:cap:fs:read:{}", scratch.knowledge().display())])
}

fn revision_iri(document: &str) -> String {
    format!("{}{document}", ikigai_knowledge::REVISION_PREFIX)
}

/// Cite "The first file says one thing." in docs/a.md: archive the revision, name the passage.
fn cite(kernel: &Kernel, scratch: &Scratch) -> (String, String) {
    let a = doc("docs/a.md");
    let archived = issue(kernel, at(Verb::Sink, &revision_iri(&a)), &citer(scratch))
        .expect("the revision is archived");
    let revision = text(&archived).trim().to_string();
    assert_eq!(revision, ikigai_knowledge::revision_of(A));
    let source =
        text(&issue(kernel, get(&revision_iri(&a)), &later_reader(scratch)).expect("the revision"));
    assert_eq!(
        source,
        format!("{revision}\n"),
        "Source answers the same revision"
    );
    (
        revision.clone(),
        ikigai_knowledge::passage_at_iri(&revision, &a, 9, 39),
    )
}

fn change(kernel: &Kernel, scratch: &Scratch, text: &str) {
    write(scratch, "docs/a.md", text);
    kernel.cut(doc("docs/a.md").as_str());
}

#[test]
fn a_range_name_moves_with_the_document_and_an_as_of_name_does_not() {
    let scratch = Scratch::new("revision");
    write(&scratch, "docs/a.md", A);
    let kernel = kernel(&scratch);
    let cap = later_reader(&scratch);
    let a = doc("docs/a.md");
    let plain = ikigai_knowledge::passage_iri(&a, 9, 39);
    let quoted = "The first file says one thing.";
    assert_eq!(
        text(&issue(&kernel, get(&plain), &cap).expect("now")),
        quoted
    );
    let (_, cited) = cite(&kernel, &scratch);
    assert_eq!(
        text(&issue(&kernel, get(&cited), &cap).expect("now")),
        quoted
    );

    change(&kernel, &scratch, A_LATER);

    // The defect: the range name now names other words.
    let moved = text(&issue(&kernel, get(&plain), &cap).expect("the range still exists"));
    assert_ne!(
        moved, quoted,
        "the range name silently points at different text"
    );
    // The as-of name still names the cited words, from the archive.
    assert_eq!(
        text(&issue(&kernel, get(&cited), &cap).expect("as of")),
        quoted
    );
}

#[test]
fn the_turtle_face_carries_both_selectors_and_the_current_passage_one_link_away() {
    let scratch = Scratch::new("revision-turtle");
    write(&scratch, "docs/a.md", A);
    let kernel = kernel(&scratch);
    let cap = later_reader(&scratch);
    let (revision, cited) = cite(&kernel, &scratch);
    change(&kernel, &scratch, A_LATER);

    let face = issue(&kernel, turtle(&cited), &cap).expect("the Turtle face");
    assert_eq!(face.repr_type.media_type, "text/turtle");
    let body = text(&face);
    let current = ikigai_knowledge::passage_iri(&doc("docs/a.md"), 50, 80);
    for want in [
        "oa:SpecificResource".to_string(),
        format!("oa:hasSource <{}>", doc("docs/a.md")),
        format!("knowledge:revision \"{revision}\""),
        "oa:TextQuoteSelector".to_string(),
        "oa:exact \"The first file says one thing.\"".to_string(),
        "oa:prefix \"# Alpha\\n\\n\"".to_string(),
        "oa:suffix \"\\n\\nAnd then a second paragraph.\\n\"".to_string(),
        "oa:TextPositionSelector".to_string(),
        "oa:start \"9\"^^".to_string(),
        "oa:end \"39\"^^".to_string(),
        format!("knowledge:currentPassage <{current}>"),
    ] {
        assert!(body.contains(&want), "{want} in:\n{body}");
    }
    // One link away: the current passage is the same words in the changed document.
    assert_eq!(
        text(&issue(&kernel, get(&current), &cap).expect("the current passage")),
        "The first file says one thing."
    );

    // Rewritten so the words are gone: the citation still resolves, with no current passage,
    // and its source is still the document.
    change(&kernel, &scratch, A_REWRITTEN);
    let body = text(&issue(&kernel, turtle(&cited), &cap).expect("as of"));
    assert!(body.contains("The first file says one thing."), "{body}");
    assert!(!body.contains("currentPassage"), "{body}");
}

#[test]
fn a_removed_document_still_answers_from_its_archive() {
    let scratch = Scratch::new("revision-removed");
    write(&scratch, "docs/a.md", A);
    let kernel = kernel(&scratch);
    let (_, cited) = cite(&kernel, &scratch);
    std::fs::remove_file(scratch.knowledge().join("docs/a.md")).expect("remove");
    kernel.cut(doc("docs/a.md").as_str());
    let cap = later_reader(&scratch);
    assert_eq!(
        text(&issue(&kernel, get(&cited), &cap).expect("as of")),
        "The first file says one thing."
    );
}

#[test]
fn an_unarchived_revision_is_not_found_naming_the_current_document() {
    let scratch = Scratch::new("revision-unarchived");
    write(&scratch, "docs/a.md", A);
    let kernel = kernel(&scratch);
    let cap = later_reader(&scratch);
    let a = doc("docs/a.md");
    let cited = ikigai_knowledge::passage_at_iri(&ikigai_knowledge::revision_of(A), &a, 9, 39);
    // Unchanged: served from the document itself, no archive needed.
    assert_eq!(
        text(&issue(&kernel, get(&cited), &cap).expect("as of")),
        "The first file says one thing."
    );
    change(&kernel, &scratch, A_LATER);
    match issue(&kernel, get(&cited), &cap) {
        Err(Error::NotFound(why)) => {
            assert!(why.contains("was not archived"), "{why}");
            assert!(why.contains(&a), "{why}");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn an_archive_that_is_not_its_revision_is_refused() {
    let scratch = Scratch::new("revision-tampered");
    write(&scratch, "docs/a.md", A);
    let kernel = kernel(&scratch);
    let (revision, cited) = cite(&kernel, &scratch);
    change(&kernel, &scratch, A_LATER);
    write(&scratch, &format!("revision/{revision}.txt"), A_LATER);
    kernel.cut(ikigai_knowledge::revision_file(&revision).as_str());
    match issue(&kernel, get(&cited), &later_reader(&scratch)) {
        Err(Error::Endpoint(why)) => assert!(why.contains("holds other text"), "{why}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn archiving_takes_the_write_and_never_overwrites() {
    let scratch = Scratch::new("revision-write");
    write(&scratch, "docs/a.md", A);
    let kernel = kernel(&scratch);
    let sink = at(Verb::Sink, &revision_iri(&doc("docs/a.md")));
    assert!(matches!(
        issue(&kernel, sink.clone(), &later_reader(&scratch)),
        Err(Error::Denied(_))
    ));
    let revision = ikigai_knowledge::revision_of(A);
    let archive = scratch.knowledge().join(format!("revision/{revision}.txt"));
    assert!(!archive.exists());
    issue(&kernel, sink.clone(), &citer(&scratch)).expect("archived");
    assert_eq!(std::fs::read_to_string(&archive).expect("the archive"), A);
    // Archiving the same revision again is not an error, and leaves the archive as it was.
    issue(&kernel, sink, &citer(&scratch)).expect("archived again");
    assert_eq!(std::fs::read_to_string(&archive).expect("the archive"), A);
}

#[test]
fn refusals_name_what_was_wrong() {
    let scratch = Scratch::new("revision-refusals");
    write(&scratch, "docs/a.md", A);
    let kernel = kernel(&scratch);
    let cap = later_reader(&scratch);
    let a = doc("docs/a.md");
    let r = ikigai_knowledge::revision_of(A);
    let bad = get(&ikigai_knowledge::passage_at_iri("ABC", &a, 0, 4));
    assert!(matches!(
        issue(&kernel, bad, &cap),
        Err(Error::InvalidArgument { name, .. }) if name == "r"
    ));
    let empty = get(&ikigai_knowledge::passage_at_iri(&r, &a, 4, 4));
    assert!(matches!(
        issue(&kernel, empty, &cap),
        Err(Error::InvalidArgument { name, .. }) if name == "end"
    ));
    match issue(
        &kernel,
        get(&ikigai_knowledge::passage_at_iri(&r, &a, 0, 500)),
        &cap,
    ) {
        Err(Error::NotFound(why)) => assert!(why.contains("70 characters"), "{why}"),
        other => panic!("expected NotFound, got {other:?}"),
    }
    let face = get(&ikigai_knowledge::passage_at_iri(&r, &a, 0, 4))
        .with_arg("as", ArgRef::Inline(b"application/json".to_vec()));
    assert!(matches!(
        issue(&kernel, face, &cap),
        Err(Error::InvalidArgument { name, .. }) if name == "as"
    ));
}

/// Where a host serves the knowledge kernel's resources over HTTP, in this test.
const KNOWLEDGE_BASE: &str = "https://knowledge.example/";

/// The operator's claim: `knowledge/passage/...` redirects, 302, to the as-of passage.
const REGISTRY: &str = r#"{"namespaces":[{
    "prefix": "knowledge", "owner": "urn:cap:name:admin:knowledge",
    "strategy": "redirect", "target": "https://knowledge.example/", "status": 302,
    "rules": [{
        "match": "passage/{revision}/{range}/{path*}",
        "target": "https://knowledge.example/urn:knowledge:passage:revision:{revision}:document:urn:knowledge:file:path:{path}:range:{range}"
    }]
}]}"#;

#[test]
fn the_persistent_iri_is_a_302_to_the_passage_as_of_its_revision() {
    let scratch = Scratch::new("revision-name");
    write(&scratch, "docs/a.md", A);
    let kernel = kernel(&scratch);
    let (revision, cited) = cite(&kernel, &scratch);
    let path = ikigai_knowledge::persistent_path(&revision, &doc("docs/a.md"), 9, 39)
        .expect("a file document has a persistent path");
    change(&kernel, &scratch, A_LATER);

    let registry = FnEndpoint::new("registry-source", |_: &Invocation<'_>| {
        Ok(Representation::new(
            ReprType::new("application/json"),
            REGISTRY.as_bytes().to_vec(),
        ))
    });
    let names = Kernel::new(Arc::new(
        ikigai_name::space().bind(Exact::new(ikigai_name::REGISTRY_IRI), registry),
    ));
    let ask = get("urn:name:document").with_arg(
        "path",
        ArgRef::Inline(format!("knowledge/{path}").into_bytes()),
    );
    let answer = issue(&names, ask, &Capability::root()).expect("the namespace answers");
    let redirect = answer.as_redirect().expect("a typed redirect");
    assert_eq!(redirect.status.code(), 302);
    let location = redirect.location.as_str();
    assert_eq!(location, format!("{KNOWLEDGE_BASE}{cited}"));

    // Followed, a year later: the exact cited words, though the document changed.
    let followed = location
        .strip_prefix(KNOWLEDGE_BASE)
        .expect("the host's base");
    assert_eq!(
        text(&issue(&kernel, get(followed), &later_reader(&scratch)).expect("as of")),
        "The first file says one thing."
    );
}
