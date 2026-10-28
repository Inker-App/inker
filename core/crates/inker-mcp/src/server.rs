//! The rmcp server: tool definitions, dispatch, and agent-facing text.
//!
//! Every string built here goes to an agent. It may contain handles, tokens,
//! kinds, years, states and counts. It must never contain a real value, a
//! private-folder path, or a backend error's detail text.

use std::sync::Arc;

use inker_core::{DocState, Handle, ListEntry};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, MetaObject, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

use crate::backend::{Backend, Resolved};
use crate::handle::parse_handle_arg;
use crate::instructions::instructions;

/// `_meta` key Claude Code reads to keep a tool out of deferred tool search.
pub const ALWAYS_LOAD_META_KEY: &str = "anthropic/alwaysLoad";

/// Agent name recorded when the client did not identify itself.
pub const UNKNOWN_AGENT: &str = "unknown agent";

/// Knobs for the server. `always_load` exists so the cold-handle spike can
/// compare with and without `_meta["anthropic/alwaysLoad"]` on `resolve`.
#[derive(Debug, Clone, Copy)]
pub struct ServerOptions {
    pub always_load: bool,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self { always_load: true }
    }
}

/// Errors from running the server over a transport.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("mcp: {0}")]
    Mcp(String),
}

/// Arguments of `list`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListArgs {}

/// Arguments of `resolve` and `describe`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct HandleArgs {
    /// The handle, e.g. `🐙/w2-acme-2025` or `ink/w2-acme-2025`.
    pub handle: String,
}

/// Arguments of `write`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteArgs {
    /// The handle of the document this output is derived from.
    pub handle: String,
    /// File name for the output, e.g. `summary.md`.
    pub name: String,
    /// The text to save, using tokens like `[SSN_1]` where real values belong.
    pub content: String,
}

/// One row as shown to agents: the handle rendered with its prefix.
#[derive(Debug, Serialize)]
struct EntryView {
    handle: String,
    kind: String,
    year: Option<u16>,
    state: DocState,
    inked: u32,
    is_folder: bool,
}

impl From<&ListEntry> for EntryView {
    fn from(e: &ListEntry) -> Self {
        Self {
            handle: e.handle.to_string(),
            kind: e.kind.as_str().to_string(),
            year: e.year,
            state: e.state,
            inked: e.inked,
            is_folder: e.is_folder,
        }
    }
}

/// The MCP server. Cheap to clone; one per connection.
#[derive(Clone)]
pub struct InkerServer {
    backend: Arc<dyn Backend>,
    options: ServerOptions,
}

impl std::fmt::Debug for InkerServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InkerServer")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

fn schema<T: JsonSchema + 'static>() -> Arc<rmcp::model::JsonObject> {
    rmcp::handler::server::common::schema_for_type::<T>()
}

/// The tool definitions, in the order `tools/list` returns them.
pub fn tool_definitions(options: ServerOptions) -> Vec<Tool> {
    let read_only = ToolAnnotations::new().read_only(true);
    let mut resolve = Tool::new(
        "resolve",
        "Read a private document by its handle (🐙/… or ink/…). Returns the text with sensitive \
         values replaced by stable tokens like [SSN_1], or a list of child handles for a folder.",
        schema::<HandleArgs>(),
    )
    .with_title("Resolve an INKER handle")
    .with_annotations(read_only.clone());
    if options.always_load {
        let mut meta = serde_json::Map::new();
        meta.insert(ALWAYS_LOAD_META_KEY.into(), Value::Bool(true));
        resolve = resolve.with_meta(MetaObject(meta));
    }
    vec![
        Tool::new(
            "list",
            "List every document and folder INKER holds, with kind, year, state and how many \
             values were inked.",
            schema::<ListArgs>(),
        )
        .with_title("List INKER handles")
        .with_annotations(read_only.clone()),
        resolve,
        Tool::new(
            "describe",
            "Explain what each token in a document stands for (for example [SSN_1]: employee) \
             and how each kind of value was treated. Never returns the values themselves.",
            schema::<HandleArgs>(),
        )
        .with_title("Describe an INKER document's tokens")
        .with_annotations(read_only),
        Tool::new(
            "write",
            "Save output derived from a document into the user's private folder. Write the \
             content with tokens like [SSN_1]; INKER fills in the real values when saving. \
             Returns a new handle for the saved file.",
            schema::<WriteArgs>(),
        )
        .with_title("Write a file with INKER")
        .with_annotations(ToolAnnotations::new().read_only(false).destructive(false)),
    ]
}

