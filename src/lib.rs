//! Grounded, cited answers over an organization's documents, as ikigai resources.
//!
//! The README is this crate's contract: the whole resource map, which names exist today and
//! which are planned. Today one does:
//!
//! - **`urn:knowledge:source:name:{name}`**, a source declaration: which resources a corpus
//!   ingests, how they are lifted, and how much authority they carry. It is configuration, a
//!   Turtle file in the config home, read **through the kernel**: this crate opens no file.
//!
//! # Where a declaration lives, and who reads it
//!
//! The declaration called `{name}` is the file
//!
//! ```text
//! <config home>/knowledge/source/{name}.ttl
//! ```
//!
//! and the endpoint sources it as [`FILE_PREFIX`]`source/{name}.ttl`. The HOST decides what
//! answers there: it binds a file module at [`FILE_TEMPLATE`], jailed to the config home's
//! `knowledge` directory. With `ikigai-fs`:
//!
//! ```
//! use std::sync::Arc;
//! use ikigai_core::{Kernel, Space, UriTemplate};
//!
//! # let home = std::env::temp_dir().join(format!("ikigai-knowledge-doc-{}", std::process::id()));
//! let mount = ikigai_core::EndpointSpace::new().bind(
//!     UriTemplate::parse(ikigai_knowledge::FILE_TEMPLATE).unwrap(),
//!     ikigai_fs::FileEndpoint::new(home.join("knowledge")).cacheable(),
//! );
//! let spaces: Vec<Arc<dyn Space>> = vec![Arc::new(ikigai_knowledge::space()), Arc::new(mount)];
//! let kernel = Kernel::new(Arc::new(ikigai_core::Fallback::new(spaces)));
//! # let _ = kernel;
//! ```
//!
//! A host that binds nothing there serves no declaration, and every name is a not-found that
//! says so.
//!
//! # Authority
//!
//! Reading the declaration called `{name}` takes the scope
//! `urn:cap:knowledge:read:source:name:{name}`, held exactly; root holds every one. The
//! endpoint declares the family (`urn:cap:knowledge:read:source:name:*`), so the manifold offers
//! it only to a caller holding some source grant, and refuses any other name with a typed
//! `Denied` before it reads anything. The read of the file is the CALLER's too, so the
//! endpoint also declares `urn:cap:fs:read:*`, which the conventional mount (`ikigai-fs`)
//! judges by path.
//!
//! # Faces
//!
//! `text/plain` by default, and `text/turtle` with `as=text/turtle`. Both come from the parsed
//! declaration, with every IRI absolute: `<>` in the file names the source itself. A blank node
//! is refused, so the graph is diffable and every statement has a stable subject.
//!
//! The text face is a header line, then each subject in the order the file first names it,
//! with its statements indented beneath it:
//!
//! ```
//! use std::sync::Arc;
//! use ikigai_core::{
//!     ArgRef, Capability, Description, EndpointSpace, Fallback, FnEndpoint, Iri, Kernel,
//!     Representation, ReprType, Request, Space, UriTemplate, Verb,
//! };
//!
//! // Any endpoint may answer at the file template; here, one fixed declaration.
//! let file = FnEndpoint::new("file", |_| {
//!     Ok(Representation::new(
//!         ReprType::new("text/turtle"),
//!         b"<> <http://purl.org/dc/terms/title> \"The work ledger\" .".to_vec(),
//!     ))
//! })
//! .with_description(Description::new("file").verb(Verb::Source).output("text/turtle"));
//! let mount = EndpointSpace::new()
//!     .bind(UriTemplate::parse(ikigai_knowledge::FILE_TEMPLATE).unwrap(), file);
//! let spaces: Vec<Arc<dyn Space>> = vec![Arc::new(ikigai_knowledge::space()), Arc::new(mount)];
//! let kernel = Kernel::new(Arc::new(Fallback::new(spaces)));
//!
//! let read = |face: &str| {
//!     let request = Request::new(Verb::Source, Iri::parse("urn:knowledge:source:name:ledger").unwrap())
//!         .with_arg("as", ArgRef::Inline(face.as_bytes().to_vec()));
//!     let repr = futures::executor::block_on(kernel.issue(request, &Capability::root())).unwrap();
//!     (repr.repr_type.media_type.clone(), String::from_utf8(repr.bytes.to_vec()).unwrap())
//! };
//!
//! let (media, text) = read("text/plain");
//! assert_eq!(media, "text/plain");
//! assert_eq!(
//!     text,
//!     [
//!         "source ledger: 1 statement",
//!         "<urn:knowledge:source:name:ledger>",
//!         "  <http://purl.org/dc/terms/title> \"The work ledger\"",
//!         "",
//!     ]
//!     .join("\n")
//! );
//!
//! let (media, turtle) = read("text/turtle");
//! assert_eq!(media, "text/turtle");
//! assert!(turtle.contains("<urn:knowledge:source:name:ledger>"), "{turtle}");
//! ```
//!
//! # Golden thread
//!
//! The answer is `.cacheable()` and inherits the file's thread: over a cacheable mount it is
//! cached under the file's IRI, which a kernel write to the file, or a host's watcher over the
//! mount, cuts. A miss hangs from the same thread, so installing the declaration un-misses it.

