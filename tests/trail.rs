//! The decision trail (ledger #3888): "who decided X, where, and what is still open" over the
//! work ledger, linked by label, drafted by a model from the CALLER's neighborhood only, and
//! checked claim by claim against the passages it cites.
//!
//! The model is a stub that answers a fixed draft and records its prompt: what the check
//! keeps and removes is this crate's, and what the model saw is the evidence of confinement.

mod common;

use common::*;
use ikigai_core::{Capability, Error, Kernel, Verb};
use ikigai_knowledge::{corpus_scope, source_scope, TRAIL_PREFIX, TRAIL_TEMPLATE};
use ikigai_store::cap_read_graph;

const ABSTAINED: &str = "<https://ikigai-rs.dev/ns/knowledge#abstained>";
/// The grant ikigai-llm takes at `urn:llm:ask`, for one provider.
const LLM_GRANT: &str = "urn:cap:llm:use:anthropic";
const REMOVED: &str = "<https://ikigai-rs.dev/ns/knowledge#removedClaims>";

fn setup(
    tag: &str,
    reply: &str,
) -> (
    Scratch,
    Kernel,
    std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) {
    let scratch = Scratch::new(tag);
    scratch.declare("ledger", LEDGER_SOURCE);
    scratch.declare_corpus("work", WORK);
    let (llm, seen) = llm_stub(reply);
    let kernel = trail_kernel(&scratch, llm);
    (scratch, kernel, seen)
}

