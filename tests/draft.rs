//! Drafts and verdicts (ledger #3858): a model's draft over a passage is provisional,
//! provenanced, labeled inferred and checked against an ontology's SHACL shapes; a verdict is
//! a person's atom, refused to a model, kept apart from what is derived, and replayed over it.
//!
//! The model is a stub that answers a fixed Turtle reply and records its prompt. The SHACL
//! check is ikigai-shacl's own `urn:shacl:validate`, bound beside the module as a host binds
//! it, so which statements survive is decided by a real validator.

mod common;

use std::sync::{Arc, Mutex};

use common::*;
use ikigai_core::{Capability, Error, Fallback, Kernel, Space, Verb};
use ikigai_knowledge::{
    draft_iri, ontology_scope, passage_at_iri, passage_iri, persistent_path, prompt_scope,
    revision_of, verdict_iri, MemoryVerdictStore, VerdictStore, FILE_PREFIX, VERDICT_READ,
    VERDICT_WRITE,
};

const ALICE_PERSON: &str = "urn:example:person:alice";
const CLAUDE_AGENT: &str = "urn:example:agent:claude";
const LLM_GRANT: &str = "urn:cap:llm:use:anthropic";

const DOC: &str = "Brian decided to adopt oxigraph.\n\nThe migration is still open.\n";

const PROMPT: &str = "Extract each decision the passage states, as a schema:ChooseAction with \
its schema:agent and schema:name.";

const SHAPES: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix schema: <http://schema.org/> .

<#Decision> a sh:NodeShape ;
  sh:targetClass schema:ChooseAction ;
  sh:property [ sh:path schema:agent ; sh:minCount 1 ; sh:nodeKind sh:IRI ] .
";

const DECISION_IRI: &str = "urn:example:decision:oxigraph";

/// One decision that satisfies the shape, one that does not (no agent), and a statement with
/// a blank node.
const REPLY: &str = "```turtle
@prefix schema: <http://schema.org/> .
<urn:example:decision:oxigraph> a schema:ChooseAction ;
  schema:agent <urn:iki:person:brian> ;
  schema:name \"Adopt oxigraph\" .