use std::collections::BTreeMap;

mod graph;
mod passage;
mod trail;
pub use graph::{
    corpus_file, corpus_scope, CORPUS_PREFIX, CORPUS_READ, CORPUS_READ_PREFIX, DEFAULT_NODES,
    ENTITY_ID, ENTITY_PREFIX, ENTITY_STATEMENTS, ENTITY_TEMPLATE, GRAPH_ID, GRAPH_PREFIX,
    GRAPH_TEMPLATE, MAX_DEPTH, MAX_NODES, NEIGHBORHOOD_ID, NEIGHBORHOOD_PREFIX,
    NEIGHBORHOOD_TEMPLATE, STORE_GRAPH_READ, STORE_GRAPH_READ_PREFIX, TRUNCATED,
};
pub use passage::{
    chunk, chunking_scope, passage_iri, CHUNKING_ID, CHUNKING_PREFIX, CHUNKING_READ,
    CHUNKING_READ_PREFIX, CHUNKING_TEMPLATE, CHUNK_ID, CHUNK_PREFIX, CHUNK_TEMPLATE,
    MAX_CHARACTERS, MAX_PASSAGE, PASSAGE_ID, PASSAGE_PREFIX, PASSAGE_TEMPLATE,
};
pub use trail::{
    ABSTAINED, LLM_ASK, LLM_USE, MIN_QUOTE, REMOVED_CLAIMS, TRAIL_DEPTH, TRAIL_ENTITIES, TRAIL_ID,
    TRAIL_PREFIX, TRAIL_TEMPLATE,
};

use async_trait::async_trait;
use ikigai_core::{
    ArgSpec, Description, Endpoint, EndpointSpace, Error, Invocation, Iri, ReprType,
    Representation, Result, UriTemplate, Verb,
};
use oxrdf::{NamedOrBlankNode, Term, Triple};
use oxttl::{TurtleParser, TurtleSerializer};

/// The namespace of this module's own vocabulary, [`VOCABULARY`]: its terms are
/// `https://ikigai-rs.dev/ns/knowledge#{term}`, and the ontology's IRI is this without the `#`.
pub const VOCABULARY_NAMESPACE: &str = "https://ikigai-rs.dev/ns/knowledge#";

/// This module's own vocabulary, as Turtle (`src/vocabulary.ttl`): an `owl:Ontology` for
/// `https://ikigai-rs.dev/ns/knowledge` whose `owl:versionInfo` is the crate version, defining
/// every term under [`VOCABULARY_NAMESPACE`] the module emits ([`TRUNCATED`], [`ABSTAINED`]
/// and [`REMOVED_CLAIMS`]). Published
/// at that namespace once deployed.
pub const VOCABULARY: &str = include_str!("vocabulary.ttl");

/// The name [`space`] claims: `urn:iki:space:knowledge`.
pub const SPACE_ID: &str = "urn:iki:space:knowledge";

/// The IRI template a source declaration is served at.
pub const SOURCE_TEMPLATE: &str = "urn:knowledge:source:name:{name}";
/// The IRI prefix of [`SOURCE_TEMPLATE`]: a source's name follows it.
pub const SOURCE_PREFIX: &str = "urn:knowledge:source:name:";
/// The source endpoint's description id.
pub const SOURCE_ID: &str = "knowledge-source";