fn state_label(s: DocState) -> &'static str {
    match s {
        DocState::Ready => "ready",
        DocState::NeedsAttention => "pending review",
        DocState::Queued => "queued",
        DocState::Reading | DocState::Inking => "still being inked",
        DocState::Failed => "failed",
    }
}

fn entry_line(e: &ListEntry) -> String {
    let mut s = format!("{} · {}", e.handle, e.kind.as_str());
    if let Some(y) = e.year {
        s.push_str(&format!(" · {y}"));
    }
    s.push_str(&format!(" · {} · {} inked", state_label(e.state), e.inked));
    if e.is_folder {
        s.push_str(" · folder");
    }
    s
}

/// Map a backend error to the text the agent sees. Never includes the
/// backend's own message, which is not guaranteed to be value-free.
fn error_text(tool: &str, h: Option<&Handle>, e: &inker_core::Error) -> String {
    use inker_core::Error as E;
    let shown = h
        .map(|h| h.to_string())
        .unwrap_or_else(|| "that handle".into());
    match e {
        E::PendingReview(_) => format!(
            "pending review: the user has not finished reviewing {shown}. Ask them to answer in INKER, then try again."
        ),
        E::NotReady(_) => format!("not ready: {shown} is still being inked."),
        E::UnknownHandle(_) => {
            format!("unknown handle: {shown}. Call `list` to see the handles INKER holds.")
        }
        E::InvalidHandle(_) => "not an INKER handle. Handles look like 🐙/w2-acme-2025 or \
                                ink/w2-acme-2025; call `list` to see them."
            .to_string(),
        E::Refused(_) => format!(
            "refused: INKER would not complete `{tool}` because it would place real values somewhere an agent can see."
        ),
        _ => format!("INKER could not complete `{tool}`. The INKER app has the details."),
    }
}

fn variant(e: &inker_core::Error) -> &'static str {
    use inker_core::Error as E;
    match e {
        E::InvalidHandle(_) => "invalid_handle",
        E::UnknownHandle(_) => "unknown_handle",
        E::PendingReview(_) => "pending_review",
        E::NotReady(_) => "not_ready",
        E::Vault(_) => "vault",
        E::Index(_) => "index",
        E::Extract(_) => "extract",
        E::Detect(_) => "detect",
        E::Ingest(_) => "ingest",
        E::Refused(_) => "refused",
        E::Io(_) => "io",
        E::Json(_) => "json",
    }
}

fn ok(text: String, structured: Value) -> CallToolResult {
    let mut r = CallToolResult::success(vec![ContentBlock::text(text)]);
    r.structured_content = Some(structured);
    r
}

fn tool_error(text: String) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(text)])
}

fn args<T: for<'de> Deserialize<'de>>(p: &CallToolRequestParams) -> Result<T, McpError> {
    let v = Value::Object(p.arguments.clone().unwrap_or_default());
    serde_json::from_value(v)
        .map_err(|e| McpError::invalid_params(format!("bad arguments: {e}"), None))
}

fn agent_name(ctx: &RequestContext<RoleServer>) -> String {
    let from_meta = ctx.meta.client_info().map(|i| i.name);
    let from_init = || ctx.peer.peer_info().map(|i| i.client_info.name.clone());
    from_meta
        .or_else(from_init)
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| UNKNOWN_AGENT.to_string())
}

impl InkerServer {
    pub fn new(backend: Arc<dyn Backend>, options: ServerOptions) -> Self {
        Self { backend, options }
    }

