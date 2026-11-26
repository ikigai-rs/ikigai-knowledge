//! Passages that stay put: a document's revision, and a passage as of that revision.
//!
//! A passage name, `urn:knowledge:passage:document:{d}:range:{start}-{end}`, is a range of
//! the document's CURRENT text, so it moves when the document changes: a citation made with
//! it may later point at different words. These names do not move.
//!
//! - **`urn:knowledge:revision:document:{d}`** is the document's revision: the BLAKE3 digest
//!   of its text, 64 lowercase hex digits. `Source` answers it. `Sink` also archives the
//!   text, content-addressed, at `urn:knowledge:file:path:revision/{r}.txt` (the config
//!   home's `knowledge/revision/{r}.txt`), written through the kernel, never over a
//!   different archive.
//! - **`urn:knowledge:passage:revision:{r}:document:{d}:range:{start}-{end}`** is a passage as
//!   of the revision `{r}`: the text of that revision from `{start}` to `{end}`, from the
//!   document while it is still that revision and from the archive after it changes. Its
//!   Turtle face is the W3C Web Annotation target: the document, a `TextQuoteSelector` and a
//!   `TextPositionSelector`, the revision, and the same words in the current text
//!   (`knowledge:currentPassage`) when the quote still finds them there.
//!
//! An archive is checked against its name on every read: text whose digest is not `{r}` is
//! refused, never served as that revision.

use async_trait::async_trait;
use ikigai_core::{
    ActionSpec, ArgRef, ArgSpec, Description, Endpoint, Error, Invocation, Iri, ReprType,
    Representation, Request, Result, Verb,
};
use oxrdf::{Literal, NamedNode, Triple};
use oxttl::TurtleSerializer;

use crate::passage::{bound, document_arg, read_document, read_optional, slice};
use crate::{passage_iri, FILE_PREFIX, FS_READ, VOCABULARY_NAMESPACE, XSD_STRING};

/// The IRI template a document's revision is served at.
pub const REVISION_TEMPLATE: &str = "urn:knowledge:revision:document:{d}";
/// The IRI prefix of [`REVISION_TEMPLATE`].
pub const REVISION_PREFIX: &str = "urn:knowledge:revision:document:";
/// The revision endpoint's description id.
pub const REVISION_ID: &str = "knowledge-revision";
/// The IRI template a passage as of a revision is served at.
pub const PASSAGE_AT_TEMPLATE: &str =
    "urn:knowledge:passage:revision:{r}:document:{d}:range:{start}-{end}";
/// The IRI prefix of [`PASSAGE_AT_TEMPLATE`].
pub const PASSAGE_AT_PREFIX: &str = "urn:knowledge:passage:revision:";
/// The as-of passage endpoint's description id.
pub const PASSAGE_AT_ID: &str = "knowledge-passage-at";
/// What archiving a revision takes under `ikigai-fs`: some write grant, judged by path where
/// the archive is written.
pub const FS_WRITE: &str = "urn:cap:fs:write:*";

/// On an as-of passage: the revision of the document it is a range of.
pub const REVISION: &str = "https://ikigai-rs.dev/ns/knowledge#revision";
/// On an as-of passage: the passage of the document's current text with the same words.
pub const CURRENT_PASSAGE: &str = "https://ikigai-rs.dev/ns/knowledge#currentPassage";

/// How many characters of context a `TextQuoteSelector`'s prefix and suffix carry.
pub const QUOTE_CONTEXT: usize = 32;

const TEXT: &str = "text/plain";
const TURTLE: &str = "text/turtle";
const OA: &str = "http://www.w3.org/ns/oa#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_NON_NEGATIVE: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";

