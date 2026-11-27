//! A person's verdict on a model's draft: an ATOM, written only by a person, kept apart from
//! everything derived and replayed over it.
//!
//! - **`urn:knowledge:verdict:draft:{x}`** is the verdict on the draft `{x}`. A `Sink` records
//!   one act (accept, reject, correct or retract); a `Source` replays the acts over what is
//!   derived today.
//!
//! # Persons only
//!
//! A write takes `urn:cap:knowledge:write:verdict`, and the capability must NAME a principal
//! (`urn:cap:principal:<iri>`, which a door mints for the party it authenticated) that the
//! host's [`VerdictStore`] declares a person. Root names nobody and is refused, and so is a
//! principal the store does not declare a person (an agent, a model, a service): no model
//! writes a verdict, whatever grants its capability holds.
//!
//! The store is authority the HOST holds, handed to
//! [`space_with_verdicts`](crate::space_with_verdicts) when it builds the module: verdicts,
//! the person list and the drafts judged live there, and no request reaches them under the
//! caller's capability. No grant this module publishes, a file write included, can add a
//! person or write a verdict by hand.
//!
//! # Kept apart, replayed over what is derived
//!
//! The verdict lives in the host's store, never inside a draft or a derived graph, so
//! re-deriving passages or re-running a model never touches it. Each act records the person,
//! the decision, the digest of the draft the person read, the passage it judged as of its
//! document's revision (`urn:knowledge:passage:revision:{r}:document:{d}:range:{a}-{b}`,
//! `knowledge:judgedPassage`), and the statements it accepts (a correction's are the person's
//! own), so:
//!
//! - **A re-run model never re-applies a person's word.** An accept, reject or correction
//!   names the digest of the draft the person read, and judges the draft the store kept under
//!   that digest when it was served; no model is asked again. An unknown digest is refused
//!   (`Conflict`). Once recorded, the verdict holds the statements it
//!   accepted, whatever the model drafts later.
//! - **A rebuild never loses it.** Replaying reads the passage the draft was over: when its
//!   text is what the person judged, the verdict holds; when the passage is gone, or now
//!   holds other text, the verdict is reported ORPHANED (`knowledge:orphaned true`), never
//!   dropped and never applied to text nobody judged.
//! - **It cites what was judged.** The host's store keeps the document's text at each
//!   revision a draft was served over, so the passage the act names still answers the exact
//!   words the person judged after the document changes, an orphaned verdict's included.
//! - **No act is lost.** A write is conditional on the version the act was decided over;
//!   when another act landed first, the write is refused (`Conflict`), never dropped.
//! - **Supersession.** A later act supersedes the earlier ones (`dcterms:replaces`); every
//!   act stays in the file, so the history is readable. A retraction withdraws the verdict in
//!   effect.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ikigai_core::{
    ActionSpec, ArgSpec, Description, Endpoint, Error, Invocation, ReprType, Representation,
    Result, Verb,
};
use oxrdf::{Literal, NamedNode, NamedOrBlankNode, Term, Triple};
use oxttl::{TurtleParser, TurtleSerializer};

use crate::draft::{count, passage_name, split_draft, DRAFT_PREFIX};
use crate::graph::iri;
use crate::revision::revision_of;
use crate::XSD_STRING;

/// The IRI template a verdict is served at.
pub const VERDICT_TEMPLATE: &str = "urn:knowledge:verdict:draft:{x}";
/// The IRI prefix of [`VERDICT_TEMPLATE`]: the draft's name follows it.
pub const VERDICT_PREFIX: &str = "urn:knowledge:verdict:draft:";
/// The verdict endpoint's description id.
pub const VERDICT_ID: &str = "knowledge-verdict";
/// The scope that reads a verdict.
pub const VERDICT_READ: &str = "urn:cap:knowledge:read:verdict";
/// The scope that writes a verdict. Necessary, never sufficient: the capability must also
/// name a declared person.
pub const VERDICT_WRITE: &str = "urn:cap:knowledge:write:verdict";