    async fn blocking<T, F>(&self, f: F) -> Result<T, McpError>
    where
        T: Send + 'static,
        F: FnOnce(&dyn Backend) -> T + Send + 'static,
    {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || f(backend.as_ref()))
            .await
            .map_err(|_| McpError::internal_error("backend task failed", None))
    }

    async fn list(&self) -> Result<CallToolResult, McpError> {
        match self.blocking(|b| b.list()).await? {
            Ok(entries) => {
                let mut text = format!("{} handle(s):", entries.len());
                for e in &entries {
                    text.push('\n');
                    text.push_str(&entry_line(e));
                }
                let views: Vec<EntryView> = entries.iter().map(EntryView::from).collect();
                Ok(ok(text, json!({ "entries": views })))
            }
            Err(e) => Ok(self.fail("list", None, &e)),
        }
    }

    fn fail(&self, tool: &str, h: Option<&Handle>, e: &inker_core::Error) -> CallToolResult {
        tracing::info!(tool, error = variant(e), "tool call refused");
        tool_error(error_text(tool, h, e))
    }

    async fn resolve(&self, a: HandleArgs, agent: String) -> Result<CallToolResult, McpError> {
        let h = match parse_handle_arg(&a.handle) {
            Ok(h) => h,
            Err(e) => return Ok(self.fail("resolve", None, &e)),
        };
        let hh = h.clone();
        let res = self.blocking(move |b| b.resolve(&hh)).await?;
        let resolved = match res {
            Ok(r) => r,
            Err(e) => return Ok(self.fail("resolve", Some(&h), &e)),
        };
        let note = h.clone();
        self.blocking(move |b| b.note_read(&agent, &note)).await?;
        tracing::debug!(handle = %h, "resolved");
        Ok(match resolved {
            Resolved::Document(d) => {
                let mut text = format!("{} · {}", d.handle, d.kind.as_str());
                if let Some(y) = d.year {
                    text.push_str(&format!(" · {y}"));
                }
                text.push_str(
                    "\nSensitive values are replaced by stable tokens like [SSN_1]. \
                     Call `describe` for what each token stands for.",
                );
                for p in &d.pages {
                    text.push_str(&format!("\n\n--- page {} ---\n{}", p.page, p.text));
                }
                if !d.fields.is_empty() {
                    text.push_str("\n\n--- form fields ---");
                    for f in &d.fields {
                        text.push_str(&format!("\n{}: {}", f.name, f.value));
                    }
                }
                let structured = json!({
                    "type": "document",
                    "handle": d.handle.to_string(),
                    "kind": d.kind.as_str(),
                    "year": d.year,
                    "pages": d.pages,
                    "fields": d.fields,
                });
                ok(text, structured)
            }
            Resolved::Folder(f) => {
                let mut text = format!(
                    "{} is a folder with {} item(s). Resolve each one you need:",
                    f.handle,
                    f.children.len()
                );
                for c in &f.children {
                    text.push('\n');
                    text.push_str(&entry_line(c));
                }
                let children: Vec<EntryView> = f.children.iter().map(EntryView::from).collect();
                ok(
                    text,
                    json!({"type": "folder", "handle": f.handle.to_string(), "children": children}),
                )
            }
        })
    }

    async fn describe(&self, a: HandleArgs, agent: String) -> Result<CallToolResult, McpError> {
        let h = match parse_handle_arg(&a.handle) {
            Ok(h) => h,
            Err(e) => return Ok(self.fail("describe", None, &e)),
        };
        let hh = h.clone();
        let d = match self.blocking(move |b| b.describe(&hh)).await? {
            Ok(d) => d,
            Err(e) => return Ok(self.fail("describe", Some(&h), &e)),
        };
        let note = h.clone();
        self.blocking(move |b| b.note_read(&agent, &note)).await?;
        let mut text = format!("{} · {}", d.handle, d.kind.as_str());
        if let Some(y) = d.year {
            text.push_str(&format!(" · {y}"));
        }
        text.push_str(&format!(" · {}", state_label(d.state)));
        if d.roles.is_empty() {
            text.push_str("\nNo tokens.");
        } else {
            text.push_str("\nTokens:");
            for r in &d.roles {
                text.push_str(&format!("\n{}: {}", r.token, r.role));
            }
        }
        if !d.treatments.is_empty() {
            text.push_str("\nTreatments:");
            for t in &d.treatments {
                let how = match t.treatment {
                    inker_core::Treatment::Tokenize => "tokenize",
                    inker_core::Treatment::Generalize => "generalize",
                    inker_core::Treatment::Keep => "keep",
                };
                text.push_str(&format!("\n{}: {how} ({})", t.kind.label(), t.count));
            }
        }
        let roles: Vec<Value> = d
            .roles
            .iter()
            .map(|r| json!({"token": r.token.to_string(), "role": r.role}))
            .collect();
        let treatments: Vec<Value> = d
            .treatments
            .iter()
            .map(|t| json!({"kind": t.kind.label(), "treatment": t.treatment, "count": t.count}))
            .collect();
        Ok(ok(
            text,
            json!({
                "handle": d.handle.to_string(),
                "kind": d.kind.as_str(),
                "year": d.year,
                "state": d.state,
                "roles": roles,
                "treatments": treatments,
            }),
        ))
    }

    async fn write(&self, a: WriteArgs) -> Result<CallToolResult, McpError> {
        let h = match parse_handle_arg(&a.handle) {
            Ok(h) => h,
            Err(e) => return Ok(self.fail("write", None, &e)),
        };
        let (hh, name, content) = (h.clone(), a.name.clone(), a.content);
        let receipt = match self
            .blocking(move |b| b.write(&hh, &name, &content))
            .await?
        {
            Ok(r) => r,
            Err(e) => return Ok(self.fail("write", Some(&h), &e)),
        };
        let name = a.name.replace(['\n', '\r'], " ");
        let text = format!(
            "Saved {name} to the user's private folder as {}. Resolve that handle to see what was saved.",
            receipt.handle
        );
        Ok(ok(
            text,
            json!({"handle": receipt.handle.to_string(), "name": name}),
        ))
    }
}