/// The revision of a document whose text is `text`: its BLAKE3 digest, in lowercase hex.
///
/// ```
/// let r = ikigai_knowledge::revision_of("# Alpha\n");
/// assert_eq!(r.len(), 64);
/// assert_ne!(r, ikigai_knowledge::revision_of("# Alpha, edited\n"));
/// ```
pub fn revision_of(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

/// The name of the passage of `document` from `start` to `end` as of `revision`.
///
/// ```
/// let r = "0".repeat(64);
/// assert_eq!(
///     ikigai_knowledge::passage_at_iri(&r, "urn:knowledge:file:path:docs/a.md", 0, 12),
///     format!(
///         "urn:knowledge:passage:revision:{r}:document:urn:knowledge:file:path:docs/a.md:range:0-12"
///     )
/// );
/// ```
pub fn passage_at_iri(revision: &str, document: &str, start: usize, end: usize) -> String {
    format!("{PASSAGE_AT_PREFIX}{revision}:document:{document}:range:{start}-{end}")
}

/// The path of an as-of passage under a persistent-naming namespace (an ikigai-name
/// `redirect` claim): `passage/{r}/{start}-{end}/{path}`, for a file document
/// `urn:knowledge:file:path:{path}`. `None` for any other document.
///
/// ```
/// let r = "0".repeat(64);
/// assert_eq!(
///     ikigai_knowledge::persistent_path(&r, "urn:knowledge:file:path:docs/a.md", 0, 12),
///     Some(format!("passage/{r}/0-12/docs/a.md"))
/// );
/// assert_eq!(ikigai_knowledge::persistent_path(&r, "urn:iki:ledger:item:1", 0, 12), None);
/// ```
pub fn persistent_path(revision: &str, document: &str, start: usize, end: usize) -> Option<String> {
    let path = document.strip_prefix(FILE_PREFIX)?;
    Some(format!("passage/{revision}/{start}-{end}/{path}"))
}

/// Where the text of `revision` is archived: a file under the config home's `knowledge`
/// directory, named by its digest.
///
/// ```
/// let r = "0".repeat(64);
/// assert_eq!(
///     ikigai_knowledge::revision_file(&r),
///     format!("urn:knowledge:file:path:revision/{r}.txt")
/// );
/// ```
pub fn revision_file(revision: &str) -> String {
    format!("{FILE_PREFIX}revision/{revision}.txt")
}

/// The revision `{r}` names: 64 lowercase hex digits.
fn revision_arg(inv: &Invocation<'_>) -> Result<String> {
    let r = inv
        .bindings
        .get("r")
        .ok_or_else(|| Error::MissingArgument("r".to_string()))?;
    if r.len() == 64 && r.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
        Ok(r.to_string())
    } else {
        Err(Error::InvalidArgument {
            name: "r".to_string(),
            detail: format!(
                "`{r}` is not a revision: use the 64 lowercase hex digits of the document's \
                 BLAKE3 digest, as {REVISION_PREFIX}{{d}} answers it"
            ),
        })
    }
}

/// The endpoint at [`REVISION_TEMPLATE`].
pub(crate) struct RevisionEndpoint;

#[async_trait]
impl Endpoint for RevisionEndpoint {
    async fn invoke(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let document = document_arg(inv)?;
        let text = read_document(inv, &document).await?;
        let revision = revision_of(&text);
        if inv.request.verb != Verb::Sink {
            return Ok(Representation::new(
                ReprType::new(TEXT),
                format!("{revision}\n").into_bytes(),
            )
            .cacheable());
        }
        let file = revision_file(&revision);
        let target = Iri::parse(&file)
            .map_err(|e| Error::Endpoint(format!("revision `{revision}`: {e}")))?;
        let write = Request::new(Verb::Sink, target)
            .with_arg("content", ArgRef::Inline(text.into_bytes()))
            .with_arg("if-none-match", ArgRef::Inline(b"*".to_vec()));
        match inv.issue(write).await {
            // Already archived: the archive is named by its digest, and every read checks it.
            Ok(_) | Err(Error::Conflict(_)) => {}
            Err(Error::Unresolved(_)) => {
                return Err(Error::NotFound(format!(
                    "revision {revision} of `{document}` was not archived: nothing serves \
                     {file} in this kernel; a host archives revisions by binding a writable \
                     file module at {FILE_PREFIX}{{path}} over the config home's knowledge directory"
                )))
            }
            Err(e) => return Err(e),
        }
        Ok(Representation::new(
            ReprType::new(TEXT),
            format!("{revision}\n").into_bytes(),
        ))
    }