<urn:example:decision:unattributed> a schema:ChooseAction .
_:someone schema:name \"a blank\" .
```";

/// A different reply, as a re-run model might give.
const OTHER_REPLY: &str = "<urn:example:decision:other> a <http://schema.org/ChooseAction> ;
  <http://schema.org/agent> <urn:iki:person:alice> .";

fn setup(tag: &str) -> Scratch {
    let scratch = Scratch::new(tag);
    let k = scratch.knowledge();
    for dir in ["docs", "prompt", "ontology"] {
        std::fs::create_dir_all(k.join(dir)).expect("a dir");
    }
    std::fs::write(k.join("docs/decision.md"), DOC).expect("the document");
    std::fs::write(k.join("prompt/extraction.txt"), PROMPT).expect("the prompt");
    std::fs::write(k.join("ontology/extraction.ttl"), SHAPES).expect("the ontology");
    scratch
}

/// The host's verdict store: Alice is a person; the agent is not.
fn verdicts() -> Arc<MemoryVerdictStore> {
    Arc::new(MemoryVerdictStore::new().with_person(ALICE_PERSON))
}

/// A host's kernel: the module, the file mount, the model and the SHACL validator, and a
/// store when the test needs a corpus, with a fresh verdict store.
fn host(
    scratch: &Scratch,
    reply: &str,
    store: Option<ikigai_store::DurableStore>,
) -> (Kernel, Arc<Mutex<Vec<String>>>) {
    host_with(scratch, reply, store, verdicts())
}

/// A host's kernel over the verdict store `kept`, which outlives any one kernel.
fn host_with(
    scratch: &Scratch,
    reply: &str,
    store: Option<ikigai_store::DurableStore>,
    kept: Arc<MemoryVerdictStore>,
) -> (Kernel, Arc<Mutex<Vec<String>>>) {
    let (llm, seen) = llm_stub(reply);
    let mut spaces: Vec<Arc<dyn Space>> = vec![
        Arc::new(ikigai_knowledge::space_with_verdicts(kept)),
        Arc::new(mount(scratch)),
        Arc::new(llm),
        Arc::new(ikigai_shacl::space()),
    ];
    if let Some(store) = store {
        spaces.push(Arc::new(ikigai_store::space(store)));
    }
    (Kernel::new(Arc::new(Fallback::new(spaces))), seen)
}

/// The first passage of the document: "Brian decided to adopt oxigraph."
fn passage() -> String {
    passage_iri(&format!("{FILE_PREFIX}docs/decision.md"), 0, 32)
}

fn draft() -> String {
    draft_iri(&passage(), "claude-sonnet")
}

/// What a person's session holds to read and draft.
fn reading(scratch: &Scratch) -> Vec<String> {
    vec![
        prompt_scope("extraction"),
        ontology_scope("extraction"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
        LLM_GRANT.to_string(),
        VERDICT_READ.to_string(),
    ]
}

/// A session for `principal` that holds every grant a verdict takes.
fn judge(scratch: &Scratch, principal: &str) -> Capability {
    let mut scopes = reading(scratch);
    // No file write: a person's verdict, and the text it cites, are kept by the host.
    scopes.push(VERDICT_WRITE.to_string());
    Capability::scoped(scopes)
        .with_principal(principal)
        .expect("a principal")
}

fn read_draft(kernel: &Kernel, face: Option<&str>, cap: &Capability) -> Result<String, Error> {
    let mut args = vec![];
    if let Some(face) = face {
        args.push(("as", face));
    }
    issue(kernel, request(Verb::Source, &draft(), &args), cap).map(|r| text(&r))
}

fn digest_of(text_face: &str) -> String {
    text_face
        .lines()
        .find_map(|l| l.strip_prefix("digest "))
        .expect("the text face states the digest")
        .to_string()
}

fn verdict(kernel: &Kernel, face: Option<&str>, cap: &Capability) -> Result<String, Error> {
    let mut args = vec![];
    if let Some(face) = face {
        args.push(("as", face));
    }
    issue(
        kernel,
        request(Verb::Source, &verdict_iri(&draft()), &args),
        cap,
    )
    .map(|r| text(&r))
}

fn judge_draft(kernel: &Kernel, args: &[(&str, &str)], cap: &Capability) -> Result<String, Error> {
    issue(
        kernel,
        request(Verb::Sink, &verdict_iri(&draft()), args),
        cap,
    )
    .map(|r| text(&r))
}

fn recorded(kept: &MemoryVerdictStore) -> bool {
    futures::executor::block_on(kept.read(&draft()))
        .expect("the store")
        .is_some()
}

#[test]
fn the_prompt_and_the_ontology_are_resources_under_their_own_grants() {
    let scratch = setup("prompt");
    let (kernel, _) = host(&scratch, REPLY, None);
    let cap = Capability::scoped(reading(&scratch));
    let prompt = issue(
        &kernel,
        request(Verb::Source, "urn:knowledge:prompt:name:extraction", &[]),
        &cap,
    )
    .expect("the prompt");
    assert_eq!(prompt.repr_type.media_type, "text/plain");
    assert_eq!(text(&prompt), PROMPT);
    let ontology = issue(
        &kernel,
        request(Verb::Source, "urn:knowledge:ontology:name:extraction", &[]),
        &cap,
    )
    .expect("the ontology");
    assert_eq!(ontology.repr_type.media_type, "text/turtle");
    // `<#Decision>` is resolved against the ontology's own name.
    assert!(
        text(&ontology).contains("<urn:knowledge:ontology:name:extraction#Decision>"),
        "{}",
        text(&ontology)
    );
    let other = Capability::scoped([
        prompt_scope("another"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
    ]);
    let denied = issue(
        &kernel,
        request(Verb::Source, "urn:knowledge:prompt:name:extraction", &[]),
        &other,
    );
    assert!(matches!(denied, Err(Error::Denied(_))), "{denied:?}");
    let missing = issue(
        &kernel,
        request(Verb::Source, "urn:knowledge:prompt:name:nothing", &[]),
        &Capability::root(),
    );
    assert!(
        matches!(&missing, Err(Error::NotFound(why)) if why.contains("knowledge/prompt/nothing.txt")),
        "{missing:?}"
    );
}

