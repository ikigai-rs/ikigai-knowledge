//! Documents as sources: a chunking rule, a document's passages under it, and one passage.
//!
//! - **`urn:knowledge:chunking:name:{n}`** is a chunking rule, a Turtle file in the config
//!   home, `knowledge/chunking/{n}.ttl`, stating `<> knowledge:maxCharacters N`.
//! - **`urn:knowledge:chunk:document:{d}:chunking:{n}`** is the passages of the document `{d}`
//!   under the rule `{n}`: their names, one per line.
//! - **`urn:knowledge:passage:document:{d}:range:{start}-{end}`** is one passage: the
//!   document's text from character `{start}` (inclusive) to `{end}` (exclusive).
//!
//! `{d}` is the document's own IRI. Today that is a file under the config home's `knowledge`
//! directory, `urn:knowledge:file:path:{path}`, which the host binds; the document alias,
//! repository trees and lifts are later rows of the map.
//!
//! # The rule
//!
//! A paragraph is a run of lines that are not blank. Consecutive paragraphs are packed into
//! one passage while the passage spans at most `maxCharacters` characters; a paragraph longer
//! than that is cut into pieces of `maxCharacters`. A passage starts at its first paragraph's
//! first character and ends after its last paragraph's last character, so ranges are what a
//! `TextPositionSelector` carries.
//!
//! # Golden threads
//!
//! Every answer is cacheable and reads its inputs through the kernel, so it hangs from their
//! threads: a passage and a chunk list from the document's file, a chunk list also from the
//! rule. A change to one file re-derives only that file's passages; every other document's
//! stay cached. A change to the rule re-derives every chunk list under it.

use async_trait::async_trait;
use ikigai_core::{
    ArgSpec, Description, Endpoint, Error, Invocation, Iri, ReprType, Representation, Result, Verb,
};
use oxrdf::{NamedOrBlankNode, Term};

use crate::{check_name, read_turtle, FILE_PREFIX, FILE_TEMPLATE, FS_READ, XSD_STRING};

/// The IRI template a chunking rule is served at.
pub const CHUNKING_TEMPLATE: &str = "urn:knowledge:chunking:name:{n}";
/// The IRI prefix of [`CHUNKING_TEMPLATE`]: a rule's name follows it.
pub const CHUNKING_PREFIX: &str = "urn:knowledge:chunking:name:";
/// The chunking rule endpoint's description id.
pub const CHUNKING_ID: &str = "knowledge-chunking";
/// The IRI template a document's passages under a rule are listed at.
pub const CHUNK_TEMPLATE: &str = "urn:knowledge:chunk:document:{d}:chunking:{n}";
/// The IRI prefix of [`CHUNK_TEMPLATE`].
pub const CHUNK_PREFIX: &str = "urn:knowledge:chunk:document:";
/// The chunk list endpoint's description id.
pub const CHUNK_ID: &str = "knowledge-chunk";
/// The IRI template one passage is served at.
pub const PASSAGE_TEMPLATE: &str = "urn:knowledge:passage:document:{d}:range:{start}-{end}";
/// The IRI prefix of [`PASSAGE_TEMPLATE`].
pub const PASSAGE_PREFIX: &str = "urn:knowledge:passage:document:";
/// The passage endpoint's description id.
pub const PASSAGE_ID: &str = "knowledge-passage";

/// The scope that reads the chunking rule called `{n}` is this followed by the name.
pub const CHUNKING_READ_PREFIX: &str = "urn:cap:knowledge:read:chunking:name:";
/// What the chunking endpoints declare: some chunking rule read grant.
pub const CHUNKING_READ: &str = "urn:cap:knowledge:read:chunking:name:*";

/// The term a chunking rule states its passage length with.
pub const MAX_CHARACTERS: &str = "https://ikigai-rs.dev/ns/knowledge#maxCharacters";
/// The largest `maxCharacters` a rule may state.
pub const MAX_PASSAGE: usize = 100_000;

const TEXT: &str = "text/plain";