    fn describe(&self) -> Description {
        let d = || {
            ArgSpec::new("d")
                .summary("the document's IRI, such as urn:knowledge:file:path:docs/README.md")
                .class(XSD_STRING)
                .binding()
        };
        Description::new(REVISION_ID)
            .title("Knowledge document revision")
            .summary(
                "A document's revision: the BLAKE3 digest of its text, 64 lowercase hex \
                 digits, one line. {d} is the document's IRI, a file under the config home's \
                 knowledge directory (urn:knowledge:file:path:{path}), read through the kernel. \
                 Source answers it, cached under the document's thread. Sink also archives the \
                 text at urn:knowledge:file:path:revision/{r}.txt, written through the kernel \
                 and never over an existing archive, so a passage as of the revision outlives \
                 the document's next edit; it takes the file read and write. text/plain.",
            )
            .verb(Verb::Source)
            .verb(Verb::Sink)
            .verb(Verb::Meta)
            .requires(FS_READ)
            .input(d())
            .output(TEXT)
            .action(
                ActionSpec::new(Verb::Source)
                    .summary("the document's current revision")
                    .requires(FS_READ)
                    .input(d())
                    .output(TEXT),
            )
            .action(
                ActionSpec::new(Verb::Sink)
                    .summary("archive the document's current text under its revision")
                    .requires(FS_READ)
                    .requires(FS_WRITE)
                    .input(d())
                    .output(TEXT),
            )
    }
}

/// The two faces of an as-of passage.
enum Face {
    Text,
    Turtle,
}

fn face(inv: &Invocation<'_>) -> Result<Face> {
    let Ok(asked) = inv.inline_str("as") else {
        return Ok(Face::Text);
    };
    match ikigai_core::media_type_essence(asked.trim()).as_str() {
        "" | TEXT => Ok(Face::Text),
        TURTLE => Ok(Face::Turtle),
        other => Err(Error::InvalidArgument {
            name: "as".to_string(),
            detail: format!(
                "`{other}` is not a face of a passage: ask for text/plain (the default, its \
                 text) or text/turtle (its Web Annotation target)"
            ),
        }),
    }
}

/// The endpoint at [`PASSAGE_AT_TEMPLATE`].
pub(crate) struct PassageAtEndpoint;

#[async_trait]
impl Endpoint for PassageAtEndpoint {
    async fn invoke(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let revision = revision_arg(inv)?;
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
        let face = face(inv)?;
        // The document as it is now: the revision itself while it is unchanged, and where the
        // current passage is found after it changes. A document since removed has no current
        // text, and the archive still answers.
        let current = read_optional(inv, &document).await?;
        let text = match &current {
            Some(now) if revision_of(now) == revision => now.clone(),
            _ => read_archive(inv, &document, &revision, current.is_some()).await?,
        };
        let length = text.chars().count();
        if end > length {
            return Err(Error::NotFound(format!(
                "no passage {start}-{end} in revision {revision} of `{document}`: it has \
                 {length} characters"
            )));
        }
        let exact = slice(&text, start, end);
        let bytes = match face {
            Face::Text => {
                return Ok(Representation::new(ReprType::new(TEXT), exact.into_bytes()).cacheable())
            }
            Face::Turtle => {
                let prefix = slice(&text, start.saturating_sub(QUOTE_CONTEXT), start);
                let suffix = slice(&text, end, (end + QUOTE_CONTEXT).min(length));
                let moved = current
                    .as_deref()
                    .and_then(|now| relocate(now, &exact, &prefix, &suffix))
                    .map(|(s, e)| passage_iri(&document, s, e));
                let target = Target {
                    iri: passage_at_iri(&revision, &document, start, end),
                    document: &document,
                    revision: &revision,
                    start,
                    end,
                    exact: &exact,
                    prefix: &prefix,
                    suffix: &suffix,
                    current: moved,
                };
                target.to_turtle()?
            }
        };
        Ok(Representation::new(ReprType::new(TURTLE), bytes).cacheable())
    }

