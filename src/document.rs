//! Documents named through a source: `urn:knowledge:document:source:{s}:id:{d}`.
//!
//! A source declaration that holds documents states where they live, once:
//!
//! ```turtle
//! <> knowledge:documentBase <urn:knowledge:file:path:docs/> .
//! ```
//!
//! and the document `{d}` of the source `{s}` is an ALIAS onto the resource named by that base
//! followed by `{d}`: `urn:knowledge:document:source:docs:id:guide/intro.md` answers what
//! `urn:knowledge:file:path:docs/guide/intro.md` answers, byte for byte, in its media type.
//! The alias is read under the source's grant, so which documents a caller may name follows
//! the source they may read; the target is read through the kernel under the caller's own
//! capability, so the resource that answers there judges it as it judges any read.
//!
//! The chunk list and the passage take an alias as their `{d}`, like a file: a passage of an
//! aliased document is named under the alias, so a citation names the source it rests on.
//!
//! # Golden threads
//!
//! The answer is cacheable and hangs from the threads of what it read: the declaration's file
//! and the target. A changed file re-derives only the documents (and passages) over it; a
//! changed declaration re-derives every document of that source.

use async_trait::async_trait;
use ikigai_core::{
    ArgSpec, Description, Endpoint, Error, Invocation, Iri, OutputFrom, Representation, Result,
    Verb,
};
use oxrdf::{NamedOrBlankNode, Term};

use crate::{
    check_name, read_turtle, source_file, source_iri, source_scope, FS_READ, SOURCE_READ,
    XSD_STRING,
};

/// The IRI template a source's document is served at.
pub const DOCUMENT_TEMPLATE: &str = "urn:knowledge:document:source:{s}:id:{d}";
/// The IRI prefix of [`DOCUMENT_TEMPLATE`]: a source's name follows it.
pub const DOCUMENT_PREFIX: &str = "urn:knowledge:document:source:";
/// The document endpoint's description id.
pub const DOCUMENT_ID: &str = "knowledge-document";
/// The term a source declaration states where its documents live with: an IRI that a
/// document's id is appended to.
pub const DOCUMENT_BASE: &str = "https://ikigai-rs.dev/ns/knowledge#documentBase";

/// The name of the document `id` of the source `source`.
///
/// ```
/// assert_eq!(
///     ikigai_knowledge::document_iri("docs", "guide/intro.md"),
///     "urn:knowledge:document:source:docs:id:guide/intro.md"
/// );
/// ```
pub fn document_iri(source: &str, id: &str) -> String {
    format!("{DOCUMENT_PREFIX}{source}:id:{id}")
}

/// An id is a relative path under the base: segments that are not empty, `.` or `..`, with
/// no whitespace, control character, `?` or `#`, so it can only name something beneath it.
fn check_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && !id.starts_with('/')
        && id
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
        && !id
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '?' || c == '#');
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidArgument {
            name: "d".to_string(),
            detail: format!(
                "`{id}` is not a document id: name a path under the source's documentBase, \
                 such as guide/intro.md, with no empty, `.` or `..` segment, no whitespace and \
                 no `?` or `#`"
            ),
        })
    }
}

/// The parts of an alias IRI, if `iri` is one: the source's name and the document's id.
pub(crate) fn split_alias(iri: &str) -> Option<(&str, &str)> {
    let rest = iri.strip_prefix(DOCUMENT_PREFIX)?;
    let (source, id) = rest.split_once(":id:")?;
    check_name("name", source).ok()?;
    check_id(id).ok()?;
    Some((source, id))
}

