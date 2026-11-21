//! The work ledger as the first source: a corpus's named graphs, an entity, and a bounded
//! neighborhood, each composed from ikigai-store's NARROW doors under the caller's capability.
//!
//! The ledger's items, links, comments and labels are already RDF in ikigai-store, one named
//! graph per ledger (`urn:iki:ledger:graph:{name}`), so its lift is the identity and its
//! entities (items, people, repositories) are the IRIs it already uses: no model reads it.
//!
//! - **`urn:knowledge:graph:corpus:{c}`** lists the named graphs the corpus `{c}` holds that
//!   exist and that the caller may read.
//! - **`urn:knowledge:entity:id:{e}`** is every statement about `{e}`, or naming it, in those
//!   graphs, closed over `owl:sameAs`: one SPARQL CONSTRUCT.
//! - **`urn:knowledge:neighborhood:entity:{e}:depth:{n}`** is the statements within `{n}`
//!   hops of `{e}`, a breadth-first walk of SPARQL CONSTRUCTs bounded by a depth limit and a
//!   node budget.
//!
//! # Which graphs a corpus holds
//!
//! A corpus is a Turtle file, `<config home>/knowledge/corpus/{c}.ttl`, naming its sources
//! with `dcterms:hasPart`; each source declaration names the store graphs it covers the same
//! way. Both are read through the kernel, like a source declaration.
//!
//! # Authority
//!
//! All three take `urn:cap:knowledge:read:corpus:name:{c}`, held exactly, and read a source's
//! declaration only under `urn:cap:knowledge:read:source:name:{s}`: a source the caller may
//! not read contributes no graph, exactly as an undeclared one. The graphs are then
//! narrowed to those the caller holds `urn:cap:store:read:graph:<iri>` for BEFORE the store is
//! asked anything, and the store's own enumeration (`urn:iki:store:graphs`, existence and
//! may-read) narrows them again. So a graph the caller may not read is never named to the
//! store, never queried, and is absent from every answer exactly as an undeclared one is: a
//! caller learns nothing about a ledger it holds no token for.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use ikigai_core::{
    ArgRef, ArgSpec, Description, Endpoint, Error, Invocation, Iri, ReprType, Representation,
    Request, Result, Verb,
};
use oxrdf::{Literal, NamedNode, NamedOrBlankNode, Term, Triple};
use oxttl::{NTriplesParser, NTriplesSerializer, TurtleSerializer};

use crate::{
    check_name, read_turtle, source_scope, FILE_PREFIX, FS_READ, SOURCE_PREFIX, SOURCE_READ,
    XSD_STRING,
};

/// The IRI template a corpus's graph list is served at.
pub const GRAPH_TEMPLATE: &str = "urn:knowledge:graph:corpus:{c}";
/// The graph-list endpoint's description id.
pub const GRAPH_ID: &str = "knowledge-graph";
/// The entity endpoint's description id.
pub const ENTITY_ID: &str = "knowledge-entity";
/// The neighborhood endpoint's description id.
pub const NEIGHBORHOOD_ID: &str = "knowledge-neighborhood";
/// The IRI prefix of [`GRAPH_TEMPLATE`].
pub const GRAPH_PREFIX: &str = "urn:knowledge:graph:corpus:";
/// The IRI template an entity is served at.
pub const ENTITY_TEMPLATE: &str = "urn:knowledge:entity:id:{e}";
/// The IRI prefix of [`ENTITY_TEMPLATE`].
pub const ENTITY_PREFIX: &str = "urn:knowledge:entity:id:";
/// The IRI template a bounded neighborhood is served at.
pub const NEIGHBORHOOD_TEMPLATE: &str = "urn:knowledge:neighborhood:entity:{e}:depth:{n}";
/// The IRI prefix of [`NEIGHBORHOOD_TEMPLATE`].
pub const NEIGHBORHOOD_PREFIX: &str = "urn:knowledge:neighborhood:entity:";
/// A corpus's own name: this followed by the corpus name, the base `<>` resolves to in its file.
pub const CORPUS_PREFIX: &str = "urn:knowledge:corpus:name:";