/// On a verdict: the draft it judges.
pub const JUDGES: &str = "https://ikigai-rs.dev/ns/knowledge#judges";
/// On a verdict: one of its acts.
pub const ACT: &str = "https://ikigai-rs.dev/ns/knowledge#act";
/// On an act: its place in the verdict's history, from 1.
pub const SEQUENCE: &str = "https://ikigai-rs.dev/ns/knowledge#sequence";
/// On an act: what the person decided, one of the four decisions below.
pub const DECISION: &str = "https://ikigai-rs.dev/ns/knowledge#decision";
/// The decision that accepts a draft's statements as they are.
pub const ACCEPTED: &str = "https://ikigai-rs.dev/ns/knowledge#Accepted";
/// The decision that rejects a draft.
pub const REJECTED: &str = "https://ikigai-rs.dev/ns/knowledge#Rejected";
/// The decision that replaces a draft's statements with the person's own.
pub const CORRECTED: &str = "https://ikigai-rs.dev/ns/knowledge#Corrected";
/// The decision that withdraws the verdict in effect.
pub const RETRACTED: &str = "https://ikigai-rs.dev/ns/knowledge#Retracted";
/// On an act: the digest of the draft the person judged.
pub const DRAFT_DIGEST: &str = "https://ikigai-rs.dev/ns/knowledge#draftDigest";
/// On an act: the passage the person judged, as of its document's revision.
pub const JUDGED_PASSAGE: &str = "https://ikigai-rs.dev/ns/knowledge#judgedPassage";
/// On an act: the statements it accepts, as N-Triples text.
pub const STATEMENTS: &str = "https://ikigai-rs.dev/ns/knowledge#statements";
/// On a replayed verdict, with the value `true`: the passage it judged is gone or now holds
/// other text, so the verdict applies to nothing derived today.
pub const ORPHANED: &str = "https://ikigai-rs.dev/ns/knowledge#orphaned";

const PROV_ATTRIBUTED: &str = "http://www.w3.org/ns/prov#wasAttributedTo";
const DCTERMS_REPLACES: &str = "http://purl.org/dc/terms/replaces";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const VERDICT_CLASS: &str = "https://ikigai-rs.dev/ns/knowledge#Verdict";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const TEXT: &str = "text/plain";
const TURTLE: &str = "text/turtle";

/// The name of the verdict on the draft `draft`.
///
/// ```
/// assert_eq!(
///     ikigai_knowledge::verdict_iri("urn:knowledge:draft:passage:p:model:m"),
///     "urn:knowledge:verdict:draft:urn:knowledge:draft:passage:p:model:m"
/// );
/// ```
pub fn verdict_iri(draft: &str) -> String {
    format!("{VERDICT_PREFIX}{draft}")
}

/// A draft as it was served, kept so a person's verdict judges what they read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredDraft {
    /// The draft's name, `urn:knowledge:draft:passage:{p}:model:{m}`.
    pub draft: String,
    /// The digest the draft's faces state, which a verdict names.
    pub digest: String,
    /// The passage the model saw, as of its document's revision:
    /// `urn:knowledge:passage:revision:{r}:document:{d}:range:{start}-{end}`.
    pub passage: String,
    /// The statements it proposed, one N-Triples line each, sorted.
    pub statements: String,
    /// The draft's `oa:Annotation`, with its body in its own named graph, as TriG.
    pub annotation: Vec<u8>,
}

