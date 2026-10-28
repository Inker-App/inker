//! inker-ffi: the UniFFI façade the Swift shell calls. See
//! docs/stack-decision.md sections 3.1, 3.2 and 3.7.
//!
//! The shell talks to one [`CoreHandle`]. The handle forwards to a
//! [`Backend`]; today that is [`PreviewBackend`], which fakes the pipeline so
//! the app runs end to end with no core wired. The real `inker_core::Core`
//! slots in as another `Backend` implementation.
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
mod preview;
mod types;

pub use agents::{AgentKind, DoctorReport, Plan, PlanStep};
pub use backend::{Backend, ChangeSink};
pub use error::FfiError;
pub use handle::{CoreHandle, QueueListener};
pub use preview::{PreviewBackend, PreviewScenario, codename_from_name};
pub use types::{
    DocItem, DocState, FolderSummary, HandleStyle, IngestMode, Loader, QueueItem, ReadEvent,
    ReviewAnswer, ReviewQuestion, Status,
};

uniffi::setup_scaffolding!();
