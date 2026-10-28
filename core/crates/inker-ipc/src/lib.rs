//! inker-ipc: the unix socket between INKER.app and the `inker mcp` shims
//! (stack-decision.md 3.2).
//!
//! - [`host`] runs inside the app (or `inker serve`): it owns the socket and
//!   serves the MCP server from `inker-mcp` on every connection.
//! - [`shim`] is `inker mcp`: it forwards the agent's stdio to the socket byte
//!   for byte, launching the app first if it is not running.
//! - [`paths`] finds the data directory and socket path; the CLI reuses it.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod host;
pub mod paths;
pub mod shim;