/// Where a host keeps verdicts, the person list and the drafts persons judge: authority the
/// HOST holds, never reached under a caller's capability.
///
/// A verdict is an opaque body with a version; [`write`](Self::write) is conditional on the
/// version the writer read (`None`: no verdict yet) and answers `Error::Conflict` when another
/// write landed first.
#[async_trait]
pub trait VerdictStore: Send + Sync {
    /// Whether the host declares `principal` a person.
    async fn is_person(&self, principal: &str) -> Result<bool>;
    /// The verdict on `draft` and its version, or `None` when no person has judged it.
    async fn read(&self, draft: &str) -> Result<Option<(Vec<u8>, String)>>;
    /// Write the verdict on `draft` if its version is still `expected`.
    async fn write(&self, draft: &str, expected: Option<&str>, body: Vec<u8>) -> Result<()>;
    /// Keep a draft as it was served.
    async fn keep_draft(&self, draft: StoredDraft) -> Result<()>;
    /// The draft kept under `digest`, if any.
    async fn draft(&self, digest: &str) -> Result<Option<StoredDraft>>;
    /// Keep a document's text at the revision `revision` a draft was served over, so the
    /// passage a verdict cites still answers after the document changes. Content-addressed:
    /// keeping the same revision twice keeps one.
    async fn keep_revision(&self, revision: &str, text: &str) -> Result<()>;
    /// The text kept at `revision`, if any. The module checks it against `revision` before
    /// serving it.
    async fn revision(&self, revision: &str) -> Result<Option<String>>;
}

/// A [`VerdictStore`] in memory: for tests, and for a host that keeps verdicts no longer than
/// it runs.
///
/// ```
/// let store =
///     ikigai_knowledge::MemoryVerdictStore::new().with_person("urn:example:person:alice");
/// # let _ = store;
/// ```
#[derive(Default)]
pub struct MemoryVerdictStore {
    persons: BTreeSet<String>,
    verdicts: Mutex<BTreeMap<String, Vec<u8>>>,
    drafts: Mutex<BTreeMap<String, StoredDraft>>,
    revisions: Mutex<BTreeMap<String, String>>,
}

impl MemoryVerdictStore {
    /// An empty store, declaring no one a person.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare `principal` a person.
    pub fn with_person(mut self, principal: &str) -> Self {
        self.persons.insert(principal.to_string());
        self
    }
}

fn version(body: &[u8]) -> String {
    blake3::hash(body).to_hex().to_string()
}

fn poisoned<T>(_: T) -> Error {
    Error::Endpoint("the verdict store's lock is poisoned".to_string())
}

#[async_trait]
impl VerdictStore for MemoryVerdictStore {
    async fn is_person(&self, principal: &str) -> Result<bool> {
        Ok(self.persons.contains(principal))
    }

    async fn read(&self, draft: &str) -> Result<Option<(Vec<u8>, String)>> {
        let held = self.verdicts.lock().map_err(poisoned)?;
        Ok(held.get(draft).map(|b| (b.clone(), version(b))))
    }

    async fn write(&self, draft: &str, expected: Option<&str>, body: Vec<u8>) -> Result<()> {
        let mut held = self.verdicts.lock().map_err(poisoned)?;
        let now = held.get(draft).map(|b| version(b));
        if now.as_deref() != expected {
            return Err(Error::Conflict(format!(
                "another act on `{draft}` was recorded first: read the verdict again, then decide"
            )));
        }
        held.insert(draft.to_string(), body);
        Ok(())
    }

    async fn keep_draft(&self, draft: StoredDraft) -> Result<()> {
        let mut held = self.drafts.lock().map_err(poisoned)?;
        held.insert(draft.digest.clone(), draft);
        Ok(())
    }

    async fn draft(&self, digest: &str) -> Result<Option<StoredDraft>> {
        let held = self.drafts.lock().map_err(poisoned)?;
        Ok(held.get(digest).cloned())
    }

    async fn keep_revision(&self, revision: &str, text: &str) -> Result<()> {
        if revision_of(text) != revision {
            return Err(Error::Endpoint(format!(
                "the text offered as revision {revision} has another digest, so it is not kept"
            )));
        }
        let mut held = self.revisions.lock().map_err(poisoned)?;
        held.entry(revision.to_string())
            .or_insert_with(|| text.to_string());
        Ok(())
    }

    async fn revision(&self, revision: &str) -> Result<Option<String>> {
        let held = self.revisions.lock().map_err(poisoned)?;
        Ok(held.get(revision).cloned())
    }
}

/// A decision a person can make.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Decision {
    Accept,
    Reject,
    Correct,
    Retract,
}

