//! Documents as sources: a chunking rule, a document's passages under it and one passage, each
//! read through the kernel and cached under the threads of what it read, so a changed file
//! re-derives only its own passages.

mod common;

use common::*;
use ikigai_core::{Capability, Error, Iri, Kernel, Request, Verb};

const RULE: &str = "@prefix knowledge: <https://ikigai-rs.dev/ns/knowledge#> .
<> knowledge:maxCharacters 40 .
";

const A: &str = "# Alpha\n\nThe first file says one thing.\n\nAnd then a second paragraph.\n";
const B: &str = "# Beta\n\nThe second file says another.\n";

fn write(scratch: &Scratch, path: &str, text: &str) {
    let file = scratch.knowledge().join(path);
    std::fs::create_dir_all(file.parent().expect("a parent")).expect("the directory");
    std::fs::write(file, text).expect("write the file");
}

fn doc(path: &str) -> String {
    format!("{}{path}", ikigai_knowledge::FILE_PREFIX)
}

fn get(iri: &str) -> Request {
    Request::new(
        Verb::Source,
        Iri::parse(iri.to_string()).expect("a valid IRI"),
    )
}

fn chunks(path: &str, rule: &str) -> Request {
    get(&format!(
        "{}{}:chunking:{rule}",
        ikigai_knowledge::CHUNK_PREFIX,
        doc(path)
    ))
}

fn home() -> Scratch {
    let scratch = Scratch::new("passage");
    write(&scratch, "chunking/prose.ttl", RULE);
    write(&scratch, "docs/a.md", A);
    write(&scratch, "docs/b.md", B);
    scratch
}

/// The caller a host would hold: the rule's grant and the file read.
fn caller(scratch: &Scratch) -> Capability {
    Capability::scoped([
        ikigai_knowledge::chunking_scope("prose"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
    ])
}

fn lines(kernel: &Kernel, request: Request, cap: &Capability) -> Vec<String> {
    text(&issue(kernel, request, cap).expect("an answer"))
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn the_rule_is_a_resource_read_by_name() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let rule = get("urn:knowledge:chunking:name:prose");
    assert_eq!(
        text(&issue(&kernel, rule, &caller(&scratch)).expect("the rule")),
        "chunking prose: paragraphs packed into passages of at most 40 characters\n"
    );
}

#[test]
fn a_document_is_cut_into_passages_by_the_rule_and_each_passage_is_its_text() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let cap = caller(&scratch);
    let names = lines(&kernel, chunks("docs/a.md", "prose"), &cap);
    let a = doc("docs/a.md");
    assert_eq!(
        names,
        [
            ikigai_knowledge::passage_iri(&a, 0, 39),
            ikigai_knowledge::passage_iri(&a, 41, 69),
        ]
    );
    // Each name is a passage the kernel serves: the document's characters in that range.
    let first = issue(&kernel, get(&names[0]), &cap).expect("the first passage");
    assert_eq!(first.repr_type.media_type, "text/plain");
    assert_eq!(text(&first), "# Alpha\n\nThe first file says one thing.");
    let second = issue(&kernel, get(&names[1]), &cap).expect("the second passage");
    assert_eq!(text(&second), "And then a second paragraph.");
}

#[test]
fn a_changed_file_re_derives_only_its_own_passages() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let cap = caller(&scratch);
    let a = doc("docs/a.md");
    let b = doc("docs/b.md");
    let before_a = lines(&kernel, chunks("docs/a.md", "prose"), &cap);
    let before_b = lines(&kernel, chunks("docs/b.md", "prose"), &cap);
    let passage_b = get(&before_b[0]);
    issue(&kernel, passage_b.clone(), &cap).expect("b's passage");
    let passage_a = get(&before_a[0]);
    issue(&kernel, passage_a.clone(), &cap).expect("a's passage");
    for request in [
        chunks("docs/a.md", "prose"),
        chunks("docs/b.md", "prose"),
        passage_a.clone(),
        passage_b.clone(),
    ] {
        assert!(kernel.is_cached(&request, &cap), "{}", request.target);
    }

    // One file changes, and its thread is cut, as a host's watcher over the mount cuts it.
    write(&scratch, "docs/a.md", "# Alpha\n\nRewritten.\n");
    kernel.cut(a.as_str());

    // Only a's answers are gone; b's stay cached.
    assert!(!kernel.is_cached(&chunks("docs/a.md", "prose"), &cap));
    assert!(!kernel.is_cached(&passage_a, &cap));
    assert!(kernel.is_cached(&chunks("docs/b.md", "prose"), &cap));
    assert!(kernel.is_cached(&passage_b, &cap));

    // a re-derives from its new text; b answers as before, from the cache.
    assert_eq!(
        lines(&kernel, chunks("docs/a.md", "prose"), &cap),
        [ikigai_knowledge::passage_iri(&a, 0, 19)]
    );
    assert_eq!(lines(&kernel, chunks("docs/b.md", "prose"), &cap), before_b);
    assert_eq!(before_b, [ikigai_knowledge::passage_iri(&b, 0, 37)]);
}