/// Where the source endpoint reads the config home's `knowledge` directory: the template a
/// host binds a file module at, jailed to that directory.
pub const FILE_TEMPLATE: &str = "urn:knowledge:file:path:{path}";
/// The IRI prefix of [`FILE_TEMPLATE`]: a file under the config home's `knowledge` directory
/// is this followed by its path relative to that directory.
pub const FILE_PREFIX: &str = "urn:knowledge:file:path:";

/// The scope that reads the declaration called `{name}` is this followed by the name.
pub const SOURCE_READ_PREFIX: &str = "urn:cap:knowledge:read:source:name:";
/// What the source endpoint declares: some source read grant. The endpoint judges the name.
pub const SOURCE_READ: &str = "urn:cap:knowledge:read:source:name:*";
/// What reading the declaration's file takes under `ikigai-fs`: some read grant, judged by
/// path where the file is served.
pub const FS_READ: &str = "urn:cap:fs:read:*";

pub(crate) const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const TEXT: &str = "text/plain";
const TURTLE: &str = "text/turtle";

/// The `urn:knowledge:*` space.
///
/// Configuration-free (no parameters, nothing read while it is built), so it names itself
/// [`SPACE_ID`]: every call holds the same doors. A host binds the file mount the source
/// endpoint reads through (see the crate docs) beside it, never inside it. The name goes on
/// LAST, since a later `bind` drops it.
pub fn space() -> EndpointSpace {
    EndpointSpace::new()
        .bind(
            UriTemplate::parse(SOURCE_TEMPLATE).expect("SOURCE_TEMPLATE is a valid template"),
            SourceEndpoint,
        )
        .bind(
            UriTemplate::parse(GRAPH_TEMPLATE).expect("GRAPH_TEMPLATE is a valid template"),
            graph::GraphEndpoint,
        )
        .bind(
            UriTemplate::parse(ENTITY_TEMPLATE).expect("ENTITY_TEMPLATE is a valid template"),
            graph::EntityEndpoint,
        )
        .bind(
            UriTemplate::parse(NEIGHBORHOOD_TEMPLATE)
                .expect("NEIGHBORHOOD_TEMPLATE is a valid template"),
            graph::NeighborhoodEndpoint,
        )
        .bind(
            UriTemplate::parse(TRAIL_TEMPLATE).expect("TRAIL_TEMPLATE is a valid template"),
            trail::TrailEndpoint,
        )
        .bind(
            UriTemplate::parse(CHUNKING_TEMPLATE).expect("CHUNKING_TEMPLATE is a valid template"),
            passage::ChunkingEndpoint,
        )
        .bind(
            UriTemplate::parse(CHUNK_TEMPLATE).expect("CHUNK_TEMPLATE is a valid template"),
            passage::ChunkEndpoint,
        )
        .bind(
            UriTemplate::parse(PASSAGE_TEMPLATE).expect("PASSAGE_TEMPLATE is a valid template"),
            passage::PassageEndpoint,
        )
        .named(ikigai_core::space_iri("knowledge"))
}

/// The scope that reads the declaration called `name`.
///
/// ```
/// assert_eq!(
///     ikigai_knowledge::source_scope("ledger"),
///     "urn:cap:knowledge:read:source:name:ledger"
/// );
/// ```
pub fn source_scope(name: &str) -> String {
    format!("{SOURCE_READ_PREFIX}{name}")
}

/// The IRI the declaration called `name` is read from, relative to the config home's
/// `knowledge` directory. It is also the golden thread the answer hangs from.
///
/// ```
/// assert_eq!(
///     ikigai_knowledge::source_file("ledger").unwrap(),
///     "urn:knowledge:file:path:source/ledger.ttl"
/// );
/// // A name is one path segment: nothing that could leave the source directory.
/// assert!(ikigai_knowledge::source_file("../elsewhere").is_err());
/// ```
pub fn source_file(name: &str) -> Result<String> {
    check_name("name", name)?;
    Ok(format!("{FILE_PREFIX}source/{name}.ttl"))
}

/// A name is one ordinary path segment, and a plain IRI segment. `arg` is the argument that
/// carries it: `name` for a source, `corpus` for a corpus.
pub(crate) fn check_name(arg: &str, name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && !name.starts_with('.')
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        let (kind, dir) = match arg {
            "name" => ("source", "source"),
            "chunking" => ("chunking rule", "chunking"),
            _ => ("corpus", "corpus"),
        };
        Err(Error::InvalidArgument {
            name: arg.to_string(),
            detail: format!(
                "`{name}` is not a {kind} name: use 1 to 64 letters, digits, `-`, `_` and `.`, \
                 not starting with `.` (the file knowledge/{dir}/{{name}}.ttl in the config home)"
            ),
        })
    }
}