impl Decision {
    fn parse(word: &str) -> Result<Decision> {
        match word.trim() {
            "accept" => Ok(Decision::Accept),
            "reject" => Ok(Decision::Reject),
            "correct" => Ok(Decision::Correct),
            "retract" => Ok(Decision::Retract),
            other => Err(Error::InvalidArgument {
                name: "decision".to_string(),
                detail: format!(
                    "`{other}` is not a decision: use accept, reject, correct or retract"
                ),
            }),
        }
    }

    fn iri(self) -> &'static str {
        match self {
            Decision::Accept => ACCEPTED,
            Decision::Reject => REJECTED,
            Decision::Correct => CORRECTED,
            Decision::Retract => RETRACTED,
        }
    }

    fn from_iri(iri: &str) -> Option<Decision> {
        [
            Decision::Accept,
            Decision::Reject,
            Decision::Correct,
            Decision::Retract,
        ]
        .into_iter()
        .find(|d| d.iri() == iri)
    }

    fn past(self) -> &'static str {
        match self {
            Decision::Accept => "accepted",
            Decision::Reject => "rejected",
            Decision::Correct => "corrected",
            Decision::Retract => "retracted",
        }
    }
}

/// One recorded act.
struct Act {
    sequence: usize,
    person: String,
    decision: Decision,
    draft_digest: String,
    /// The passage judged, as of its document's revision.
    passage: String,
    statements: String,
    note: Option<String>,
}

/// A verdict as its file holds it.
struct Verdict {
    iri: String,
    draft: String,
    acts: Vec<Act>,
}

/// How a verdict replays over what is derived today.
enum Replay {
    Holds,
    Orphaned(String),
}

/// The draft `{x}` names, checked to be a draft over a passage.
fn draft_arg(inv: &Invocation<'_>) -> Result<String> {
    let target = inv.request.target.as_str();
    let x = target.strip_prefix(VERDICT_PREFIX).unwrap_or_default();
    let ok = split_draft(x).is_some_and(|(p, m)| passage_name("x", p).is_ok() && !m.is_empty());
    if ok {
        Ok(x.to_string())
    } else {
        Err(Error::InvalidArgument {
            name: "x".to_string(),
            detail: format!(
                "`{x}` is not a draft: name one as {DRAFT_PREFIX}{{passage}}:model:{{model}}"
            ),
        })
    }
}

/// Read the verdict on `draft` and its version, or `None` when no person has judged it.
async fn read_verdict(store: &dyn VerdictStore, draft: &str) -> Result<Option<(Verdict, String)>> {
    let verdict = verdict_iri(draft);
    let what = format!("verdict on `{draft}`");
    let Some((body, version)) = store.read(draft).await? else {
        return Ok(None);
    };
    let bad = || {
        Error::Endpoint(format!(
            "{what}: the store holds no verdict this module wrote"
        ))
    };
    let mut triples = Vec::new();
    for t in TurtleParser::new().for_slice(&body) {
        triples.push(t.map_err(|_| bad())?);
    }
    let mut acts: BTreeMap<String, Vec<&Triple>> = BTreeMap::new();
    for t in &triples {
        if let NamedOrBlankNode::NamedNode(s) = &t.subject {
            if s.as_str() != verdict {
                acts.entry(s.as_str().to_string()).or_default().push(t);
            }
        }
    }
    let mut out = Vec::new();
    for statements in acts.values() {
        let one = |p: &str| {
            statements
                .iter()
                .find(|t| t.predicate.as_str() == p)
                .map(|t| t.object.clone())
        };
        let value = |p: &str| match one(p) {
            Some(Term::Literal(l)) => Some(l.value().to_string()),
            _ => None,
        };
        let named = |p: &str| match one(p) {
            Some(Term::NamedNode(n)) => Some(n.as_str().to_string()),
            _ => None,
        };
        out.push(Act {
            sequence: value(SEQUENCE)
                .and_then(|s| s.parse().ok())
                .ok_or_else(bad)?,
            person: named(PROV_ATTRIBUTED).ok_or_else(bad)?,
            decision: named(DECISION)
                .and_then(|d| Decision::from_iri(&d))
                .ok_or_else(bad)?,
            draft_digest: value(DRAFT_DIGEST).ok_or_else(bad)?,
            passage: named(JUDGED_PASSAGE).ok_or_else(bad)?,
            statements: value(STATEMENTS).unwrap_or_default(),
            note: value(RDFS_COMMENT),
        });
    }
    out.sort_by_key(|a| a.sequence);
    Ok(Some((
        Verdict {
            iri: verdict,
            draft: draft.to_string(),
            acts: out,
        },
        version,
    )))
}