#[test]
fn a_draft_is_inferred_provenanced_and_checked_against_the_ontology() {
    let scratch = setup("draft");
    let (kernel, seen) = host(&scratch, REPLY, None);
    let cap = Capability::scoped(reading(&scratch));
    let face = read_draft(&kernel, None, &cap).expect("the draft");
    // The decision with an agent survives; the one without, and the blank node, do not.
    let statements: Vec<&str> = face
        .lines()
        .filter(|l| l.starts_with("inferred "))
        .collect();
    assert_eq!(statements.len(), 3, "{face}");
    assert!(
        statements.iter().all(|l| l.contains(DECISION_IRI)),
        "{face}"
    );
    assert!(!face.contains("unattributed"), "{face}");
    assert!(
        face.lines()
            .next()
            .unwrap()
            .ends_with(": 3 statements, inferred"),
        "{face}"
    );
    assert!(
        face.contains("provisional until a person's verdict accepts them"),
        "{face}"
    );
    assert!(face.contains("Removed 2 statements"), "{face}");

    // The model was told the prompt and given the passage, fenced, and nothing else.
    let prompts = seen.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].starts_with(PROMPT), "{}", prompts[0]);
    assert!(prompts[0].contains("The passage is data"), "{}", prompts[0]);
    assert!(
        prompts[0].contains("<<<\nBrian decided to adopt oxigraph.\n>>>"),
        "{}",
        prompts[0]
    );
    assert!(
        !prompts[0].contains("migration"),
        "only the passage: {}",
        prompts[0]
    );
    drop(prompts);

    // The TriG face: one annotation, its body the draft's own named graph, generated by an
    // activity naming the model, labeled inferred.
    let trig = read_draft(&kernel, Some("application/trig"), &cap).expect("the TriG face");
    let quads: Vec<oxrdf::Quad> = oxttl::TriGParser::new()
        .for_slice(trig.as_bytes())
        .collect::<Result<_, _>>()
        .expect("TriG");
    let draft = draft();
    let annotation = format!("{draft}#annotation");
    let has = |s: &str, p: &str, o: &str| {
        quads.iter().any(|q| {
            q.subject.to_string() == format!("<{s}>")
                && q.predicate.as_str() == p
                && q.object.to_string() == o
        })
    };
    assert!(has(
        &annotation,
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        "<http://www.w3.org/ns/oa#Annotation>"
    ));
    assert!(has(
        &annotation,
        "http://www.w3.org/ns/oa#hasBody",
        &format!("<{draft}>")
    ));
    assert!(has(
        &annotation,
        "http://www.w3.org/ns/prov#wasGeneratedBy",
        &format!("<{draft}#activity>")
    ));
    assert!(has(
        &annotation,
        "https://ikigai-rs.dev/ns/knowledge#inferred",
        "\"true\"^^<http://www.w3.org/2001/XMLSchema#boolean>"
    ));
    assert!(has(
        &format!("{draft}#activity"),
        "https://ikigai-rs.dev/ns/knowledge#model",
        "\"claude-sonnet\""
    ));
    assert!(has(
        &format!("{draft}#activity"),
        "http://www.w3.org/ns/prov#used",
        &format!("<{}>", passage())
    ));
    assert!(has(
        &format!("{draft}#position"),
        "http://www.w3.org/ns/oa#end",
        "\"32\"^^<http://www.w3.org/2001/XMLSchema#nonNegativeInteger>"
    ));
    let in_body = quads
        .iter()
        .filter(|q| q.graph_name.to_string() == format!("<{draft}>"))
        .count();
    assert_eq!(
        in_body, 3,
        "the drafted statements are in the draft's own graph: {trig}"
    );
    assert!(
        quads
            .iter()
            .filter(|q| q.graph_name.is_default_graph())
            .all(|q| !q.subject.to_string().contains(DECISION_IRI)),
        "no drafted statement is outside its graph: {trig}"
    );
}