/// The endpoint at [`SOURCE_TEMPLATE`].
struct SourceEndpoint;

#[async_trait]
impl Endpoint for SourceEndpoint {
    async fn invoke(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let name = inv
            .bindings
            .get("name")
            .ok_or_else(|| Error::MissingArgument("name".to_string()))?;
        let face = face(inv)?;
        let file = source_file(name)?;
        // The module's own rule, before anything is read: a caller who may not read this
        // source learns nothing about whether it is declared.
        if !inv.capability.allows(&source_scope(name)) {
            return Err(Error::Denied(format!(
                "reading the source `{name}` takes `{}`",
                source_scope(name)
            )));
        }
        let file_iri =
            Iri::parse(&file).map_err(|e| Error::Endpoint(format!("source `{name}`: {e}")))?;
        let repr = match inv.source(&file_iri).await {
            Ok(repr) => repr,
            // The miss is a fact about the FILE: hang it from the file's thread, so
            // installing the declaration (a write there, or a watcher's cut) un-misses it.
            Err(Error::NotFound(why)) => {
                inv.depends_on(file.as_str());
                return Err(Error::NotFound(format!(
                    "no source named `{name}` is declared: {why} (expected at \
                     knowledge/source/{name}.ttl in the config home, read as {file})"
                )));
            }
            Err(Error::Unresolved(_)) => {
                inv.depends_on(file.as_str());
                return Err(Error::NotFound(format!(
                    "no source named `{name}` is declared: nothing serves {file} in this \
                     kernel; a host serves declarations by binding a file module at \
                     {FILE_TEMPLATE} over the config home's knowledge directory"
                )));
            }
            Err(e) => return Err(e),
        };
        let declaration = Declaration::parse(name, &repr.bytes)?;
        let (media, bytes) = match face {
            Face::Text => (TEXT, declaration.to_text().into_bytes()),
            Face::Turtle => (TURTLE, declaration.to_turtle()?),
        };
        Ok(Representation::new(ReprType::new(media), bytes).cacheable())
    }

    fn describe(&self) -> Description {
        Description::new(SOURCE_ID)
            .title("Knowledge source declaration")
            .summary(
                "A source declaration found by name in the config home: \
                 <config home>/knowledge/source/{name}.ttl, which says what a corpus ingests, \
                 its lift and its authority. Read through the kernel from \
                 urn:knowledge:file:path:source/{name}.ttl, which the host binds over the \
                 config home, so the caller needs read authority there as well as \
                 urn:cap:knowledge:read:source:name:{name}. text/plain by default, \
                 as=text/turtle for the graph, with <> resolved to the source's own name. \
                 A name with nothing behind it is not-found; a malformed declaration, or one \
                 with a blank node, is refused.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(SOURCE_READ)
            .requires(FS_READ)
            .input(
                ArgSpec::new("name")
                    .summary(
                        "the source's name: the file knowledge/source/{name}.ttl in the config \
                         home",
                    )
                    .class(XSD_STRING)
                    .binding(),
            )
            .output(TEXT)
            .output(TURTLE)
    }
}

/// The two faces.
enum Face {
    Text,
    Turtle,
}

/// The face `as=` asks for: none, or `text/plain`, is the text face.
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
                "`{other}` is not a face of a source declaration: ask for text/plain (the \
                 default) or text/turtle"
            ),
        }),
    }
}

/// A parsed declaration: its statements in file order, and the prefixes it spelled.
struct Declaration {
    name: String,
    triples: Vec<Triple>,
    prefixes: BTreeMap<String, String>,
}