    fn describe(&self) -> Description {
        let binding = |name: &str, summary: &str| {
            ArgSpec::new(name)
                .summary(summary)
                .class(XSD_STRING)
                .binding()
        };
        Description::new(PASSAGE_AT_ID)
            .title("Knowledge passage as of a revision")
            .summary(
                "One passage of a document as of a revision, so a citation keeps pointing at \
                 the same words after the document changes: the text of revision {r} from \
                 character {start} (inclusive) to {end} (exclusive). Read from the document \
                 while it is still that revision, and from its archive \
                 (urn:knowledge:file:path:revision/{r}.txt, written by a Sink to \
                 urn:knowledge:revision:document:{d}) after it changes; an archive whose \
                 digest is not {r} is refused. text/plain is the text exactly; as=text/turtle \
                 is its Web Annotation target: the document, a TextQuoteSelector and a \
                 TextPositionSelector, knowledge:revision, and knowledge:currentPassage when \
                 the quote finds the same words in the current text. Cached under the \
                 document's and the archive's threads.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(FS_READ)
            .input(binding(
                "r",
                "the revision: the document's BLAKE3 digest, 64 lowercase hex digits",
            ))
            .input(binding(
                "d",
                "the document's IRI, such as urn:knowledge:file:path:docs/README.md",
            ))
            .input(binding("start", "the first character, counted from 0"))
            .input(binding("end", "the character after the last"))
            .input(
                ArgSpec::new("as")
                    .summary("text/plain (the default) or text/turtle")
                    .class(XSD_STRING)
                    .one_of([TEXT, TURTLE])
                    .optional(),
            )
            .output(TEXT)
            .output(TURTLE)
    }
}

/// The text of `revision` from its archive, refused unless its digest is the revision.
async fn read_archive(
    inv: &Invocation<'_>,
    document: &str,
    revision: &str,
    changed: bool,
) -> Result<String> {
    let file = revision_file(revision);
    let why = if changed {
        "the document has changed since"
    } else {
        "the document is not there"
    };
    let Some(text) = read_optional(inv, &file).await? else {
        return Err(Error::NotFound(format!(
            "no revision {revision} of `{document}`: {why}, and that revision was not archived \
             (a Sink to {REVISION_PREFIX}{document} archives the current one); the current \
             text is at {document}"
        )));
    };
    if revision_of(&text) != revision {
        return Err(Error::Endpoint(format!(
            "the archive of revision {revision} at {file} holds other text (its digest is {}), \
             so it is not served as that revision",
            revision_of(&text)
        )));
    }
    Ok(text)
}

/// Where the quoted words are in `text`, by the `TextQuoteSelector`: the occurrence of
/// `exact` with this prefix and suffix around it, else the one occurrence of `exact` when
/// there is exactly one. Character offsets, end exclusive.
fn relocate(text: &str, exact: &str, prefix: &str, suffix: &str) -> Option<(usize, usize)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find(exact) {
        let byte = from + at;
        found.push(byte);
        from = byte + text[byte..].chars().next().map_or(1, char::len_utf8);
    }
    let fits = |&&byte: &&usize| {
        text[..byte].ends_with(prefix) && text[byte + exact.len()..].starts_with(suffix)
    };
    let byte = match found.iter().find(fits) {
        Some(&byte) => byte,
        None if found.len() == 1 => found[0],
        None => return None,
    };
    let start = text[..byte].chars().count();
    Some((start, start + exact.chars().count()))
}

