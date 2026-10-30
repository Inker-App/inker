//! inker-mcp: the MCP server INKER exposes to coding agents (stack-decision.md 3.6).
//!
//! Four tools: `list`, `resolve`, `describe`, `write`. The server talks to the
//! core through the [`Backend`] trait so it can be tested with [`FakeBackend`]
//! and wired to the real `inker_core` facade later. It runs over stdio or any
//! `AsyncRead + AsyncWrite` pair, which is how `inker-ipc` serves it on a unix
//! socket.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod backend;
pub mod client;
pub mod fake;
pub mod handle;
pub mod instructions;
pub mod server;
mod sessions;

pub use backend::{Backend, PendingReview, Resolved, ServerStatus, WriteReceipt};
pub use client::{ClientError, McpClient, ToolOutput};
pub use fake::FakeBackend;
pub use handle::parse_handle_arg;
pub use instructions::instructions;
pub use server::{
    ALWAYS_LOAD_META_KEY, CLI_TOOLS, INKER_CLI_CLIENT, InkerServer, ServeError, ServerOptions,
    UNKNOWN_AGENT, serve_pair, serve_pair_in, serve_stdio, serve_stream, serve_stream_in,
    tool_definitions,
};
pub use sessions::Sessions;