fn grants(scratch: &Scratch, graphs: &[&str]) -> Capability {
    let mut scopes = vec![
        corpus_scope("work"),
        source_scope("ledger"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
        LLM_GRANT.to_string(),
    ];
    scopes.extend(graphs.iter().map(|g| cap_read_graph(g)));
    Capability::scoped(scopes)
}

fn trail(
    kernel: &Kernel,
    topic: &str,
    face: Option<&str>,
    cap: &Capability,
) -> Result<String, Error> {
    let iri = format!("{TRAIL_PREFIX}work");
    let mut args = vec![("topic", topic)];
    if let Some(face) = face {
        args.push(("as", face));
    }
    issue(kernel, request(Verb::Source, &iri, &args), cap).map(|r| text(&r))
}

/// A draft with one claim of each kind: grounded, cited to a passage outside the context,
/// quoting words the passage does not hold, and uncited.
fn mixed_draft() -> String {
    format!(
        "CLAIM: Brian decided to adopt oxigraph, on the item's first comment.\n\
         CITE: <{DECIDED}>\n\
         QUOTE: \"we adopt oxigraph, since it already backs ikigai-store\"\n\
         \n\
         CLAIM: The migration of old graphs is still open.\n\
         CITE: {STILL_OPEN}\n\
         QUOTE: the migration of old graphs is not scheduled\n\
         \n\
         CLAIM: Acme's budget decided it.\n\
         CITE: {ACME_COMMENT}\n\
         QUOTE: acme's private budget is 4M\n\
         \n\
         CLAIM: Alice approved the decision.\n\
         CITE: {STILL_OPEN}\n\
         QUOTE: Alice approved the move to oxigraph\n\
         \n\
         CLAIM: The decision was unanimous.\n"
    )
}

#[test]
fn grounded_claims_are_kept_with_their_citations_and_the_rest_removed() {
    let (s, kernel, _) = setup("trail-mixed", &mixed_draft());
    let got = trail(&kernel, "oxigraph", None, &grants(&s, &[DEFAULT])).unwrap();
    assert_eq!(
        got,
        format!(
            "decision trail: oxigraph\n\
             2 claims, each with the passage it rests on\n\
             1. Brian decided to adopt oxigraph, on the item's first comment.\n   \
                cited: {DECIDED}, characters 9 to 63\n   \
                quote: \"we adopt oxigraph, since it already backs ikigai-store\"\n\
             2. The migration of old graphs is still open.\n   \
                cited: {STILL_OPEN}, characters 12 to 56\n   \
                quote: \"the migration of old graphs is not scheduled\"\n\
             3 claims removed: no passage it cited supports it\n"
        )
    );
}

#[test]
fn the_turtle_face_is_web_annotations_with_quote_and_position_selectors() {
    let (s, kernel, _) = setup("trail-turtle", &mixed_draft());
    let got = trail(
        &kernel,
        "oxigraph",
        Some("text/turtle"),
        &grants(&s, &[DEFAULT]),
    )
    .unwrap();
    let triples: Vec<oxrdf::Triple> = oxttl::TurtleParser::new()
        .for_slice(got.as_bytes())
        .collect::<Result<_, _>>()
        .expect("valid Turtle");
    let nt: String = triples.iter().map(|t| format!("{t} .\n")).collect();
    let answer = format!("{TRAIL_PREFIX}work");
    let oa = "http://www.w3.org/ns/oa#";
    for line in [
        format!("<{answer}#claim-1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{oa}Annotation> ."),
        format!("<{answer}#claim-1> <{oa}hasTarget> <{answer}#claim-1-target> ."),
        format!("<{answer}#claim-1-target> <{oa}hasSource> <{DECIDED}> ."),
        format!("<{answer}#claim-1-quote> <{oa}exact> \"we adopt oxigraph, since it already backs ikigai-store\" ."),
        format!("<{answer}#claim-1-position> <{oa}start> \"9\"^^<http://www.w3.org/2001/XMLSchema#nonNegativeInteger> ."),
        format!("<{answer}#claim-2-target> <{oa}hasSource> <{STILL_OPEN}> ."),
        format!("<{answer}> {REMOVED} \"3\"^^<http://www.w3.org/2001/XMLSchema#integer> ."),
    ] {
        assert!(nt.contains(&line), "missing {line}\n{nt}");
    }
    assert!(!nt.contains("claim-3"), "{nt}");
    assert!(!nt.contains(ABSTAINED), "{nt}");
}

#[test]
fn the_model_sees_only_the_neighborhood_the_caller_may_read() {
    let (s, kernel, seen) = setup("trail-confined", &mixed_draft());
    trail(&kernel, "oxigraph", None, &grants(&s, &[DEFAULT])).unwrap();
    let prompts = seen.lock().unwrap().clone();
    assert_eq!(prompts.len(), 1);
    let prompt = &prompts[0];
    // The decision and both of its comments, with who wrote them.
    assert!(
        prompt.contains(&format!("[passage {DECISION}]")),
        "{prompt}"
    );
    assert!(prompt.contains(&format!("[passage {DECIDED}]")), "{prompt}");
    assert!(prompt.contains(&format!("author: {BRIAN}")), "{prompt}");
    assert!(prompt.contains("Still open: the migration"), "{prompt}");
    // Never acme's same-titled item: the caller holds no token for that ledger.
    assert!(!prompt.contains(ACME_DECISION), "{prompt}");
    assert!(!prompt.contains("private budget"), "{prompt}");
    // Paired: granted acme too, the model sees acme's item, so the confinement is the grant.
    let (s, kernel, seen) = setup("trail-confined-acme", &mixed_draft());
    trail(&kernel, "oxigraph", None, &grants(&s, &[DEFAULT, ACME])).unwrap();
    let both = seen.lock().unwrap()[0].clone();
    assert!(both.contains("private budget"), "{both}");
}

#[test]
fn a_claim_quoting_a_passage_the_caller_may_read_is_kept_once_granted() {
    let (s, kernel, _) = setup("trail-acme", &mixed_draft());
    let got = trail(&kernel, "oxigraph", None, &grants(&s, &[DEFAULT, ACME])).unwrap();
    assert!(
        got.starts_with("decision trail: oxigraph\n3 claims"),
        "{got}"
    );
    assert!(got.contains(&format!("cited: {ACME_COMMENT}")), "{got}");
    assert!(got.contains("2 claims removed"), "{got}");
}

#[test]
fn a_draft_with_no_grounded_claim_abstains() {
    let draft = format!(
        "CLAIM: Alice approved it.\nCITE: {STILL_OPEN}\nQUOTE: Alice approved it in a meeting\n\n\
         CLAIM: It was decided.\nCITE: {DECIDED}\nQUOTE: Decided\n\nNONE\n"
    );
    let (s, kernel, _) = setup("trail-abstain", &draft);
    let cap = grants(&s, &[DEFAULT]);
    assert_eq!(
        trail(&kernel, "oxigraph", None, &cap).unwrap(),
        "decision trail: oxigraph\n\
         no grounded answer: no claim of the draft was supported by a passage it cited\n\
         2 claims removed: no passage it cited supports it\n"
    );
    let turtle = trail(&kernel, "oxigraph", Some("text/turtle"), &cap).unwrap();
    assert!(turtle.contains("knowledge:abstained true"), "{turtle}");
    assert!(!turtle.contains("Annotation"), "{turtle}");
}

#[test]
fn a_topic_that_links_to_nothing_abstains_without_asking_the_model() {
    let (s, kernel, seen) = setup("trail-unlinked", &mixed_draft());
    let got = trail(&kernel, "kubernetes", None, &grants(&s, &[DEFAULT])).unwrap();
    assert_eq!(
        got,
        "decision trail: kubernetes\n\
         no grounded answer: nothing in the corpus you may read is labeled with it\n"
    );
    // Acme's item is labeled with it, and a caller without acme's token is told the same.
    let hidden = trail(&kernel, "at acme", None, &grants(&s, &[DEFAULT])).unwrap();
    assert_eq!(hidden.replace("at acme", "kubernetes"), got);
    assert!(seen.lock().unwrap().is_empty(), "the model was never asked");
}

#[test]
fn a_topic_is_matched_without_regard_to_case_and_as_a_literal() {
    let (s, kernel, seen) = setup("trail-case", &mixed_draft());
    let cap = grants(&s, &[DEFAULT]);
    let got = trail(&kernel, "OXIGRAPH as the STORE", None, &cap).unwrap();
    assert!(got.contains("2 claims"), "{got}");
    // A quote in the topic is data, never query syntax.
    let got = trail(&kernel, "x\" ) } } DROP ALL #", None, &cap).unwrap();
    assert!(got.contains("no grounded answer"), "{got}");
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn a_topic_that_is_empty_or_multiline_is_refused() {
    let (s, kernel, _) = setup("trail-bad", &mixed_draft());
    let cap = grants(&s, &[DEFAULT]);
    for bad in ["", "   ", "two\nlines"] {
        let err = trail(&kernel, bad, None, &cap).unwrap_err();
        assert!(
            matches!(err, Error::InvalidArgument { ref name, .. } if name == "topic"),
            "{err:?}"
        );
    }
    let err = trail(&kernel, "oxigraph", Some("application/json"), &cap).unwrap_err();
    assert!(
        matches!(err, Error::InvalidArgument { ref name, .. } if name == "as"),
        "{err:?}"
    );
}

#[test]
fn a_caller_without_the_corpus_grant_is_denied() {
    let (s, kernel, seen) = setup("trail-denied", &mixed_draft());
    let cap = Capability::scoped([
        corpus_scope("other"),
        source_scope("ledger"),
        format!("urn:cap:fs:read:{}", s.knowledge().display()),
        cap_read_graph(DEFAULT),
        LLM_GRANT.to_string(),
    ]);
    let err = trail(&kernel, "oxigraph", None, &cap).unwrap_err();
    assert!(matches!(err, Error::Denied(_)), "{err:?}");
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn a_control_character_in_a_claim_never_reaches_the_text_face() {
    let draft = format!(
        "CLAIM: Brian decided \u{1b}[31mit\u{1b}[0m.\nCITE: {DECIDED}\nQUOTE: we adopt oxigraph\n"
    );
    let (s, kernel, _) = setup("trail-control", &draft);
    let got = trail(&kernel, "oxigraph", None, &grants(&s, &[DEFAULT])).unwrap();
    assert!(!got.contains('\u{1b}'), "{got:?}");
    assert!(got.contains("1 claim removed"), "{got}");
}

#[test]
fn a_kernel_with_no_model_says_how_to_bind_one() {
    let scratch = Scratch::new("trail-nollm");
    scratch.declare("ledger", LEDGER_SOURCE);
    scratch.declare_corpus("work", WORK);
    let kernel = trail_kernel(&scratch, ikigai_core::EndpointSpace::new());
    let err = trail(&kernel, "oxigraph", None, &grants(&scratch, &[DEFAULT])).unwrap_err();
    assert!(
        matches!(err, Error::NotFound(ref why) if why.contains("urn:llm:ask") && why.contains("ikigai-llm")),
        "{err:?}"
    );
}

/// What `urn:kernel:actions` offers a capability, one endpoint template per line.
fn offered(kernel: &Kernel, cap: &Capability) -> Vec<String> {
    let got = issue(
        kernel,
        request(Verb::Source, "urn:kernel:actions", &[]),
        cap,
    )
    .map(|r| text(&r))
    .unwrap();
    got.lines().map(str::to_string).collect()
}

#[test]
fn the_trail_declares_the_model_grant_it_issues_under() {
    let (s, kernel, _) = setup("trail-declares", &mixed_draft());
    // The family form is satisfied by any one provider's grant: offered with it.
    let offer = offered(&kernel, &grants(&s, &[DEFAULT]));
    assert!(offer.iter().any(|l| l == TRAIL_TEMPLATE), "{offer:?}");
}

#[test]
fn a_caller_without_a_model_grant_is_denied_before_any_model_call_and_not_offered_the_trail() {
    let (s, kernel, seen) = setup("trail-nollm-grant", &mixed_draft());
    let cap = Capability::scoped([
        corpus_scope("work"),
        source_scope("ledger"),
        format!("urn:cap:fs:read:{}", s.knowledge().display()),
        cap_read_graph(DEFAULT),
    ]);
    let err = trail(&kernel, "oxigraph", None, &cap).unwrap_err();
    assert!(
        matches!(err, Error::Denied(ref why) if why.contains("urn:cap:llm:use:")),
        "{err:?}"
    );
    assert!(seen.lock().unwrap().is_empty(), "the model was never asked");
    // Not even the abstaining paths run: a topic that links to nothing is denied the same.
    let err = trail(&kernel, "kubernetes", None, &cap).unwrap_err();
    assert!(matches!(err, Error::Denied(_)), "{err:?}");
    // And the manifold does not offer the trail to that capability.
    // Paired: the same capability is still offered the neighborhood, which asks no model.
    let offer = offered(&kernel, &cap);
    assert!(!offer.iter().any(|l| l == TRAIL_TEMPLATE), "{offer:?}");
    assert!(
        offer
            .iter()
            .any(|l| l.starts_with("urn:knowledge:neighborhood:")),
        "{offer:?}"
    );
}
