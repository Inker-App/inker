//! inker-ffi: the UniFFI façade the Swift shell calls. See
//! docs/stack-decision.md sections 3.1, 3.2 and 3.7.
//!
//! The shell talks to one [`CoreHandle`]. The handle forwards to a
//! [`Backend`]: [`HostBackend`] over the real core (`inker_host::Host`), which
//! also serves the MCP socket, or [`PreviewBackend`], which fakes the pipeline
//! for SwiftUI previews. Swift supplies the Keychain, PDFKit and Vision
//! through the [`HostProviders`] callback interface.
//!
//! Nothing that crosses this boundary may carry a real sensitive value: the
//! review question carries a prompt, a location and the *length* of the value,
//! never the value.

// UniFFI's scaffolding macros expand to `extern "C"` functions with
// `#[no_mangle]` and raw-pointer handling; that is the whole point of the
// crate. The workspace warns on unsafe code, so it is allowed here only.
#![allow(unsafe_code)]

mod agents;
mod backend;
mod error;
mod handle;
mod host_backend;
mod preview;
mod providers;
mod types;

pub use agents::{AgentKind, DoctorReport, Plan, PlanStep};
pub use backend::{Backend, ChangeSink};
pub use error::FfiError;
pub use handle::{CoreHandle, QueueListener};
pub use host_backend::HostBackend;
pub use preview::{PreviewBackend, PreviewScenario, codename_from_name};
pub use providers::{ExtractedFfi, ExtractedField, ExtractedPage, HostProviders, ProviderError};
pub use types::{
    Board, BoardEntry, DocDetails, DocItem, DocState, FindMatch, FolderItem, FolderSummary,
    HandleStyle, IngestMode, Loader, MatchField, QueueItem, ReadEvent, ReviewAnswer,
    ReviewQuestion, Status, TagCount, YearGroup,
};

uniffi::setup_scaffolding!();