impl Verdict {
    /// The act in effect: the latest, unless it is a retraction.
    fn in_effect(&self) -> Option<&Act> {
        self.acts.last().filter(|a| a.decision != Decision::Retract)
    }

    fn triples(&self) -> Result<Vec<Triple>> {
        let node = |s: &str| {
            NamedNode::new(s).map_err(|e| Error::Endpoint(format!("verdict `{}`: {e}", self.iri)))
        };
        let p = |s: &str| NamedNode::new_unchecked(s);
        let verdict = node(&self.iri)?;
        let mut out = vec![
            Triple::new(verdict.clone(), p(RDF_TYPE), p(VERDICT_CLASS)),
            Triple::new(verdict.clone(), p(JUDGES), node(&self.draft)?),
        ];
        let mut previous: Option<NamedNode> = None;
        for a in &self.acts {
            let act = node(&format!("{}#act-{}", self.iri, a.sequence))?;
            out.push(Triple::new(verdict.clone(), p(ACT), act.clone()));
            let lit = |v: &str| Literal::new_simple_literal(v);
            out.push(Triple::new(
                act.clone(),
                p(SEQUENCE),
                Literal::new_typed_literal(a.sequence.to_string(), p(XSD_INTEGER)),
            ));
            out.push(Triple::new(
                act.clone(),
                p(PROV_ATTRIBUTED),
                node(&a.person)?,
            ));
            out.push(Triple::new(act.clone(), p(DECISION), p(a.decision.iri())));
            out.push(Triple::new(
                act.clone(),
                p(DRAFT_DIGEST),
                lit(&a.draft_digest),
            ));
            out.push(Triple::new(
                act.clone(),
                p(JUDGED_PASSAGE),
                node(&a.passage)?,
            ));
            if !a.statements.is_empty() {
                out.push(Triple::new(act.clone(), p(STATEMENTS), lit(&a.statements)));
            }
            if let Some(note) = &a.note {
                out.push(Triple::new(act.clone(), p(RDFS_COMMENT), lit(note)));
            }
            if let Some(prev) = previous.take() {
                out.push(Triple::new(act.clone(), p(DCTERMS_REPLACES), prev));
            }
            previous = Some(act);
        }
        Ok(out)
    }

    fn to_turtle(&self, replay: Option<&Replay>) -> Result<Vec<u8>> {
        let mut triples = self.triples()?;
        if let Some(Replay::Orphaned(_)) = replay {
            triples.push(Triple::new(
                NamedNode::new_unchecked(&self.iri),
                NamedNode::new_unchecked(ORPHANED),
                Literal::new_typed_literal("true", NamedNode::new_unchecked(XSD_BOOLEAN)),
            ));
        }
        let failed = |e: std::io::Error| {
            Error::Endpoint(format!("verdict `{}`: writing Turtle: {e}", self.iri))
        };
        let mut writer = TurtleSerializer::new()
            .with_prefix("knowledge", crate::VOCABULARY_NAMESPACE)
            .and_then(|s| s.with_prefix("prov", "http://www.w3.org/ns/prov#"))
            .map_err(|e| Error::Endpoint(format!("verdict `{}`: {e}", self.iri)))?
            .for_writer(Vec::new());
        for t in &triples {
            writer.serialize_triple(t).map_err(failed)?;
        }
        writer.finish().map_err(failed)
    }