/// An as-of passage's Web Annotation target.
struct Target<'a> {
    iri: String,
    document: &'a str,
    revision: &'a str,
    start: usize,
    end: usize,
    exact: &'a str,
    prefix: &'a str,
    suffix: &'a str,
    current: Option<String>,
}

impl Target<'_> {
    fn to_turtle(&self) -> Result<Vec<u8>> {
        let node = |s: &str| {
            NamedNode::new(s).map_err(|e| Error::Endpoint(format!("the passage's IRI: {e}")))
        };
        let p = |s: &str| NamedNode::new_unchecked(s);
        let oa = |local: &str| NamedNode::new_unchecked(format!("{OA}{local}"));
        let count = |n: usize| Literal::new_typed_literal(n.to_string(), p(XSD_NON_NEGATIVE));
        let target = node(&self.iri)?;
        let quote = node(&format!("{}#quote", self.iri))?;
        let position = node(&format!("{}#position", self.iri))?;
        let mut triples = vec![
            Triple::new(target.clone(), p(RDF_TYPE), oa("SpecificResource")),
            Triple::new(target.clone(), oa("hasSource"), node(self.document)?),
            Triple::new(
                target.clone(),
                p(REVISION),
                Literal::new_simple_literal(self.revision),
            ),
            Triple::new(target.clone(), oa("hasSelector"), quote.clone()),
            Triple::new(target.clone(), oa("hasSelector"), position.clone()),
            Triple::new(quote.clone(), p(RDF_TYPE), oa("TextQuoteSelector")),
            Triple::new(
                quote.clone(),
                oa("exact"),
                Literal::new_simple_literal(self.exact),
            ),
        ];
        if !self.prefix.is_empty() {
            triples.push(Triple::new(
                quote.clone(),
                oa("prefix"),
                Literal::new_simple_literal(self.prefix),
            ));
        }
        if !self.suffix.is_empty() {
            triples.push(Triple::new(
                quote,
                oa("suffix"),
                Literal::new_simple_literal(self.suffix),
            ));
        }
        triples.extend([
            Triple::new(position.clone(), p(RDF_TYPE), oa("TextPositionSelector")),
            Triple::new(position.clone(), oa("start"), count(self.start)),
            Triple::new(position, oa("end"), count(self.end)),
        ]);
        if let Some(current) = &self.current {
            triples.push(Triple::new(target, p(CURRENT_PASSAGE), node(current)?));
        }
        let failed = |e: std::io::Error| Error::Endpoint(format!("writing Turtle: {e}"));
        let mut writer = TurtleSerializer::new()
            .with_prefix("oa", OA)
            .and_then(|s| s.with_prefix("knowledge", VOCABULARY_NAMESPACE))
            .map_err(|e| Error::Endpoint(format!("a prefix: {e}")))?
            .for_writer(Vec::new());
        for t in &triples {
            writer.serialize_triple(t).map_err(failed)?;
        }
        writer.finish().map_err(failed)
    }
}

#[cfg(test)]
mod tests {
    use super::relocate;

    #[test]
    fn the_quote_finds_its_words_by_their_context_then_alone() {
        let text = "one two. one two. three";
        // Two occurrences: the context picks the second.
        assert_eq!(relocate(text, "one two", ". ", ". three"), Some((9, 16)));
        // Two occurrences and no context that fits: nothing.
        assert_eq!(relocate(text, "one two", "x", "y"), None);
        // One occurrence: found though its context changed.
        assert_eq!(relocate("new. three", "three", "two. ", ""), Some((5, 10)));
        assert_eq!(relocate("é three", "three", "", ""), Some((2, 7)));
        assert_eq!(relocate("gone", "three", "", ""), None);
    }
}