/// The scope that reads the corpus called `{c}` is this followed by the name.
pub const CORPUS_READ_PREFIX: &str = "urn:cap:knowledge:read:corpus:name:";
/// What the corpus endpoints declare: some corpus read grant. The endpoint judges the name.
pub const CORPUS_READ: &str = "urn:cap:knowledge:read:corpus:name:*";
/// ikigai-store's narrow read of one named graph is this followed by the graph's IRI.
pub const STORE_GRAPH_READ_PREFIX: &str = "urn:cap:store:read:graph:";
/// What the corpus endpoints declare of the store: some named-graph read grant.
pub const STORE_GRAPH_READ: &str = "urn:cap:store:read:graph:*";

/// The deepest neighborhood served.
pub const MAX_DEPTH: u32 = 3;
/// The node budget a neighborhood takes when `nodes=` is not given.
pub const DEFAULT_NODES: usize = 100;
/// The largest node budget `nodes=` may ask for.
pub const MAX_NODES: usize = 1000;
/// The most statements an entity answers.
pub const ENTITY_STATEMENTS: usize = 1000;
/// The most statements one hop of a neighborhood asks the store for, per node of budget: a
/// hop asks for at most this times the budget (10,000 at the largest budget), in a stable
/// order, so a hub node shortens the walk instead of failing it on the store's own limits.
pub const HOP_STATEMENTS_PER_NODE: usize = 10;
/// The predicate an answer that was cut carries, on the answer's own IRI, with the value
/// `true`: an entity past [`ENTITY_STATEMENTS`], or a neighborhood past its node budget or a
/// hop's limit. A complete answer carries no such statement.
pub const TRUNCATED: &str = "https://ikigai-rs.dev/ns/knowledge#truncated";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const HAS_PART: &str = "http://purl.org/dc/terms/hasPart";
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const TURTLE: &str = "text/turtle";
const NTRIPLES: &str = "application/n-triples";
const TEXT: &str = "text/plain";

/// The scope that reads the corpus called `name`.
///
/// ```
/// assert_eq!(
///     ikigai_knowledge::corpus_scope("work"),
///     "urn:cap:knowledge:read:corpus:name:work"
/// );
/// ```
pub fn corpus_scope(name: &str) -> String {
    format!("{CORPUS_READ_PREFIX}{name}")
}

/// The file the corpus called `name` is read from, relative to the config home's `knowledge`
/// directory.
///
/// ```
/// assert_eq!(
///     ikigai_knowledge::corpus_file("work").unwrap(),
///     "urn:knowledge:file:path:corpus/work.ttl"
/// );
/// assert!(ikigai_knowledge::corpus_file("../elsewhere").is_err());
/// ```
pub fn corpus_file(name: &str) -> Result<String> {
    check_name("corpus", name)?;
    Ok(format!("{FILE_PREFIX}corpus/{name}.ttl"))
}

/// The named graphs the corpus `corpus` holds that exist and the caller may read, sorted.
async fn readable_graphs(inv: &Invocation<'_>, corpus: &str) -> Result<Vec<String>> {
    let file = corpus_file(corpus)?;
    // Before anything is read: a caller who may not read this corpus learns nothing about
    // whether it is declared.
    if !inv.capability.allows(&corpus_scope(corpus)) {
        return Err(Error::Denied(format!(
            "reading the corpus `{corpus}` takes `{}`",
            corpus_scope(corpus)
        )));
    }
    let base = format!("{CORPUS_PREFIX}{corpus}");
    let what = format!("corpus `{corpus}`");
    let path = format!("knowledge/corpus/{corpus}.ttl");
    let triples = read_turtle(inv, &file, &base, &what, &path).await?;
    let mut sources = BTreeSet::new();
    for object in parts(&triples, &base) {
        match object.strip_prefix(SOURCE_PREFIX) {
            Some(name) => {
                sources.insert(name.to_string());
            }
            None => {
                return Err(Error::Endpoint(format!(
                    "corpus `{corpus}`: {path} names <{object}> as a part, which is not a \
                     source; a corpus's parts are {SOURCE_PREFIX}{{name}}"
                )))
            }
        }
    }
    let mut declared = BTreeSet::new();
    for name in &sources {
        // A source the caller may not read contributes nothing, exactly as an undeclared
        // one: its declaration is never read.
        if !inv.capability.allows(&source_scope(name)) {
            continue;
        }
        let file = crate::source_file(name)?;
        let base = format!("{SOURCE_PREFIX}{name}");
        let what = format!("source `{name}` (a part of the corpus `{corpus}`)");
        let path = format!("knowledge/source/{name}.ttl");
        let triples = read_turtle(inv, &file, &base, &what, &path).await?;
        declared.extend(parts(&triples, &base));
    }
    // The caller's own grants first: a graph it may not read is never named to the store.
    let granted: Vec<String> = declared
        .into_iter()
        .filter(|g| {
            inv.capability
                .allows(&format!("{STORE_GRAPH_READ_PREFIX}{g}"))
        })
        .collect();
    if granted.is_empty() {
        return Ok(Vec::new());
    }
    // Then the store's: which of them exist, by the store's own enumeration.
    let request = Request::new(Verb::Source, iri("urn:iki:store:graphs")?);
    let listed = match inv.issue(request).await {
        Ok(repr) => String::from_utf8_lossy(&repr.bytes).into_owned(),
        Err(Error::Unresolved(_)) => {
            return Err(Error::NotFound(format!(
                "corpus `{corpus}`: no store answers urn:iki:store:graphs in this kernel; a \
                 host serves the ledger's graphs by binding ikigai-store beside this space"
            )))
        }
        Err(e) => return Err(e),
    };
    let existing: BTreeSet<&str> = listed.lines().map(str::trim).collect();
    Ok(granted
        .into_iter()
        .filter(|g| existing.contains(g.as_str()))
        .collect())
}