impl Declaration {
    fn parse(name: &str, bytes: &[u8]) -> Result<Declaration> {
        let base = source_iri(name);
        let mut parser = TurtleParser::new()
            .with_base_iri(base.as_str())
            .map_err(|e| Error::Endpoint(format!("source `{name}`: {e}")))?
            .for_slice(bytes);
        let mut triples = Vec::new();
        for triple in parser.by_ref() {
            let triple = triple.map_err(|e| {
                Error::Endpoint(format!(
                    "source `{name}`: knowledge/source/{name}.ttl is not valid Turtle: {e}"
                ))
            })?;
            let blank = matches!(triple.subject, NamedOrBlankNode::BlankNode(_))
                || matches!(triple.object, Term::BlankNode(_));
            if blank {
                return Err(Error::Endpoint(format!(
                    "source `{name}`: knowledge/source/{name}.ttl has a blank node in \
                     `{triple}`; name every resource with an IRI (`<>` is the source itself)"
                )));
            }
            triples.push(triple);
        }
        let prefixes = parser
            .prefixes()
            .map(|(p, iri)| (p.to_string(), iri.to_string()))
            .collect();
        Ok(Declaration {
            name: name.to_string(),
            triples,
            prefixes,
        })
    }

    fn to_text(&self) -> String {
        let count = self.triples.len();
        let noun = if count == 1 {
            "statement"
        } else {
            "statements"
        };
        let mut out = format!("source {}: {count} {noun}\n", self.name);
        // Subjects in the order the file first names them, each with its statements.
        let mut order: Vec<&NamedOrBlankNode> = Vec::new();
        for t in &self.triples {
            if !order.contains(&&t.subject) {
                order.push(&t.subject);
            }
        }
        for subject in order {
            out.push_str(&format!("{subject}\n"));
            for t in self.triples.iter().filter(|t| &t.subject == subject) {
                out.push_str(&format!("  {} {}\n", t.predicate, t.object));
            }
        }
        out
    }

    fn to_turtle(&self) -> Result<Vec<u8>> {
        let failed = |e: std::io::Error| {
            Error::Endpoint(format!("source `{}`: writing Turtle: {e}", self.name))
        };
        let mut serializer = TurtleSerializer::new();
        for (prefix, iri) in &self.prefixes {
            serializer = serializer.with_prefix(prefix, iri).map_err(|e| {
                Error::Endpoint(format!("source `{}`: prefix `{prefix}`: {e}", self.name))
            })?;
        }
        let mut writer = serializer.for_writer(Vec::new());
        for t in &self.triples {
            writer.serialize_triple(t).map_err(failed)?;
        }
        writer.finish().map_err(failed)
    }
}

/// The source's own IRI, the base `<>` resolves to.
fn source_iri(name: &str) -> String {
    format!("{SOURCE_PREFIX}{name}")
}

/// Read a Turtle file through the kernel and parse it against `base`, refusing a blank node.
/// `what` names the thing the file declares and `path` the file, for every sentence.
pub(crate) async fn read_turtle(
    inv: &Invocation<'_>,
    file: &str,
    base: &str,
    what: &str,
    path: &str,
) -> Result<Vec<Triple>> {
    let file_iri = Iri::parse(file).map_err(|e| Error::Endpoint(format!("{what}: {e}")))?;
    let repr = match inv.source(&file_iri).await {
        Ok(repr) => repr,
        Err(Error::NotFound(why)) => {
            inv.depends_on(file);
            return Err(Error::NotFound(format!(
                "no {what} is declared: {why} (expected at {path} in the config home, read as \
                 {file})"
            )));
        }
        Err(Error::Unresolved(_)) => {
            inv.depends_on(file);
            return Err(Error::NotFound(format!(
                "no {what} is declared: nothing serves {file} in this kernel; a host serves \
                 declarations by binding a file module at {FILE_TEMPLATE} over the config \
                 home's knowledge directory"
            )));
        }
        Err(e) => return Err(e),
    };
    let mut parser = TurtleParser::new()
        .with_base_iri(base)
        .map_err(|e| Error::Endpoint(format!("{what}: {e}")))?
        .for_slice(&repr.bytes);
    let mut triples = Vec::new();
    for triple in parser.by_ref() {
        let triple = triple
            .map_err(|e| Error::Endpoint(format!("{what}: {path} is not valid Turtle: {e}")))?;
        if matches!(triple.subject, NamedOrBlankNode::BlankNode(_))
            || matches!(triple.object, Term::BlankNode(_))
        {
            return Err(Error::Endpoint(format!(
                "{what}: {path} has a blank node in `{triple}`; name every resource with an IRI"
            )));
        }
        triples.push(triple);
    }
    Ok(triples)
}