#[test]
fn a_draft_refuses_a_caller_without_the_prompt_or_the_ontology_before_the_model() {
    let scratch = setup("draft-denied");
    let (kernel, seen) = host(&scratch, REPLY, None);
    let cap = Capability::scoped([
        prompt_scope("extraction"),
        ontology_scope("another"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
        LLM_GRANT.to_string(),
    ]);
    let denied = read_draft(&kernel, None, &cap);
    assert!(
        matches!(&denied, Err(Error::Denied(why)) if why.contains("ontology")),
        "{denied:?}"
    );
    assert!(seen.lock().unwrap().is_empty(), "the model was never asked");
}

#[test]
fn a_draft_leaves_out_what_the_corpus_already_states() {
    let scratch = setup("draft-corpus");
    scratch.declare("ledger", LEDGER_SOURCE);
    scratch.declare_corpus("work", WORK);
    let reply =
        format!("{REPLY}\n<{ITEM2}> <https://ikigai-rs.dev/ns/ledger#author> <{BRIAN}> .\n")
            .replace("```turtle\n", "")
            .replace("```", "");
    let (kernel, _) = host(&scratch, &reply, Some(seeded_store()));
    let mut scopes = reading(&scratch);
    scopes.extend([
        ikigai_knowledge::corpus_scope("work"),
        ikigai_knowledge::source_scope("ledger"),
        format!("urn:cap:store:read:graph:{DEFAULT}"),
    ]);
    let cap = Capability::scoped(scopes);
    let without = issue(&kernel, request(Verb::Source, &draft(), &[]), &cap)
        .map(|r| text(&r))
        .expect("the draft");
    assert!(without.contains(ITEM2), "{without}");
    let with = issue(
        &kernel,
        request(Verb::Source, &draft(), &[("corpus", "work")]),
        &cap,
    )
    .map(|r| text(&r))
    .expect("the draft over the corpus");
    assert!(!with.contains(ITEM2), "the ledger already says it: {with}");
    assert!(
        with.contains("Left out 1 statement the corpus already states."),
        "{with}"
    );
}

#[test]
fn no_model_writes_a_verdict() {
    let scratch = setup("seal");
    let kept = verdicts();
    let (kernel, _) = host_with(&scratch, REPLY, None, kept.clone());
    let cap = Capability::scoped(reading(&scratch));
    let digest = digest_of(&read_draft(&kernel, None, &cap).expect("the draft"));
    let accept = [("decision", "accept"), ("digest", digest.as_str())];

    // A model's capability holding every grant a verdict takes, under its own principal.
    let model = judge(&scratch, CLAUDE_AGENT);
    let refused = judge_draft(&kernel, &accept, &model);
    assert!(
        matches!(&refused, Err(Error::Denied(why)) if why.contains("only a person writes a verdict")),
        "{refused:?}"
    );
    // A principal declared nothing at all.
    let stranger = judge(&scratch, "urn:example:person:mallory");
    assert!(matches!(
        judge_draft(&kernel, &accept, &stranger),
        Err(Error::Denied(_))
    ));
    // Every grant but no principal: the capability names nobody.
    let mut scopes = reading(&scratch);
    // No file write: a person's verdict, and the text it cites, are kept by the host.
    scopes.push(VERDICT_WRITE.to_string());
    let anonymous = Capability::scoped(scopes);
    assert!(matches!(
        judge_draft(&kernel, &accept, &anonymous),
        Err(Error::Denied(why)) if why.contains("names nobody")
    ));
    // Root is the host's authority, not a person's.
    assert!(matches!(
        judge_draft(&kernel, &accept, &Capability::root()),
        Err(Error::Denied(_))
    ));
    assert!(!recorded(&kept), "nothing was written");

    // The person writes it.
    let written = judge_draft(&kernel, &accept, &judge(&scratch, ALICE_PERSON)).expect("Alice's");
    assert!(
        written.starts_with(&format!(
            "verdict on {}: accepted by <{ALICE_PERSON}>",
            draft()
        )),
        "{written}"
    );
    assert!(recorded(&kept));
}

#[test]
fn a_person_judges_only_the_draft_they_read() {
    let scratch = setup("stale");
    let kept = verdicts();
    let (kernel, _) = host_with(&scratch, REPLY, None, kept.clone());
    let alice = judge(&scratch, ALICE_PERSON);
    let stale = judge_draft(
        &kernel,
        &[("decision", "accept"), ("digest", "0123")],
        &alice,
    );
    assert!(
        matches!(&stale, Err(Error::Conflict(why)) if why.contains("read it again")),
        "{stale:?}"
    );
    assert!(!recorded(&kept));
}

#[test]
fn a_verdict_survives_a_rebuild_and_is_orphaned_when_its_passage_is_gone() {
    let scratch = setup("rebuild");
    let alice = judge(&scratch, ALICE_PERSON);
    // The host's store outlives every kernel: a rebuild starts a fresh kernel and cache.
    let kept = verdicts();
    let digest = {
        let (kernel, _) = host_with(&scratch, REPLY, None, kept.clone());
        let digest = digest_of(&read_draft(&kernel, None, &alice).expect("the draft"));
        judge_draft(
            &kernel,
            &[
                ("decision", "accept"),
                ("digest", digest.as_str()),
                ("note", "Brian said so on the item"),
            ],
            &alice,
        )
        .expect("accepted");
        digest
    };

    // Rebuild from scratch: a fresh kernel and cache, and a model that now drafts something
    // else. The person's verdict still holds, on what they accepted.
    let (kernel, _) = host_with(&scratch, OTHER_REPLY, None, kept.clone());
    let rerun = read_draft(&kernel, None, &alice).expect("the re-run draft");
    assert_ne!(digest_of(&rerun), digest, "the model changed its mind");
    assert!(
        rerun.contains("inferred <urn:example:decision:other>"),
        "{rerun}"
    );
    let replayed = verdict(&kernel, None, &alice).expect("the verdict");
    assert!(
        replayed.starts_with(&format!(
            "verdict on {}: accepted by <{ALICE_PERSON}>, holds\n",
            draft()
        )),
        "{replayed}"
    );
    assert!(
        replayed.contains("3 statements accepted by a person:"),
        "{replayed}"
    );
    assert!(replayed.contains(DECISION_IRI), "{replayed}");
    assert!(
        !replayed.contains("decision:other"),
        "never re-applied: {replayed}"
    );
    assert!(
        replayed.contains("note: Brian said so on the item"),
        "{replayed}"
    );
    let turtle = verdict(&kernel, Some("text/turtle"), &alice).expect("Turtle");
    assert!(!turtle.contains("orphaned"), "{turtle}");

    // A digest no draft was served under is refused; the old digest still names the draft
    // Alice read, as it was served, never the re-run.
    assert!(matches!(
        judge_draft(
            &kernel,
            &[("decision", "accept"), ("digest", "feedface")],
            &alice
        ),
        Err(Error::Conflict(_))
    ));

    // The document is rewritten: the passage now holds other text. The verdict is reported
    // orphaned, through the file's golden thread, never dropped.
    std::fs::write(
        scratch.knowledge().join("docs/decision.md"),
        "We have not decided anything yet, and will not soon.\n",
    )
    .expect("rewrite");
    let (kernel, _) = host_with(&scratch, OTHER_REPLY, None, kept.clone());
    let orphaned = verdict(&kernel, None, &alice).expect("still there");
    assert!(
        orphaned.contains(", orphaned: the passage") && orphaned.contains("now holds other text"),
        "{orphaned}"
    );
    let turtle = verdict(&kernel, Some("text/turtle"), &alice).expect("Turtle");
    assert!(turtle.contains("knowledge:orphaned true"), "{turtle}");

    // The document is cut short: the passage is gone.
    std::fs::write(scratch.knowledge().join("docs/decision.md"), "Short.\n").expect("rewrite");
    let (kernel, _) = host_with(&scratch, OTHER_REPLY, None, kept.clone());
    let gone = verdict(&kernel, None, &alice).expect("still there");
    assert!(gone.contains("is gone"), "{gone}");
}

#[test]
fn a_correction_supersedes_and_a_retraction_withdraws() {
    let scratch = setup("history");
    let (kernel, _) = host(&scratch, REPLY, None);
    let alice = judge(&scratch, ALICE_PERSON);
    let digest = digest_of(&read_draft(&kernel, None, &alice).expect("the draft"));
    judge_draft(
        &kernel,
        &[("decision", "reject"), ("digest", &digest)],
        &alice,
    )
    .expect("reject");
    let corrected = judge_draft(
        &kernel,
        &[
            ("decision", "correct"),
            ("digest", &digest),
            (
                "content",
                "<urn:example:decision:oxigraph> <http://schema.org/agent> <urn:iki:person:alice> .",
            ),
        ],
        &alice,
    )
    .expect("correct");
    assert!(corrected.contains("corrected by"), "{corrected}");
    let replayed = verdict(&kernel, None, &alice).expect("the verdict");
    assert!(
        replayed.contains("1 statement accepted by a person:"),
        "{replayed}"
    );
    assert!(replayed.contains("<urn:iki:person:alice>"), "{replayed}");
    assert!(replayed.contains("history: 2 acts"), "{replayed}");
    let turtle = verdict(&kernel, Some("text/turtle"), &alice).expect("Turtle");
    assert!(
        turtle.contains("<http://purl.org/dc/terms/replaces>"),
        "{turtle}"
    );

    let colored = judge_draft(
        &kernel,
        &[
            ("decision", "reject"),
            ("digest", &digest),
            ("note", "red \u{1b}[31m"),
        ],
        &alice,
    );
    assert!(
        matches!(&colored, Err(Error::InvalidArgument { name, .. }) if name == "note"),
        "{colored:?}"
    );
    judge_draft(&kernel, &[("decision", "retract")], &alice).expect("retract");
    let withdrawn = verdict(&kernel, None, &alice).expect("the verdict");
    assert!(withdrawn.contains("retracted by"), "{withdrawn}");
    assert!(withdrawn.contains("none in effect"), "{withdrawn}");
    assert!(withdrawn.contains("history: 3 acts"), "{withdrawn}");
    assert!(matches!(
        judge_draft(&kernel, &[("decision", "retract")], &alice),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn a_verdict_nobody_wrote_is_not_found() {
    let scratch = setup("none");
    let (kernel, _) = host(&scratch, REPLY, None);
    let missing = verdict(&kernel, None, &Capability::scoped(reading(&scratch)));
    assert!(
        matches!(&missing, Err(Error::NotFound(why)) if why.contains("no person has judged it")),
        "{missing:?}"
    );
}

/// Every scope a published action requires, over the module's draft and verdict doors and
/// the doors they read through.
fn published_grants(kernel: &Kernel) -> Vec<String> {
    let names = [
        verdict_iri(&draft()),
        draft(),
        passage(),
        "urn:knowledge:prompt:name:extraction".to_string(),
        "urn:knowledge:ontology:name:extraction".to_string(),
        "urn:knowledge:source:name:ledger".to_string(),
    ];
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let iri = ikigai_core::Iri::parse(&name).expect("an IRI");
        let described = kernel.describe(&iri).expect("described");
        out.extend(described.requires.iter().cloned());
        out.extend(
            described
                .actions
                .iter()
                .flat_map(|a| a.requires.iter().cloned()),
        );
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn the_published_grants_reach_no_person_list_and_no_verdict() {
    let scratch = setup("grants");
    let kept = verdicts();
    let (kernel, _) = host_with(&scratch, REPLY, None, kept.clone());
    let grants = published_grants(&kernel);
    assert!(grants.iter().any(|g| g == VERDICT_WRITE), "{grants:?}");
    assert!(
        !grants.iter().any(|g| g.starts_with("urn:cap:fs:write")),
        "no published action writes a file: {grants:?}"
    );
    // An agent principal holding every published grant, every grant the test host knows,
    // and a write over the whole config home besides.
    let mut scopes = grants.clone();
    scopes.extend(reading(&scratch));
    scopes.push(format!(
        "urn:cap:fs:write:{}",
        scratch.knowledge().display()
    ));
    let agent = Capability::scoped(scopes)
        .with_principal(CLAUDE_AGENT)
        .expect("a principal");
    // The old person file and the old verdict file are plain files now, nothing more: writing
    // them makes no one a person and records no verdict.
    std::fs::write(
        scratch.knowledge().join("person.ttl"),
        format!("<{CLAUDE_AGENT}> a <http://www.w3.org/ns/prov#Person> .\n"),
    )
    .expect("a file");
    let digest = digest_of(&read_draft(&kernel, None, &agent).expect("the draft"));
    let refused = judge_draft(
        &kernel,
        &[("decision", "accept"), ("digest", digest.as_str())],
        &agent,
    );
    assert!(
        matches!(&refused, Err(Error::Denied(why)) if why.contains("only a person writes a verdict")),
        "{refused:?}"
    );
    assert!(!recorded(&kept), "nothing was written");
    // A file the agent writes where a verdict once lived is no verdict either.
    let forged = issue(
        &kernel,
        request(
            Verb::Sink,
            &format!("{FILE_PREFIX}verdict/forged.ttl"),
            &[(
                "content",
                "<urn:knowledge:verdict:draft:x> a <https://ikigai-rs.dev/ns/knowledge#Verdict> .",
            )],
        ),
        &agent,
    );
    let _ = forged;
    assert!(matches!(
        verdict(&kernel, None, &agent),
        Err(Error::NotFound(_))
    ));
    assert!(!recorded(&kept));
}

#[test]
fn a_draft_read_with_a_corpus_is_judged_as_read() {
    let scratch = setup("corpus-verdict");
    scratch.declare("ledger", LEDGER_SOURCE);
    scratch.declare_corpus("work", WORK);
    let reply =
        format!("{REPLY}\n<{ITEM2}> <https://ikigai-rs.dev/ns/ledger#author> <{BRIAN}> .\n")
            .replace("```turtle\n", "")
            .replace("```", "");
    let kept = verdicts();
    let (kernel, _) = host_with(&scratch, &reply, Some(seeded_store()), kept.clone());
    let mut scopes = reading(&scratch);
    scopes.extend([
        ikigai_knowledge::corpus_scope("work"),
        ikigai_knowledge::source_scope("ledger"),
        format!("urn:cap:store:read:graph:{DEFAULT}"),
        VERDICT_WRITE.to_string(),
    ]);
    let alice = Capability::scoped(scopes)
        .with_principal(ALICE_PERSON)
        .expect("a principal");
    let plain = digest_of(&read_draft(&kernel, None, &alice).expect("the draft"));
    let filtered = issue(
        &kernel,
        request(Verb::Source, &draft(), &[("corpus", "work")]),
        &alice,
    )
    .map(|r| text(&r))
    .expect("the corpus draft");
    let filtered = digest_of(&filtered);
    assert_ne!(plain, filtered, "the corpus is part of the digest");
    judge_draft(
        &kernel,
        &[("decision", "accept"), ("digest", filtered.as_str())],
        &alice,
    )
    .expect("a corpus-filtered draft is accepted as read");
    let replayed = verdict(&kernel, None, &alice).expect("the verdict");
    assert!(replayed.contains("accepted by"), "{replayed}");
}

#[test]
fn judging_never_asks_the_model_again() {
    let scratch = setup("no-rerun");
    let (kernel, seen) = host(&scratch, REPLY, None);
    let alice = judge(&scratch, ALICE_PERSON);
    let digest = digest_of(&read_draft(&kernel, None, &alice).expect("the draft"));
    let asked = seen.lock().expect("seen").len();
    for decision in ["reject", "accept"] {
        judge_draft(
            &kernel,
            &[("decision", decision), ("digest", digest.as_str())],
            &alice,
        )
        .expect("an act");
    }
    assert_eq!(seen.lock().expect("seen").len(), asked, "no model call");
}

/// A store whose reads yield once, so two acts begun together both read before either writes.
struct Racing(MemoryVerdictStore);

#[async_trait::async_trait]
impl VerdictStore for Racing {
    async fn is_person(&self, principal: &str) -> ikigai_core::Result<bool> {
        self.0.is_person(principal).await
    }
    async fn read(&self, draft: &str) -> ikigai_core::Result<Option<(Vec<u8>, String)>> {
        let read = self.0.read(draft).await;
        Yield(false).await;
        read
    }
    async fn write(
        &self,
        draft: &str,
        expected: Option<&str>,
        body: Vec<u8>,
    ) -> ikigai_core::Result<()> {
        self.0.write(draft, expected, body).await
    }
    async fn keep_draft(&self, draft: ikigai_knowledge::StoredDraft) -> ikigai_core::Result<()> {
        self.0.keep_draft(draft).await
    }
    async fn draft(
        &self,
        digest: &str,
    ) -> ikigai_core::Result<Option<ikigai_knowledge::StoredDraft>> {
        self.0.draft(digest).await
    }
    async fn keep_revision(&self, revision: &str, text: &str) -> ikigai_core::Result<()> {
        self.0.keep_revision(revision, text).await
    }
    async fn revision(&self, revision: &str) -> ikigai_core::Result<Option<String>> {
        self.0.revision(revision).await
    }
}

struct Yield(bool);

impl std::future::Future for Yield {
    type Output = ();
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            std::task::Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }
}

#[test]
fn two_acts_at_once_never_drop_one() {
    let scratch = setup("race");
    let racing = Arc::new(Racing(MemoryVerdictStore::new().with_person(ALICE_PERSON)));
    let (llm, _) = llm_stub(REPLY);
    let spaces: Vec<Arc<dyn Space>> = vec![
        Arc::new(ikigai_knowledge::space_with_verdicts(racing.clone())),
        Arc::new(mount(&scratch)),
        Arc::new(llm),
        Arc::new(ikigai_shacl::space()),
    ];
    let kernel = Kernel::new(Arc::new(Fallback::new(spaces)));
    let alice = judge(&scratch, ALICE_PERSON);
    let digest = digest_of(&read_draft(&kernel, None, &alice).expect("the draft"));
    let act = |decision: &'static str| {
        let req = request(
            Verb::Sink,
            &verdict_iri(&draft()),
            &[("decision", decision), ("digest", digest.as_str())],
        );
        kernel.issue(req, &alice)
    };
    let (a, b) = futures::executor::block_on(futures::future::join(act("accept"), act("reject")));
    let outcomes = [a.is_ok(), b.is_ok()];
    assert_eq!(
        outcomes.iter().filter(|ok| **ok).count(),
        1,
        "one act lands: {a:?} {b:?}"
    );
    let lost = if a.is_ok() { b } else { a };
    assert!(
        matches!(&lost, Err(Error::Conflict(why)) if why.contains("recorded first")),
        "{lost:?}"
    );
    let replayed = verdict(&kernel, None, &alice).expect("the verdict");
    assert!(replayed.contains("history: 1 act"), "{replayed}");
}

/// Hub review point 5: a verdict names the passage as of its document's revision, so after
/// the document changes, the orphaned verdict still cites the exact text the person judged.
#[test]
fn a_verdict_cites_the_passage_as_of_the_revision_the_person_judged() {
    let scratch = setup("as-of");
    let alice = judge(&scratch, ALICE_PERSON);
    let kept = verdicts();
    let document = format!("{FILE_PREFIX}docs/decision.md");
    let judged = passage_at_iri(&revision_of(DOC), &document, 0, 32);
    {
        let (kernel, _) = host_with(&scratch, REPLY, None, kept.clone());
        let digest = digest_of(&read_draft(&kernel, None, &alice).expect("the draft"));
        judge_draft(
            &kernel,
            &[("decision", "accept"), ("digest", digest.as_str())],
            &alice,
        )
        .expect("accepted");
    }
    // Nothing archived a file: the person held no file write, and the revisions directory
    // was never written.
    assert!(!scratch.knowledge().join("revision").exists());

    // The document changes: the verdict is orphaned, and still cites the revision judged.
    std::fs::write(
        scratch.knowledge().join("docs/decision.md"),
        "We have not decided anything yet, and will not soon.\n",
    )
    .expect("rewrite");
    let (kernel, _) = host_with(&scratch, OTHER_REPLY, None, kept.clone());
    let orphaned = verdict(&kernel, None, &alice).expect("still there");
    assert!(orphaned.contains("orphaned"), "{orphaned}");
    assert!(
        orphaned.contains(&format!("judged passage <{judged}>")),
        "{orphaned}"
    );
    let turtle = verdict(&kernel, Some("text/turtle"), &alice).expect("Turtle");
    assert!(
        turtle.contains(&format!("knowledge:judgedPassage <{judged}>")),
        "{turtle}"
    );
    assert!(!turtle.contains("passageDigest"), "{turtle}");
    // The cited name still answers the exact words the person judged, from the host's
    // store, after the document changed; a reader needs only the file read.
    let reader = Capability::scoped(reading(&scratch));
    let words = issue(&kernel, request(Verb::Source, &judged, &[]), &reader).expect("as of");
    assert_eq!(text(&words), "Brian decided to adopt oxigraph.");
    // Its persistent HTTP path names the same revision and range.
    let path = persistent_path(&revision_of(DOC), &document, 0, 32).expect("a path");
    assert!(
        path.starts_with(&format!(
            "passage/revision/{}/range/0-32/path/",
            revision_of(DOC)
        )),
        "{path}"
    );
}