/// The scope that reads the chunking rule called `name`.
///
/// ```
/// assert_eq!(
///     ikigai_knowledge::chunking_scope("prose"),
///     "urn:cap:knowledge:read:chunking:name:prose"
/// );
/// ```
pub fn chunking_scope(name: &str) -> String {
    format!("{CHUNKING_READ_PREFIX}{name}")
}

/// The name of the passage of `document` from character `start` to `end`.
///
/// ```
/// assert_eq!(
///     ikigai_knowledge::passage_iri("urn:knowledge:file:path:docs/a.md", 0, 12),
///     "urn:knowledge:passage:document:urn:knowledge:file:path:docs/a.md:range:0-12"
/// );
/// ```
pub fn passage_iri(document: &str, start: usize, end: usize) -> String {
    format!("{PASSAGE_PREFIX}{document}:range:{start}-{end}")
}

/// The character ranges, `[start, end)`, of `text`'s passages under a rule of `max`
/// characters: paragraphs packed while a passage spans at most `max`, a longer paragraph cut
/// into pieces of `max`. `max` is at least 1.
///
/// ```
/// let text = "One.\n\nTwo.\n\n\nA longer third paragraph.\n";
/// assert_eq!(ikigai_knowledge::chunk(text, 10), vec![(0, 10), (13, 23), (23, 33), (33, 38)]);
/// assert_eq!(ikigai_knowledge::chunk(text, 1000), vec![(0, 38)]);
/// assert!(ikigai_knowledge::chunk("\n\n", 10).is_empty());
/// ```
pub fn chunk(text: &str, max: usize) -> Vec<(usize, usize)> {
    let max = max.max(1);
    // Paragraphs: maximal runs of non-blank lines, as character ranges without the final
    // line break.
    let mut paragraphs: Vec<(usize, usize)> = Vec::new();
    let mut open: Option<(usize, usize)> = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let length = line.chars().count();
        let body = line.strip_suffix('\n').unwrap_or(line);
        let body = body.strip_suffix('\r').unwrap_or(body);
        if body.trim().is_empty() {
            if let Some(p) = open.take() {
                paragraphs.push(p);
            }
        } else {
            let end = at + body.chars().count();
            open = Some(match open {
                Some((start, _)) => (start, end),
                None => (at, end),
            });
        }
        at += length;
    }
    if let Some(p) = open {
        paragraphs.push(p);
    }
    let mut passages = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    for (start, end) in paragraphs {
        if let Some((s, e)) = current {
            if end - s <= max {
                current = Some((s, end));
                continue;
            }
            passages.push((s, e));
        }
        let mut piece = start;
        while end - piece > max {
            passages.push((piece, piece + max));
            piece += max;
        }
        current = Some((piece, end));
    }
    if let Some(p) = current {
        passages.push(p);
    }
    passages
}

/// The document `{d}` names: a file under the config home's `knowledge` directory.
fn document_arg(inv: &Invocation<'_>) -> Result<String> {
    let d = inv
        .bindings
        .get("d")
        .ok_or_else(|| Error::MissingArgument("d".to_string()))?;
    let path = d.strip_prefix(FILE_PREFIX).unwrap_or("");
    let ok = !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
        && !path.chars().any(|c| c.is_control() || c.is_whitespace());
    if ok && Iri::parse(d.to_string()).is_ok() {
        Ok(d.to_string())
    } else {
        Err(Error::InvalidArgument {
            name: "d".to_string(),
            detail: format!(
                "`{d}` is not a document: name a file under the config home's knowledge \
                 directory as {FILE_PREFIX}{{path}}, such as {FILE_PREFIX}docs/README.md"
            ),
        })
    }
}

/// The rule `{n}` names, refusing a caller who may not read it.
fn rule_arg(inv: &Invocation<'_>) -> Result<String> {
    let n = inv
        .bindings
        .get("n")
        .ok_or_else(|| Error::MissingArgument("n".to_string()))?;
    check_name("chunking", n)?;
    if !inv.capability.allows(&chunking_scope(n)) {
        return Err(Error::Denied(format!(
            "reading the chunking rule `{n}` takes `{}`",
            chunking_scope(n)
        )));
    }
    Ok(n.to_string())
}

