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
        let dir = std::env::temp_dir().join(format!(
            "ikigai-knowledge-{}-{}-{tag}",
            std::process::id(),
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