/// The IRIs `base` names with `dcterms:hasPart`.
fn parts(triples: &[Triple], base: &str) -> Vec<String> {
    triples
        .iter()
        .filter(|t| {
            t.predicate.as_str() == HAS_PART
                && matches!(&t.subject, NamedOrBlankNode::NamedNode(s) if s.as_str() == base)
        })
        .filter_map(|t| match &t.object {
            Term::NamedNode(o) => Some(o.as_str().to_string()),
            _ => None,
        })
        .collect()
}

fn iri(text: &str) -> Result<Iri> {
    Iri::parse(text).map_err(|e| Error::Endpoint(format!("{text}: {e}")))
}

/// A CONSTRUCT over `graphs` through `urn:iki:store:graph-construct`, as triples.
async fn construct(inv: &Invocation<'_>, graphs: &[String], query: &str) -> Result<Vec<Triple>> {
    let request = Request::new(Verb::Source, iri("urn:iki:store:graph-construct")?)
        .with_arg("graph", ArgRef::Inline(graphs.join(" ").into_bytes()))
        .with_arg("query", ArgRef::Inline(query.as_bytes().to_vec()))
        .with_arg("as", ArgRef::Inline(NTRIPLES.as_bytes().to_vec()));
    let repr = inv.issue(request).await?;
    NTriplesParser::new()
        .for_slice(&repr.bytes)
        .map(|t| t.map_err(|e| Error::Endpoint(format!("the store's N-Triples: {e}"))))
        .collect()
}

/// The corpus a request names with `corpus=`.
fn corpus_arg(inv: &Invocation<'_>) -> Result<String> {
    let corpus = inv
        .inline_str("corpus")
        .map_err(|_| Error::MissingArgument("corpus".to_string()))?
        .trim()
        .to_string();
    check_name("corpus", &corpus)?;
    Ok(corpus)
}

/// The entity `{e}` names: an absolute IRI.
fn entity_arg(inv: &Invocation<'_>) -> Result<NamedNode> {
    let e = inv
        .bindings
        .get("e")
        .ok_or_else(|| Error::MissingArgument("e".to_string()))?;
    let ok = e.contains(':') && !e.starts_with(':');
    NamedNode::new(e.to_string())
        .ok()
        .filter(|_| ok)
        .ok_or_else(|| Error::InvalidArgument {
            name: "e".to_string(),
            detail: format!(
                "`{e}` is not an entity: name it by its absolute IRI, such as \
                 urn:iki:ledger:default:item:{{id}}"
            ),
        })
}