/// Read the rule called `name` from its file: its `maxCharacters`.
async fn read_rule(inv: &Invocation<'_>, name: &str) -> Result<usize> {
    let file = format!("{FILE_PREFIX}chunking/{name}.ttl");
    let path = format!("knowledge/chunking/{name}.ttl");
    let base = format!("{CHUNKING_PREFIX}{name}");
    let what = format!("chunking rule `{name}`");
    let triples = read_turtle(inv, &file, &base, &what, &path).await?;
    let stated: Vec<&Term> = triples
        .iter()
        .filter(|t| {
            matches!(&t.subject, NamedOrBlankNode::NamedNode(s) if s.as_str() == base)
                && t.predicate.as_str() == MAX_CHARACTERS
        })
        .map(|t| &t.object)
        .collect();
    let value = match stated.as_slice() {
        [Term::Literal(l)] => l.value().parse::<usize>().ok(),
        _ => None,
    };
    match value {
        Some(n) if (1..=MAX_PASSAGE).contains(&n) => Ok(n),
        _ => Err(Error::Endpoint(format!(
            "{what}: {path} must state `<> <{MAX_CHARACTERS}> N` exactly once, N a whole \
             number from 1 to {MAX_PASSAGE}"
        ))),
    }
}

/// Read the document's text through the kernel.
async fn read_document(inv: &Invocation<'_>, document: &str) -> Result<String> {
    let iri =
        Iri::parse(document).map_err(|e| Error::Endpoint(format!("document `{document}`: {e}")))?;
    let repr = match inv.source(&iri).await {
        Ok(repr) => repr,
        Err(Error::NotFound(why)) => {
            inv.depends_on(document);
            return Err(Error::NotFound(format!("no document `{document}`: {why}")));
        }
        Err(Error::Unresolved(_)) => {
            inv.depends_on(document);
            return Err(Error::NotFound(format!(
                "no document `{document}`: nothing serves it in this kernel; a host serves \
                 documents by binding a file module at {FILE_TEMPLATE} over the config home's \
                 knowledge directory"
            )));
        }
        Err(e) => return Err(e),
    };
    String::from_utf8(repr.bytes.to_vec()).map_err(|_| {
        Error::Endpoint(format!(
            "document `{document}` is not UTF-8 text, so it has no passages"
        ))
    })
}

fn text_answer(body: String) -> Representation {
    Representation::new(ReprType::new(TEXT), body.into_bytes()).cacheable()
}

/// The endpoint at [`CHUNKING_TEMPLATE`].
pub(crate) struct ChunkingEndpoint;

#[async_trait]
impl Endpoint for ChunkingEndpoint {
    async fn invoke(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let name = rule_arg(inv)?;
        let max = read_rule(inv, &name).await?;
        let noun = if max == 1 { "character" } else { "characters" };
        Ok(text_answer(format!(
            "chunking {name}: paragraphs packed into passages of at most {max} {noun}\n"
        )))
    }

    fn describe(&self) -> Description {
        Description::new(CHUNKING_ID)
            .title("Knowledge chunking rule")
            .summary(
                "A chunking rule found by name in the config home: \
                 <config home>/knowledge/chunking/{n}.ttl, stating <> knowledge:maxCharacters N. \
                 Paragraphs (runs of non-blank lines) are packed into passages of at most N \
                 characters; a longer paragraph is cut into pieces of N. Takes \
                 urn:cap:knowledge:read:chunking:name:{n} and the file's read. text/plain.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(CHUNKING_READ)
            .requires(FS_READ)
            .input(
                ArgSpec::new("n")
                    .summary("the rule's name: the file knowledge/chunking/{n}.ttl")
                    .class(XSD_STRING)
                    .binding(),
            )
            .output(TEXT)
    }
}

/// The endpoint at [`CHUNK_TEMPLATE`].
pub(crate) struct ChunkEndpoint;

#[async_trait]
impl Endpoint for ChunkEndpoint {
    async fn invoke(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let document = document_arg(inv)?;
        let name = rule_arg(inv)?;
        // The rule is read as the resource it is, so the list hangs from its thread.
        let rule = Iri::parse(format!("{CHUNKING_PREFIX}{name}"))
            .map_err(|e| Error::Endpoint(format!("chunking rule `{name}`: {e}")))?;
        inv.source(&rule).await?;
        let max = read_rule(inv, &name).await?;
        let text = read_document(inv, &document).await?;
        let mut out = String::new();
        for (start, end) in chunk(&text, max) {
            out.push_str(&passage_iri(&document, start, end));
            out.push('\n');
        }
        Ok(text_answer(out))
    }