impl ServerHandler for InkerServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("inker", env!("CARGO_PKG_VERSION")).with_title("INKER"),
            )
            .with_instructions(instructions())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(tool_definitions(
            self.options,
        )))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tool_definitions(self.options)
            .into_iter()
            .find(|t| t.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let agent = agent_name(&context);
        tracing::debug!(tool = %request.name, agent = %agent, "tool call");
        let result = match request.name.as_ref() {
            "list" => self.list().await?,
            "resolve" => self.resolve(args(&request)?, agent).await?,
            "describe" => self.describe(args(&request)?, agent).await?,
            "write" => self.write(args(&request)?).await?,
            other => {
                return Err(McpError::invalid_params(
                    format!("unknown tool: {other}"),
                    None,
                ));
            }
        };
        Ok(result.into())
    }
}

/// Serve one MCP session over a byte stream until it closes or `ct` fires.
pub async fn serve_stream<S>(
    backend: Arc<dyn Backend>,
    stream: S,
    options: ServerOptions,
    ct: CancellationToken,
) -> Result<(), ServeError>
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (r, w) = tokio::io::split(stream);
    serve_pair(backend, r, w, options, ct).await
}

/// Serve over a separate reader and writer (stdio is one of these).
pub async fn serve_pair<R, W>(
    backend: Arc<dyn Backend>,
    reader: R,
    writer: W,
    options: ServerOptions,
    ct: CancellationToken,
) -> Result<(), ServeError>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let server = InkerServer::new(backend, options);
    let running = match server.serve_with_ct((reader, writer), ct).await {
        Ok(r) => r,
        // A peer that connects and leaves before initializing is not an error.
        Err(rmcp::service::ServerInitializeError::ConnectionClosed(_))
        | Err(rmcp::service::ServerInitializeError::Cancelled) => return Ok(()),
        Err(e) => return Err(ServeError::Mcp(e.to_string())),
    };
    running
        .waiting()
        .await
        .map_err(|e| ServeError::Mcp(e.to_string()))?;
    Ok(())
}

/// Serve over this process's stdin and stdout.
pub async fn serve_stdio(
    backend: Arc<dyn Backend>,
    options: ServerOptions,
) -> Result<(), ServeError> {
    serve_pair(
        backend,
        tokio::io::stdin(),
        tokio::io::stdout(),
        options,
        CancellationToken::new(),
    )
    .await
}