/// Read the source `name`'s declaration: its one `documentBase`.
async fn read_base(inv: &Invocation<'_>, name: &str) -> Result<String> {
    let file = source_file(name)?;
    let path = format!("knowledge/source/{name}.ttl");
    let base = source_iri(name);
    let what = format!("source `{name}`");
    let triples = read_turtle(inv, &file, &base, &what, &path).await?;
    let stated: Vec<&Term> = triples
        .iter()
        .filter(|t| {
            matches!(&t.subject, NamedOrBlankNode::NamedNode(s) if s.as_str() == base)
                && t.predicate.as_str() == DOCUMENT_BASE
        })
        .map(|t| &t.object)
        .collect();
    match stated.as_slice() {
        [Term::NamedNode(n)] => Ok(n.as_str().to_string()),
        _ => Err(Error::Endpoint(format!(
            "{what} holds no documents: {path} must state `<> <{DOCUMENT_BASE}> <iri>` exactly \
             once, the IRI a document's id is appended to, such as \
             <urn:knowledge:file:path:docs/>"
        ))),
    }
}

/// The endpoint at [`DOCUMENT_TEMPLATE`].
pub(crate) struct DocumentEndpoint;

#[async_trait]
impl Endpoint for DocumentEndpoint {
    async fn invoke(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let name = inv
            .bindings
            .get("s")
            .ok_or_else(|| Error::MissingArgument("s".to_string()))?;
        let id = inv
            .bindings
            .get("d")
            .ok_or_else(|| Error::MissingArgument("d".to_string()))?;
        check_name("name", name).map_err(|e| match e {
            Error::InvalidArgument { detail, .. } => Error::InvalidArgument {
                name: "s".to_string(),
                detail,
            },
            e => e,
        })?;
        // The source's rule, before anything is read: a caller who may not read the source
        // learns nothing about which documents it holds.
        if !inv.capability.allows(&source_scope(name)) {
            return Err(Error::Denied(format!(
                "reading the documents of the source `{name}` takes `{}`",
                source_scope(name)
            )));
        }
        check_id(id)?;
        let base = read_base(inv, name).await?;
        let target = format!("{base}{id}");
        let alias = document_iri(name, id);
        let iri = Iri::parse(target.as_str()).map_err(|e| Error::InvalidArgument {
            name: "d".to_string(),
            detail: format!("`{id}` under the base <{base}> is not an IRI: {e}"),
        })?;
        let repr = match inv.source(&iri).await {
            Ok(repr) => repr,
            Err(Error::NotFound(why)) => {
                inv.depends_on(target.as_str());
                return Err(Error::NotFound(format!(
                    "no document `{alias}`: nothing at {target}: {why}"
                )));
            }
            Err(Error::Unresolved(_)) => {
                inv.depends_on(target.as_str());
                return Err(Error::NotFound(format!(
                    "no document `{alias}`: nothing serves {target} in this kernel; the \
                     source `{name}` names its documents under <{base}>, which the host binds"
                )));
            }
            Err(e) => return Err(e),
        };
        Ok(Representation::new(repr.repr_type.clone(), repr.bytes.to_vec()).cacheable())
    }

    fn describe(&self) -> Description {
        Description::new(DOCUMENT_ID)
            .title("Knowledge document")
            .summary(
                "A document of a source, by its id: an alias onto the resource its declaration \
                 names, <config home>/knowledge/source/{s}.ttl stating \
                 <> knowledge:documentBase <iri>, the document being that IRI followed by {d}. \
                 Answers the target's bytes in its media type, read through the kernel under \
                 the caller's capability. Takes urn:cap:knowledge:read:source:name:{s} and the \
                 declaration's file read. Cached under the declaration's and the target's \
                 threads.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(SOURCE_READ)
            .requires(FS_READ)
            .input(
                ArgSpec::new("s")
                    .summary("the source's name: the file knowledge/source/{s}.ttl")
                    .class(XSD_STRING)
                    .binding(),
            )
            .input(
                ArgSpec::new("d")
                    .summary("the document's id: a relative path under the source's base")
                    .class(XSD_STRING)
                    .binding(),
            )
            // A pass-through: the face is the target's, whatever the base's resource serves.
            .output_from(OutputFrom::Input("d".to_string()))
    }
}