    fn to_text(&self, replay: &Replay) -> String {
        let mut out = String::new();
        match self.in_effect() {
            None => {
                let last = self.acts.last();
                out.push_str(&format!(
                    "verdict on {}: retracted{}, none in effect\n",
                    self.draft,
                    last.map(|a| format!(" by <{}>", a.person))
                        .unwrap_or_default()
                ));
            }
            Some(a) => {
                let state = match replay {
                    Replay::Holds => "holds".to_string(),
                    Replay::Orphaned(why) => format!("orphaned: {why}"),
                };
                out.push_str(&format!(
                    "verdict on {}: {} by <{}>, {state}\n",
                    self.draft,
                    a.decision.past(),
                    a.person
                ));
                out.push_str(&format!("judged passage <{}>\n", a.passage));
                let lines: Vec<&str> = a.statements.lines().filter(|l| !l.is_empty()).collect();
                if a.decision != Decision::Reject {
                    out.push_str(&format!(
                        "{} accepted by a person:\n",
                        count(lines.len(), "statement", "statements")
                    ));
                    for line in lines {
                        out.push_str(&format!("  {line}\n"));
                    }
                }
                if let Some(note) = &a.note {
                    out.push_str(&format!("note: {note}\n"));
                }
            }
        }
        out.push_str(&format!(
            "history: {}, the latest supersedes the rest\n",
            count(self.acts.len(), "act", "acts")
        ));
        for a in &self.acts {
            out.push_str(&format!(
                "  {} {} by <{}>, draft digest {}\n",
                a.sequence,
                a.decision.past(),
                a.person,
                a.draft_digest
            ));
        }
        out
    }
}

/// Replay the act in effect over what is derived today: the passage it judged, compared
/// with the same range of the document now.
async fn replay(inv: &Invocation<'_>, verdict: &Verdict) -> Result<Replay> {
    let Some(act) = verdict.in_effect() else {
        return Ok(Replay::Holds);
    };
    let (p, _) = split_draft(&verdict.draft).unwrap_or_default();
    let judged = match inv.source(&iri(&act.passage)?).await {
        Ok(repr) => Some(String::from_utf8_lossy(&repr.bytes).into_owned()),
        Err(Error::NotFound(_)) => None,
        Err(e) => return Err(e),
    };
    match inv.source(&iri(p)?).await {
        Ok(repr) => {
            let now = String::from_utf8_lossy(&repr.bytes);
            match judged {
                Some(judged) if judged == now => Ok(Replay::Holds),
                Some(_) => Ok(Replay::Orphaned(format!(
                    "the passage {p} now holds other text than the person judged"
                ))),
                None => Ok(Replay::Orphaned(format!(
                    "the text the person judged, {}, is no longer kept, so the passage {p} \
                     cannot be compared with it",
                    act.passage
                ))),
            }
        }
        Err(Error::NotFound(_)) => Ok(Replay::Orphaned(format!("the passage {p} is gone"))),
        Err(e) => Err(e),
    }
}

/// Who is writing, refused unless the capability names a principal the host's store
/// declares a person.
async fn person(inv: &Invocation<'_>, store: &dyn VerdictStore) -> Result<String> {
    if !inv.capability.allows(VERDICT_WRITE) {
        return Err(Error::Denied(format!(
            "writing a verdict takes `{VERDICT_WRITE}`, and a principal declared a person"
        )));
    }
    let Some(principal) = inv.capability.principal().map(str::to_string) else {
        return Err(Error::Denied(
            "a verdict is a person's word, and this capability names nobody: a door mints the \
             person it authenticated as urn:cap:principal:<iri>, and a host's own root names \
             no one"
                .to_string(),
        ));
    };
    if store.is_person(&principal).await? {
        Ok(principal)
    } else {
        Err(Error::Denied(format!(
            "<{principal}> is not declared a person by the host, and only a person writes a \
             verdict: never a model, an agent or a service"
        )))
    }
}

/// The person's own statements, for a correction: Turtle with no blank node.
fn correction(inv: &Invocation<'_>) -> Result<String> {
    let content = inv
        .inline_str("content")
        .map_err(|_| Error::MissingArgument("content".to_string()))?;
    let bad = |detail: String| Error::InvalidArgument {
        name: "content".to_string(),
        detail,
    };
    let mut lines = Vec::new();
    for t in TurtleParser::new().for_slice(content.as_bytes()) {
        let t = t.map_err(|e| bad(format!("a correction is Turtle: {e}")))?;
        if matches!(t.subject, NamedOrBlankNode::BlankNode(_))
            || matches!(t.object, Term::BlankNode(_))
        {
            return Err(bad(format!(
                "`{t}` has a blank node; name every resource with an IRI"
            )));
        }
        lines.push(format!("{t} ."));
    }
    lines.sort();
    lines.dedup();
    Ok(lines.join("\n"))
}