#[test]
fn a_changed_rule_re_derives_every_document_under_it() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let cap = caller(&scratch);
    lines(&kernel, chunks("docs/a.md", "prose"), &cap);
    lines(&kernel, chunks("docs/b.md", "prose"), &cap);
    write(
        &scratch,
        "chunking/prose.ttl",
        "<> <https://ikigai-rs.dev/ns/knowledge#maxCharacters> 1000 .\n",
    );
    kernel.cut(doc("chunking/prose.ttl").as_str());
    assert!(!kernel.is_cached(&chunks("docs/a.md", "prose"), &cap));
    assert!(!kernel.is_cached(&chunks("docs/b.md", "prose"), &cap));
    assert_eq!(
        lines(&kernel, chunks("docs/a.md", "prose"), &cap),
        [ikigai_knowledge::passage_iri(&doc("docs/a.md"), 0, 69)]
    );
}

#[test]
fn a_caller_without_the_rule_grant_is_denied() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let other = Capability::scoped([
        ikigai_knowledge::chunking_scope("another"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
    ]);
    match issue(&kernel, chunks("docs/a.md", "prose"), &other) {
        Err(Error::Denied(why)) => assert!(
            why.contains("urn:cap:knowledge:read:chunking:name:prose"),
            "{why}"
        ),
        other => panic!("expected Denied, got {other:?}"),
    }
}

#[test]
fn refusals_name_what_was_wrong() {
    let scratch = home();
    let kernel = kernel(&scratch);
    let cap = caller(&scratch);
    // A document outside the knowledge mount.
    let outside = get("urn:knowledge:chunk:document:urn:file:etc/passwd:chunking:prose");
    assert!(matches!(
        issue(&kernel, outside, &cap),
        Err(Error::InvalidArgument { name, .. }) if name == "d"
    ));
    // A path that climbs out.
    let climb = chunks("../secret.md", "prose");
    assert!(matches!(
        issue(&kernel, climb, &cap),
        Err(Error::InvalidArgument { name, .. }) if name == "d"
    ));
    // A range past the text.
    let past = get(&ikigai_knowledge::passage_iri(&doc("docs/b.md"), 0, 500));
    match issue(&kernel, past, &cap) {
        Err(Error::NotFound(why)) => assert!(why.contains("38 characters"), "{why}"),
        other => panic!("expected NotFound, got {other:?}"),
    }
    // An empty range.
    let empty = get(&ikigai_knowledge::passage_iri(&doc("docs/b.md"), 5, 5));
    assert!(matches!(
        issue(&kernel, empty, &cap),
        Err(Error::InvalidArgument { name, .. }) if name == "end"
    ));
    // A document that is not there.
    match issue(&kernel, chunks("docs/absent.md", "prose"), &cap) {
        Err(Error::NotFound(why)) => assert!(why.contains("docs/absent.md"), "{why}"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

/// A host's file mount that serves any path, as the crate allows: `ikigai-fs` refuses a `:` in
/// a path, but which module answers at the file template is the host's choice.
fn kernel_over_any_path() -> Kernel {
    use ikigai_core::{
        Description, EndpointSpace, Fallback, FnEndpoint, ReprType, Representation, Space,
        UriTemplate,
    };
    use std::sync::Arc;
    let file = FnEndpoint::new("file", |inv| {
        let body = match inv.bindings.get("path") {
            Some("chunking/prose.ttl") => RULE,
            _ => "abcdefghij\n",
        };
        Ok(Representation::new(
            ReprType::new("text/plain"),
            body.as_bytes().to_vec(),
        ))
    })
    .with_description(
        Description::new("file")
            .verb(Verb::Source)
            .output("text/plain"),
    );
    let mount = EndpointSpace::new().bind(
        UriTemplate::parse(ikigai_knowledge::FILE_TEMPLATE).unwrap(),
        file,
    );
    let spaces: Vec<Arc<dyn Space>> = vec![Arc::new(ikigai_knowledge::space()), Arc::new(mount)];
    Kernel::new(Arc::new(Fallback::new(spaces)))
}

#[test]
fn a_document_whose_passage_names_would_not_read_back_is_refused() {
    let kernel = kernel_over_any_path();
    let cap = Capability::root();
    // The mount serves an ordinary name, and every name the list gives reads back.
    let names = lines(&kernel, chunks("docs/x", "prose"), &cap);
    assert_eq!(
        names,
        [ikigai_knowledge::passage_iri(&doc("docs/x"), 0, 10)]
    );
    for path in ["docs/x:range:0-3", "docs/x:range"] {
        match issue(&kernel, chunks(path, "prose"), &cap) {
            Err(Error::InvalidArgument { name, detail }) => {
                assert_eq!(name, "d", "{path}: {detail}");
                assert!(detail.contains("rename the file"), "{detail}");
            }
            Ok(list) => {
                let listed: Vec<_> = text(&list)
                    .lines()
                    .map(|name| {
                        (
                            name.to_string(),
                            issue(&kernel, get(name), &cap).map(|r| text(&r)),
                        )
                    })
                    .collect();
                panic!("{path}: a list whose names do not read back as its passages: {listed:?}");
            }
            Err(other) => panic!("{path}: expected InvalidArgument naming d, got {other:?}"),
        }
    }
}