    fn describe(&self) -> Description {
        Description::new(CHUNK_ID)
            .title("Knowledge document passages")
            .summary(
                "The passages of a document under a chunking rule: their names, \
                 urn:knowledge:passage:document:{d}:range:{start}-{end}, one per line in \
                 document order, empty when the document has no text. {d} is the document's \
                 IRI, a file under the config home's knowledge directory \
                 (urn:knowledge:file:path:{path}), read through the kernel; {n} is the rule. \
                 Cached under the document's and the rule's threads, so a changed file \
                 re-derives only its own passages. Takes \
                 urn:cap:knowledge:read:chunking:name:{n} and the file's read. text/plain.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(CHUNKING_READ)
            .requires(FS_READ)
            .input(
                ArgSpec::new("d")
                    .summary("the document's IRI, such as urn:knowledge:file:path:docs/README.md")
                    .class(XSD_STRING)
                    .binding(),
            )
            .input(
                ArgSpec::new("n")
                    .summary("the chunking rule's name")
                    .class(XSD_STRING)
                    .binding(),
            )
            .output(TEXT)
    }
}

/// The endpoint at [`PASSAGE_TEMPLATE`].
pub(crate) struct PassageEndpoint;

/// A range bound: a whole number of characters.
fn bound(inv: &Invocation<'_>, name: &str) -> Result<usize> {
    let raw = inv
        .bindings
        .get(name)
        .ok_or_else(|| Error::MissingArgument(name.to_string()))?;
    let ok = !raw.is_empty() && raw.chars().all(|c| c.is_ascii_digit());
    raw.parse::<usize>()
        .ok()
        .filter(|_| ok)
        .ok_or_else(|| Error::InvalidArgument {
            name: name.to_string(),
            detail: format!("`{raw}` is not a character offset: use a whole number, such as 0"),
        })
}

#[async_trait]
impl Endpoint for PassageEndpoint {
    async fn invoke(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let document = document_arg(inv)?;
        let start = bound(inv, "start")?;
        let end = bound(inv, "end")?;
        if start >= end {
            return Err(Error::InvalidArgument {
                name: "end".to_string(),
                detail: format!(
                    "a passage ends after it starts: {end} is not past {start} (the end is \
                     exclusive)"
                ),
            });
        }
        let text = read_document(inv, &document).await?;
        let length = text.chars().count();
        if end > length {
            return Err(Error::NotFound(format!(
                "no passage {start}-{end} in `{document}`: it has {length} characters"
            )));
        }
        let slice: String = text.chars().skip(start).take(end - start).collect();
        Ok(text_answer(slice))
    }

    fn describe(&self) -> Description {
        Description::new(PASSAGE_ID)
            .title("Knowledge passage")
            .summary(
                "One passage of a document: its text from character {start} (inclusive) to \
                 {end} (exclusive), as a TextPositionSelector counts them. {d} is the \
                 document's IRI, a file under the config home's knowledge directory \
                 (urn:knowledge:file:path:{path}), read through the kernel under the caller's \
                 file read. A range past the text is not-found. Cached under the document's \
                 thread. text/plain.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(FS_READ)
            .input(
                ArgSpec::new("d")
                    .summary("the document's IRI, such as urn:knowledge:file:path:docs/README.md")
                    .class(XSD_STRING)
                    .binding(),
            )
            .input(
                ArgSpec::new("start")
                    .summary("the first character, counted from 0")
                    .class(XSD_STRING)
                    .binding(),
            )
            .input(
                ArgSpec::new("end")
                    .summary("the character after the last")
                    .class(XSD_STRING)
                    .binding(),
            )
            .output(TEXT)
    }
}