/// The endpoint at [`VERDICT_TEMPLATE`].
pub(crate) struct VerdictEndpoint {
    /// The host's verdict store; `None` when the host bound none.
    pub(crate) store: Option<Arc<dyn VerdictStore>>,
}

impl VerdictEndpoint {
    fn store(&self) -> Result<&dyn VerdictStore> {
        self.store.as_deref().ok_or_else(|| {
            Error::Unavailable(
                "this host keeps no verdicts: it builds the module with \
                 ikigai_knowledge::space_with_verdicts and a store it holds"
                    .to_string(),
            )
        })
    }

    async fn read(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let draft = draft_arg(inv)?;
        let face = verdict_face(inv)?;
        if !inv.capability.allows(VERDICT_READ) {
            return Err(Error::Denied(format!(
                "reading a verdict takes `{VERDICT_READ}`"
            )));
        }
        let Some((verdict, _)) = read_verdict(self.store()?, &draft).await? else {
            return Err(Error::NotFound(format!(
                "no verdict on `{draft}`: no person has judged it"
            )));
        };
        let replayed = replay(inv, &verdict).await?;
        let (media, bytes) = match face {
            VerdictFace::Text => (TEXT, verdict.to_text(&replayed).into_bytes()),
            VerdictFace::Turtle => (TURTLE, verdict.to_turtle(Some(&replayed))?),
        };
        // Not cacheable: the host's store has no golden thread, and a later act must show.
        Ok(Representation::new(ReprType::new(media), bytes))
    }

    async fn write(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let draft = draft_arg(inv)?;
        let decision = Decision::parse(
            inv.inline_str("decision")
                .map_err(|_| Error::MissingArgument("decision".to_string()))?,
        )?;
        let store = self.store()?;
        // The seal, before anything is read.
        let person = person(inv, store).await?;
        // optional-arg: deliberate: a note is the person's own, and may be left out.
        let note = inv
            .inline_str("note")
            .ok()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string);
        if let Some(n) = &note {
            if n.chars().any(char::is_control) || n.chars().count() > 1000 {
                return Err(Error::InvalidArgument {
                    name: "note".to_string(),
                    detail: "a note is one line of at most 1000 characters, with no control \
                             character"
                        .to_string(),
                });
            }
        }
        let (mut verdict, read_version) = match read_verdict(store, &draft).await? {
            Some((v, version)) => (v, Some(version)),
            None => (
                Verdict {
                    iri: verdict_iri(&draft),
                    draft: draft.clone(),
                    acts: Vec::new(),
                },
                None,
            ),
        };
        let act = if decision == Decision::Retract {
            let Some(current) = verdict.in_effect() else {
                return Err(Error::Conflict(format!(
                    "no verdict on `{draft}` is in effect, so there is nothing to retract"
                )));
            };
            Act {
                sequence: 0,
                person: person.clone(),
                decision,
                draft_digest: current.draft_digest.clone(),
                passage: current.passage.clone(),
                statements: String::new(),
                note,
            }
        } else {
            let read = inv
                .inline_str("digest")
                .map_err(|_| Error::MissingArgument("digest".to_string()))?
                .trim()
                .to_string();
            // The draft the person read, as it was served: never a re-run of the model.
            let kept = store.draft(&read).await?.filter(|d| d.draft == draft);
            let Some(now) = kept else {
                return Err(Error::Conflict(format!(
                    "no draft `{draft}` with the digest {read} was served: read it again, then \
                     judge what it says"
                )));
            };
            let statements = match decision {
                Decision::Accept => now.statements.clone(),
                Decision::Correct => correction(inv)?,
                _ => String::new(),
            };
            Act {
                sequence: 0,
                person: person.clone(),
                decision,
                draft_digest: read,
                passage: now.passage.clone(),
                statements,
                note,
            }
        };
        let sequence = verdict.acts.last().map_or(1, |a| a.sequence + 1);
        verdict.acts.push(Act { sequence, ..act });
        let body = verdict.to_turtle(None)?;
        // Conditional on the version this act was decided over: a lost race is a Conflict.
        store.write(&draft, read_version.as_deref(), body).await?;
        let replayed = Replay::Holds;
        Ok(Representation::new(
            ReprType::new(TEXT),
            verdict.to_text(&replayed).into_bytes(),
        ))
    }
}

