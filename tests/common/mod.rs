//! A config home each test owns, and the kernel a host would build over it.

#![allow(dead_code)] // each test file uses a different part of this module

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ikigai_core::{
    ArgRef, Capability, EndpointSpace, Error, Fallback, Iri, Kernel, Representation, Request,
    Space, UriTemplate, Verb,
};

/// The declaration the tests install: two subjects, three statements, a prefix, and `<>`.
pub const LEDGER: &str = "@prefix dcterms: <http://purl.org/dc/terms/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<> dcterms:title \"The work ledger\" ;
   dcterms:source <urn:iki:ledger:item> .

<urn:iki:ledger:item> rdfs:label \"ledger items\" .
";

/// A scratch config home, removed when dropped.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(tag: &str) -> Scratch {
        // A per-process counter as well as the clock: macOS's clock ticks in microseconds, so
        // two tests with one tag starting together got ONE directory and removed it under
        // each other (seen as `changed while read` and on-disk-spelling refusals from the
        // file module, on macOS only).
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ikigai-knowledge-{}-{}-{}-{tag}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock is after the epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("knowledge/source")).expect("scratch dir");
        Scratch(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The config home's `knowledge` directory: what the host's file mount is jailed to.
    pub fn knowledge(&self) -> PathBuf {
        self.0.join("knowledge")
    }

    /// Declare a corpus, as a person would: write the file.
    pub fn declare_corpus(&self, name: &str, turtle: &str) {
        let dir = self.knowledge().join("corpus");
        std::fs::create_dir_all(&dir).expect("corpus dir");
        std::fs::write(dir.join(format!("{name}.ttl")), turtle).expect("write the corpus");
    }

    /// Declare a source, as a person would: write the file.
    pub fn declare(&self, name: &str, turtle: &str) {
        std::fs::write(
            self.knowledge().join("source").join(format!("{name}.ttl")),
            turtle,
        )
        .expect("write the declaration");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The host's file mount: `ikigai-fs`, cacheable, at the file template, jailed to the home's
/// `knowledge` directory.
pub fn mount(scratch: &Scratch) -> EndpointSpace {
    EndpointSpace::new().bind(
        UriTemplate::parse(ikigai_knowledge::FILE_TEMPLATE).expect("a valid template"),
        ikigai_fs::FileEndpoint::new(scratch.knowledge()).cacheable(),
    )
}

/// The kernel a host builds: the module's space, then the file mount beside it.
pub fn kernel_over(module: Arc<EndpointSpace>, scratch: &Scratch) -> Kernel {
    let spaces: Vec<Arc<dyn Space>> = vec![module, Arc::new(mount(scratch))];
    Kernel::new(Arc::new(Fallback::new(spaces)))
}

pub fn kernel(scratch: &Scratch) -> Kernel {
    kernel_over(Arc::new(ikigai_knowledge::space()), scratch)
}

pub fn source(name: &str, face: Option<&str>) -> Request {
    let request = Request::new(
        Verb::Source,
        Iri::parse(format!("{}{name}", ikigai_knowledge::SOURCE_PREFIX)).expect("a valid IRI"),
    );
    match face {
        Some(face) => request.with_arg("as", ArgRef::Inline(face.as_bytes().to_vec())),
        None => request,
    }
}

pub fn issue(kernel: &Kernel, request: Request, cap: &Capability) -> Result<Representation, Error> {
    futures::executor::block_on(kernel.issue(request, cap))
}

pub fn text(repr: &Representation) -> String {
    String::from_utf8(repr.bytes.to_vec()).expect("UTF-8")
}

/// A capability that reads the source `name` and the files under `scratch`'s mount.
pub fn reader(scratch: &Scratch, name: &str) -> Capability {
    Capability::scoped([
        ikigai_knowledge::source_scope(name),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
    ])
}

/// The kernel a host builds with a store beside the module and the file mount.
pub fn kernel_with_store(scratch: &Scratch, store: ikigai_store::DurableStore) -> Kernel {
    kernel_with_store_over(Arc::new(ikigai_knowledge::space()), scratch, store)
}

pub fn kernel_with_store_over(
    module: Arc<EndpointSpace>,
    scratch: &Scratch,
    store: ikigai_store::DurableStore,
) -> Kernel {
    let spaces: Vec<Arc<dyn Space>> = vec![
        module,
        Arc::new(mount(scratch)),
        Arc::new(ikigai_store::space(store)),
    ];
    Kernel::new(Arc::new(Fallback::new(spaces)))
}

/// A request with inline arguments.
pub fn request(verb: Verb, iri: &str, args: &[(&str, &str)]) -> Request {
    let mut request = Request::new(verb, Iri::parse(iri).expect("a valid IRI"));
    for (name, value) in args {
        request = request.with_arg(*name, ArgRef::Inline(value.as_bytes().to_vec()));
    }
    request
}

// ------------------------------------------------------------------ the work ledger

pub const DEFAULT: &str = "urn:iki:ledger:graph:default";
pub const ACME: &str = "urn:iki:ledger:graph:acme";
/// Declared by the source, granted to nobody below but root, and never created in the store.
pub const EMPTY: &str = "urn:iki:ledger:graph:empty";

pub const ITEM1: &str = "urn:iki:ledger:default:item:1";
pub const ITEM2: &str = "urn:iki:ledger:default:item:2";
pub const ITEM3: &str = "urn:iki:ledger:default:item:3";
pub const ALIAS: &str = "urn:kata:issue:01JZ";
pub const BRIAN: &str = "urn:iki:person:brian";
pub const SECRET: &str = "urn:iki:ledger:acme:item:9";

/// The ledger source: its own Turtle, so its lift is the identity.
pub const LEDGER_SOURCE: &str = "@prefix dcterms: <http://purl.org/dc/terms/> .

<> dcterms:title \"The work ledger\" ;
   dcterms:hasPart <urn:iki:ledger:graph:default>, <urn:iki:ledger:graph:acme>,
                   <urn:iki:ledger:graph:empty> .
";

pub const WORK: &str = "@prefix dcterms: <http://purl.org/dc/terms/> .

<> dcterms:title \"Our repositories, docs and ledger\" ;
   dcterms:hasPart <urn:knowledge:source:name:ledger> .
";

/// Items, a link, an author, an alias, and one item in another ledger that points INTO the
/// default ledger, so a walk that ignored the confinement would find it.
pub fn quads() -> String {
    let t = "<http://purl.org/dc/terms/title>";
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let item = "<https://ikigai-rs.dev/ns/ledger#Item>";
    let blocks = "<https://ikigai-rs.dev/ns/ledger#blocks>";
    let author = "<https://ikigai-rs.dev/ns/ledger#author>";
    let same = "<http://www.w3.org/2002/07/owl#sameAs>";
    [
        format!("<{ITEM1}> {ty} {item} <{DEFAULT}> ."),
        format!("<{ITEM1}> {t} \"Wire the ledger\" <{DEFAULT}> ."),
        format!("<{ITEM1}> {blocks} <{ITEM2}> <{DEFAULT}> ."),
        format!("<{ITEM2}> {ty} {item} <{DEFAULT}> ."),
        format!("<{ITEM2}> {t} \"Publish the store\" <{DEFAULT}> ."),
        format!("<{ITEM2}> {author} <{BRIAN}> <{DEFAULT}> ."),
        format!("<{ITEM3}> {ty} {item} <{DEFAULT}> ."),
        format!("<{ITEM3}> {author} <{BRIAN}> <{DEFAULT}> ."),
        format!("<{ALIAS}> {same} <{ITEM1}> <{DEFAULT}> ."),
        format!("<{ALIAS}> {t} \"Port the importer\" <{DEFAULT}> ."),
        format!("<{SECRET}> {t} \"Their Q4 migration\" <{ACME}> ."),
        format!("<{SECRET}> {blocks} <{ITEM1}> <{ACME}> ."),
    ]
    .join("\n")
        + "\n"
}

/// The ledger's example item, for the fixtures.
pub const ITEM: &str = ITEM1;

/// An in-memory store holding [`quads`].
pub fn seeded_store() -> ikigai_store::DurableStore {
    let store = ikigai_store::DurableStore::in_memory().expect("an in-memory store");
    let kernel = Kernel::new(Arc::new(ikigai_store::space(store.clone())));
    issue(
        &kernel,
        request(
            Verb::Sink,
            "urn:iki:store:load",
            &[("content", &quads()), ("format", "application/n-quads")],
        ),
        &Capability::root(),
    )
    .expect("seeding the store");
    drop(kernel);
    store
}

/// A capability that reads the corpus `name`, the files under `scratch`'s mount, and the
/// default ledger's graph.
pub fn corpus_reader(scratch: &Scratch, name: &str) -> Capability {
    Capability::scoped([
        ikigai_knowledge::corpus_scope(name),
        ikigai_knowledge::source_scope("ledger"),
        format!("urn:cap:fs:read:{}", scratch.knowledge().display()),
        format!("urn:cap:store:read:graph:{DEFAULT}"),
    ])
}

// ------------------------------------------------------------------ the decision trail

pub const DECISION: &str = "urn:iki:ledger:default:item:4";
pub const DECIDED: &str = "urn:iki:ledger:default:comment:4a";
pub const STILL_OPEN: &str = "urn:iki:ledger:default:comment:4b";
pub const ALICE: &str = "urn:iki:person:alice";
pub const ACME_DECISION: &str = "urn:iki:ledger:acme:item:10";
pub const ACME_COMMENT: &str = "urn:iki:ledger:acme:comment:10a";

/// [`quads`] plus a decision with two comments, and a same-titled item in acme's ledger.
pub fn trail_quads() -> String {
    let t = "<http://purl.org/dc/terms/title>";
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let l = "https://ikigai-rs.dev/ns/ledger#";
    let created = "<http://purl.org/dc/terms/created>";
    let date = |d: &str| format!("\"{d}\"^^<http://www.w3.org/2001/XMLSchema#dateTime>");
    quads()
        + &[
            format!("<{DECISION}> {ty} <{l}Item> <{DEFAULT}> ."),
            format!("<{DECISION}> {t} \"Adopt oxigraph as the store\" <{DEFAULT}> ."),
            format!("<{DECISION}> <{l}status> <{l}open> <{DEFAULT}> ."),
            format!("<{DECIDED}> {ty} <{l}Comment> <{DEFAULT}> ."),
            format!("<{DECIDED}> <{l}onItem> <{DECISION}> <{DEFAULT}> ."),
            format!("<{DECIDED}> <{l}author> <{BRIAN}> <{DEFAULT}> ."),
            format!(
                "<{DECIDED}> {created} {} <{DEFAULT}> .",
                date("2026-10-01T09:00:00Z")
            ),
            format!(
                "<{DECIDED}> <{l}body> \"Decided: we adopt oxigraph, since it already backs \
                 ikigai-store.\" <{DEFAULT}> ."
            ),
            format!("<{STILL_OPEN}> {ty} <{l}Comment> <{DEFAULT}> ."),
            format!("<{STILL_OPEN}> <{l}onItem> <{DECISION}> <{DEFAULT}> ."),
            format!("<{STILL_OPEN}> <{l}author> <{ALICE}> <{DEFAULT}> ."),
            format!(
                "<{STILL_OPEN}> {created} {} <{DEFAULT}> .",
                date("2026-10-02T09:00:00Z")
            ),
            format!(
                "<{STILL_OPEN}> <{l}body> \"Still open: the migration of old graphs is not \
                 scheduled.\" <{DEFAULT}> ."
            ),
            format!("<{ACME_DECISION}> {t} \"Adopt oxigraph at acme\" <{ACME}> ."),
            format!("<{ACME_COMMENT}> <{l}onItem> <{ACME_DECISION}> <{ACME}> ."),
            format!("<{ACME_COMMENT}> <{l}body> \"acme's private budget is 4M\" <{ACME}> ."),
        ]
        .join("\n")
        + "\n"
}

/// A stand-in for ikigai-llm's `urn:llm:ask`: answers `reply`, and records every prompt it
/// was given, so a test can see exactly what the model saw.
pub fn llm_stub(reply: &str) -> (EndpointSpace, Arc<std::sync::Mutex<Vec<String>>>) {
    use ikigai_core::{Description, FnEndpoint, ReprType};
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = seen.clone();
    let reply = reply.to_string();
    let ask = FnEndpoint::new("llm-stub", move |inv| {
        let prompt = inv.inline_str("prompt")?.to_string();
        let system = inv.inline_str("system")?.to_string();
        record
            .lock()
            .unwrap()
            .push(format!("{system}\n---\n{prompt}"));
        Ok(Representation::new(
            ReprType::new("text/plain"),
            reply.clone().into_bytes(),
        ))
    })
    .with_description(
        Description::new("llm-stub")
            .title("A model stub")
            .summary("Answers a fixed draft, as urn:llm:ask would.")
            .verb(Verb::Source)
            .output("text/plain"),
    );
    let space = EndpointSpace::new().bind(UriTemplate::parse("urn:llm:ask").unwrap(), ask);
    (space, seen)
}

/// The kernel a host builds with a store holding [`trail_quads`] and a model.
pub fn trail_kernel(scratch: &Scratch, llm: EndpointSpace) -> Kernel {
    let store = ikigai_store::DurableStore::in_memory().expect("an in-memory store");
    let loader = Kernel::new(Arc::new(ikigai_store::space(store.clone())));
    issue(
        &loader,
        request(
            Verb::Sink,
            "urn:iki:store:load",
            &[
                ("content", &trail_quads()),
                ("format", "application/n-quads"),
            ],
        ),
        &Capability::root(),
    )
    .expect("seeding the store");
    drop(loader);
    let spaces: Vec<Arc<dyn Space>> = vec![
        Arc::new(ikigai_knowledge::space()),
        Arc::new(mount(scratch)),
        Arc::new(ikigai_store::space(store)),
        Arc::new(llm),
    ];
    Kernel::new(Arc::new(Fallback::new(spaces)))
}