/// The face a graph answer takes: Turtle by default, or N-Triples.
fn graph_face(inv: &Invocation<'_>, what: &str) -> Result<&'static str> {
    let Ok(asked) = inv.inline_str("as") else {
        return Ok(TURTLE);
    };
    match ikigai_core::media_type_essence(asked.trim()).as_str() {
        "" | TURTLE => Ok(TURTLE),
        NTRIPLES => Ok(NTRIPLES),
        other => Err(Error::InvalidArgument {
            name: "as".to_string(),
            detail: format!(
                "`{other}` is not a face of {what}: ask for text/turtle (the default) or \
                 application/n-triples"
            ),
        }),
    }
}

/// The statement that says the answer at `inv`'s IRI was cut.
fn truncated(inv: &Invocation<'_>) -> Result<Triple> {
    let answer = NamedNode::new(inv.request.target.as_str())
        .map_err(|e| Error::Endpoint(format!("the answer's IRI: {e}")))?;
    Ok(Triple::new(
        answer,
        NamedNode::new_unchecked(TRUNCATED),
        Literal::new_typed_literal("true", NamedNode::new_unchecked(XSD_BOOLEAN)),
    ))
}

/// The triples in a stable order, written in `media`.
fn write_graph(mut triples: Vec<Triple>, media: &'static str) -> Result<Representation> {
    triples.sort_by_key(|t| t.to_string());
    triples.dedup();
    let bytes = if media == NTRIPLES {
        let mut writer = NTriplesSerializer::new().for_writer(Vec::new());
        for t in &triples {
            writer
                .serialize_triple(t)
                .map_err(|e| Error::Endpoint(format!("writing N-Triples: {e}")))?;
        }
        writer.finish()
    } else {
        let failed = |e: std::io::Error| Error::Endpoint(format!("writing Turtle: {e}"));
        let mut writer = TurtleSerializer::new().for_writer(Vec::new());
        for t in &triples {
            writer.serialize_triple(t).map_err(failed)?;
        }
        writer.finish().map_err(failed)?
    };
    Ok(Representation::new(ReprType::new(media), bytes).cacheable())
}

fn corpus_input() -> ArgSpec {
    ArgSpec::new("corpus")
        .summary("the corpus whose graphs are read: the file knowledge/corpus/{corpus}.ttl")
        .class(XSD_STRING)
}

fn as_input() -> ArgSpec {
    ArgSpec::new("as")
        .summary("the face: text/turtle (the default) or application/n-triples")
        .class(XSD_STRING)
        .optional()
}

/// The endpoint at [`GRAPH_TEMPLATE`].
pub(crate) struct GraphEndpoint;

#[async_trait]
impl Endpoint for GraphEndpoint {
    async fn invoke(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let corpus = inv
            .bindings
            .get("c")
            .ok_or_else(|| Error::MissingArgument("c".to_string()))?
            .to_string();
        let graphs = readable_graphs(inv, &corpus).await?;
        let body: String = graphs.iter().map(|g| format!("{g}\n")).collect();
        Ok(Representation::new(ReprType::new(TEXT), body.into_bytes()).cacheable())
    }

    fn describe(&self) -> Description {
        Description::new(GRAPH_ID)
            .title("Knowledge corpus graphs")
            .summary(
                "The named graphs of a corpus that exist and that you may read: one IRI per \
                 line, sorted, empty when there are none. The corpus is the file \
                 knowledge/corpus/{c}.ttl in the config home, naming its sources with \
                 dcterms:hasPart; each source names its store graphs the same way. Takes \
                 urn:cap:knowledge:read:corpus:name:{c}, and lists a graph only under \
                 urn:cap:store:read:graph:<iri>.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(CORPUS_READ)
            .requires(SOURCE_READ)
            .requires(FS_READ)
            .requires(STORE_GRAPH_READ)
            .input(
                ArgSpec::new("c")
                    .summary("the corpus's name: the file knowledge/corpus/{c}.ttl")
                    .class(XSD_STRING)
                    .binding(),
            )
            .output(TEXT)
    }
}

/// The endpoint at [`ENTITY_TEMPLATE`].
pub(crate) struct EntityEndpoint;

#[async_trait]
impl Endpoint for EntityEndpoint {
    async fn invoke(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let entity = entity_arg(inv)?;
        let media = graph_face(inv, "an entity")?;
        let corpus = corpus_arg(inv)?;
        let graphs = readable_graphs(inv, &corpus).await?;
        if graphs.is_empty() {
            return write_graph(Vec::new(), media);
        }
        // `{e}` is a NamedNode, which refuses every character that could close the IRI, so
        // its N-Triples form is a term and never syntax.
        let query = format!(
            "CONSTRUCT {{ ?s ?p ?o }} WHERE {{ SELECT DISTINCT ?s ?p ?o WHERE {{ \
               {entity} (<{OWL_SAME_AS}>|^<{OWL_SAME_AS}>)* ?x . \
               {{ ?x ?p ?o BIND(?x AS ?s) }} UNION {{ ?s ?p ?x BIND(?x AS ?o) }} \
             }} ORDER BY ?s ?p ?o LIMIT {} }}",
            ENTITY_STATEMENTS + 1
        );
        // One more than the limit is asked for, so a cut is known, never guessed.
        // DISTINCT makes the count exact; the sort makes which one falls lexical.
        let mut triples = construct(inv, &graphs, &query).await?;
        triples.sort_by_key(|t| t.to_string());
        if triples.len() > ENTITY_STATEMENTS {
            triples.truncate(ENTITY_STATEMENTS);
            triples.push(truncated(inv)?);
        }
        write_graph(triples, media)
    }

    fn describe(&self) -> Description {
        Description::new(ENTITY_ID)
            .title("Knowledge entity")
            .summary(
                "Every statement about an entity, or naming it, in the corpus's graphs you may \
                 read, closed over owl:sameAs: a SPARQL CONSTRUCT through ikigai-store's \
                 urn:iki:store:graph-construct. {e} is the entity's IRI. At most 1000 \
                 statements, in a stable order; an answer that was cut says so, with \
                 knowledge:truncated true on its own IRI. Takes \
                 urn:cap:knowledge:read:corpus:name:{corpus}; a graph you hold no urn:cap:store:read:graph:<iri> for is never read. text/turtle by \
                 default, as=application/n-triples.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(CORPUS_READ)
            .requires(SOURCE_READ)
            .requires(FS_READ)
            .requires(STORE_GRAPH_READ)
            .input(
                ArgSpec::new("e")
                    .summary("the entity's IRI, such as urn:iki:ledger:default:item:{id}")
                    .class(XSD_STRING)
                    .binding(),
            )
            .input(corpus_input())
            .input(as_input())
            .output(TURTLE)
            .output(NTRIPLES)
    }
}

/// The endpoint at [`NEIGHBORHOOD_TEMPLATE`].
pub(crate) struct NeighborhoodEndpoint;

#[async_trait]
impl Endpoint for NeighborhoodEndpoint {
    async fn invoke(&self, inv: &Invocation<'_>) -> Result<Representation> {
        let entity = entity_arg(inv)?;
        let depth = depth_arg(inv)?;
        let budget = nodes_arg(inv)?;
        let media = graph_face(inv, "a neighborhood")?;
        let corpus = corpus_arg(inv)?;
        let graphs = readable_graphs(inv, &corpus).await?;
        if graphs.is_empty() {
            return write_graph(Vec::new(), media);
        }
        let (mut triples, cut) = walk(inv, &graphs, entity, depth, budget).await?;
        if cut {
            triples.push(truncated(inv)?);
        }
        write_graph(triples, media)
    }

    fn describe(&self) -> Description {
        Description::new(NEIGHBORHOOD_ID)
            .title("Knowledge neighborhood")
            .summary(
                "The statements within {n} hops of an entity, in either direction, in the \
                 corpus's graphs you may read: a breadth-first walk of SPARQL CONSTRUCTs \
                 through ikigai-store's urn:iki:store:graph-construct. {n} is 1 to 3. The \
                 walk stops at a node budget (nodes=, 1 to 1000, default 100), keeping the \
                 statements of each hop in lexical order until the budget is spent; each \
                 hop asks for at most 10 statements per node of budget. An answer that was \
                 cut says so, with knowledge:truncated true on its own IRI. Takes \
                 urn:cap:knowledge:read:corpus:name:{corpus}; a graph you hold no \
                 urn:cap:store:read:graph:<iri> for is never read. text/turtle by default, \
                 as=application/n-triples.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .requires(CORPUS_READ)
            .requires(SOURCE_READ)
            .requires(FS_READ)
            .requires(STORE_GRAPH_READ)
            .input(
                ArgSpec::new("e")
                    .summary("the entity's IRI, such as urn:iki:ledger:default:item:{id}")
                    .class(XSD_STRING)
                    .binding(),
            )
            .input(
                ArgSpec::new("n")
                    .summary("the depth: 1 to 3 hops")
                    .class(XSD_STRING)
                    .binding(),
            )
            .input(corpus_input())
            .input(
                ArgSpec::new("nodes")
                    .summary("the node budget: 1 to 1000, default 100")
                    .class(XSD_STRING)
                    .optional(),
            )
            .input(as_input())
            .output(TURTLE)
            .output(NTRIPLES)
    }
}

fn depth_arg(inv: &Invocation<'_>) -> Result<u32> {
    let n = inv
        .bindings
        .get("n")
        .ok_or_else(|| Error::MissingArgument("n".to_string()))?;
    n.parse::<u32>()
        .ok()
        .filter(|d| (1..=MAX_DEPTH).contains(d))
        .ok_or_else(|| Error::InvalidArgument {
            name: "n".to_string(),
            detail: format!("`{n}` is not a depth: use a whole number from 1 to {MAX_DEPTH}"),
        })
}

fn nodes_arg(inv: &Invocation<'_>) -> Result<usize> {
    let Ok(asked) = inv.inline_str("nodes") else {
        return Ok(DEFAULT_NODES);
    };
    asked
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|b| (1..=MAX_NODES).contains(b))
        .ok_or_else(|| Error::InvalidArgument {
            name: "nodes".to_string(),
            detail: format!(
                "`{asked}` is not a node budget: use a whole number from 1 to {MAX_NODES}"
            ),
        })
}

/// The breadth-first walk: each hop is one CONSTRUCT over the frontier, at most
/// [`HOP_STATEMENTS_PER_NODE`] times the budget statements in a stable order, and a statement
/// is kept only while the nodes it brings in fit the budget. Answers the statements kept, and
/// whether anything was cut.
async fn walk(
    inv: &Invocation<'_>,
    graphs: &[String],
    entity: NamedNode,
    depth: u32,
    budget: usize,
) -> Result<(Vec<Triple>, bool)> {
    let hop_limit = budget * HOP_STATEMENTS_PER_NODE;
    let mut cut = false;
    let mut nodes: BTreeSet<NamedNode> = BTreeSet::from([entity.clone()]);
    let mut frontier = vec![entity];
    let mut kept: BTreeMap<String, Triple> = BTreeMap::new();
    for _ in 0..depth {
        if frontier.is_empty() {
            break;
        }
        let values: String = frontier.iter().map(|n| format!("{n} ")).collect();
        let query = format!(
            "CONSTRUCT {{ ?s ?p ?o }} WHERE {{ SELECT DISTINCT ?s ?p ?o WHERE {{ \
               {{ VALUES ?x {{ {values}}} ?x ?p ?o BIND(?x AS ?s) }} UNION \
               {{ VALUES ?x {{ {values}}} ?s ?p ?x BIND(?x AS ?o) }} \
             }} ORDER BY ?s ?p ?o LIMIT {} }}",
            hop_limit + 1
        );
        // One more than the limit is asked for, so a cut is known, never guessed.
        // DISTINCT makes the count exact; the sort makes which one falls lexical.
        let mut found = construct(inv, graphs, &query).await?;
        found.sort_by_key(|t| t.to_string());
        if found.len() > hop_limit {
            found.truncate(hop_limit);
            cut = true;
        }
        let mut next = Vec::new();
        for t in found {
            let mut new: Vec<NamedNode> = Vec::new();
            if let NamedOrBlankNode::NamedNode(s) = &t.subject {
                if !nodes.contains(s) {
                    new.push(s.clone());
                }
            }
            // A class is not a neighbor: every item is `a ledger:Item`, and walking through
            // the class would make each item's neighborhood the whole ledger.
            let typed = t.predicate.as_str() == RDF_TYPE;
            if let (Term::NamedNode(o), false) = (&t.object, typed) {
                if !nodes.contains(o) && !new.contains(o) {
                    new.push(o.clone());
                }
            }
            if nodes.len() + new.len() > budget {
                cut = true;
                continue;
            }
            for n in new {
                nodes.insert(n.clone());
                next.push(n);
            }
            kept.insert(t.to_string(), t);
        }
        frontier = next;
    }
    Ok((kept.into_values().collect(), cut))
}