#[async_trait]
impl Endpoint for VerdictEndpoint {
    async fn invoke(&self, inv: &Invocation<'_>) -> Result<Representation> {
        match inv.request.verb {
            Verb::Sink => self.write(inv).await,
            _ => self.read(inv).await,
        }
    }

    fn describe(&self) -> Description {
        let x = || {
            ArgSpec::new("x")
                .summary("the draft's name, urn:knowledge:draft:passage:{p}:model:{m}")
                .class(XSD_STRING)
                .binding()
        };
        Description::new(VERDICT_ID)
            .title("Knowledge verdict")
            .summary(
                "A person's verdict on a model's draft: an atom kept in the host's verdict \
                 store, apart from everything derived. Source \
                 replays it: the decision in effect, by whom, the statements it accepted, \
                 and whether it holds over the passage as derived now or is orphaned because \
                 that passage is gone or changed. Sink records an act (decision=accept, \
                 reject, correct with content=, or retract), each superseding the last; it \
                 takes urn:cap:knowledge:write:verdict and a principal the host declares a \
                 person, so no model writes a verdict, and an accept, reject or correction \
                 names the digest= of the draft the person read, judged as it was served.",
            )
            .verb(Verb::Meta)
            .action(
                ActionSpec::new(Verb::Source)
                    .summary("replay the verdict over what is derived now")
                    .requires(VERDICT_READ)
                    .input(x())
                    .input(
                        ArgSpec::new("as")
                            .summary("the face: text/plain (default) or text/turtle")
                            .class(XSD_STRING)
                            .one_of([TEXT, TURTLE])
                            .default_value(TEXT)
                            .optional(),
                    )
                    .output(TEXT)
                    .output(TURTLE),
            )
            .action(
                ActionSpec::new(Verb::Sink)
                    .summary("record a person's act on the draft")
                    .requires(VERDICT_WRITE)
                    .input(x())
                    .input(
                        ArgSpec::new("decision")
                            .summary("accept, reject, correct or retract")
                            .class(XSD_STRING)
                            .one_of(["accept", "reject", "correct", "retract"]),
                    )
                    .input(
                        ArgSpec::new("digest")
                            .summary(
                                "the draft's digest as the person read it; required unless \
                                 retracting",
                            )
                            .class(XSD_STRING)
                            .optional(),
                    )
                    .input(
                        ArgSpec::new("content")
                            .summary("a correction's statements, as Turtle with no blank node")
                            .class(XSD_STRING)
                            .optional(),
                    )
                    .input(
                        ArgSpec::new("note")
                            .summary("the person's note on their decision")
                            .class(XSD_STRING)
                            .optional(),
                    )
                    .output(TEXT),
            )
    }
}

enum VerdictFace {
    Text,
    Turtle,
}

fn verdict_face(inv: &Invocation<'_>) -> Result<VerdictFace> {
    let Ok(asked) = inv.inline_str("as") else {
        return Ok(VerdictFace::Text);
    };
    match ikigai_core::media_type_essence(asked.trim()).as_str() {
        "" | TEXT => Ok(VerdictFace::Text),
        TURTLE => Ok(VerdictFace::Turtle),
        other => Err(Error::InvalidArgument {
            name: "as".to_string(),
            detail: format!(
                "`{other}` is not a face of a verdict: ask for text/plain (the default) or \
                 text/turtle"
            ),
        }),
    }
}
