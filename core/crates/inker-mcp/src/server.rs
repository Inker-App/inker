//! The rmcp server: tool definitions, dispatch, and agent-facing text.
//!
//! Every string built here goes to an agent. It may contain handles, tokens,
//! kinds, years, states and counts. It must never contain a real value, a
//! private-folder path, or a backend error's detail text.
//!
//! The exception is the INKER-only tools (`inker/review`, `inker/answer`,
//! `inker/status`) that the `inker` CLI uses to answer attention-card
//! questions and report the served folder without the app. They are never in
//! `tools/list`, and `tools/call` serves them only on a connection whose
//! `initialize` gave the client name [`INKER_CLI_CLIENT`]; to anyone else
//! they are an unknown tool. The name is a hiding rule, not authentication:
//! the socket is the user's own (0600), and an agent never chooses the client
//! name its harness sends. They still return no value, only prompts, kinds,
//! pages and the core's blanked context words; `inker/status` returns a path.

use std::future::Future;
use std::sync::Arc;

use inker_core::{DocState, Handle, ListEntry, QuestionId, ReviewAnswer};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    InitializeRequestParams, InitializeResult, ListToolsResult, MetaObject, PaginatedRequestParams,
    ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

use crate::backend::{Backend, PendingReview, Resolved};
use crate::handle::parse_handle_arg;
use crate::instructions::instructions;
use crate::sessions::Sessions;

/// `_meta` key Claude Code reads to keep a tool out of deferred tool search.
pub const ALWAYS_LOAD_META_KEY: &str = "anthropic/alwaysLoad";

/// The `initialize` client name the `inker` CLI sends. Only a connection
/// that sent it may call the INKER-only tools.
pub const INKER_CLI_CLIENT: &str = "inker-cli";

/// The INKER-only tools: never listed, served only to [`INKER_CLI_CLIENT`].
pub const CLI_TOOLS: &[&str] = &["inker/review", "inker/answer", "inker/status"];

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

/// The user's answer as the CLI sends it.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnswerWord {
    /// Ink it: the value is sensitive.
    Ink,
    /// Leave it: show the value, for this document only.
    Leave,
}

/// Arguments of `inker/answer`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnswerArgs {
    /// The question id from `inker/review`.
    pub question: i64,
    pub answer: AnswerWord,
}

/// One open question as the CLI shows it.
#[derive(Debug, Serialize)]
struct ReviewView {
    id: i64,
    handle: String,
    kind: String,
    page: u32,
    page_count: u32,
    prompt: String,
    context_before: String,
    context_after: String,
}

/// Prompt shown when the core stored none.
const DEFAULT_PROMPT: &str = "Ink it?";

impl From<&PendingReview> for ReviewView {
    fn from(q: &PendingReview) -> Self {
        Self {
            id: q.id.0,
            handle: q.handle.to_string(),
            kind: q.kind.label().to_ascii_lowercase(),
            page: q.page,
            page_count: q.page_count,
            prompt: q
                .prompt
                .clone()
                .unwrap_or_else(|| DEFAULT_PROMPT.to_string()),
            context_before: q.context_before.clone(),
            context_after: q.context_after.clone(),
        }
    }
}

impl ReviewView {
    /// `#12 · 🐙/scan-2025 · person · page 1 of 2`, the prompt, and the
    /// words around the held value with the value as `[ ? ]`.
    fn lines(&self) -> String {
        let page = if self.page_count > 0 {
            format!("page {} of {}", self.page, self.page_count)
        } else {
            format!("page {}", self.page)
        };
        let peek = format!(
            "{} [ ? ] {}",
            self.context_before.trim(),
            self.context_after.trim()
        );
        format!(
            "#{} · {} · {} · {page}\n  {}\n  {}",
            self.id,
            self.handle,
            self.kind,
            self.prompt,
            peek.trim()
        )
    }
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
    /// The live-session registry and this connection's id in it, when the
    /// host keeps one (the app does, for "Claude connected").
    session: Option<(Arc<Sessions>, u64)>,
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
        "Read a private document by its handle. Whenever the user's message contains a handle \
         like 🐙/w2-2025, ink/w2-2025 or 🐙/folder-0412, call this tool with it before \
         answering. A handle is not a path: there is no 🐙 folder on disk, so do not look for \
         the file with other tools or ask where it lives, and never guess what a handle refers \
         to. Returns the document's text with sensitive values replaced by stable tokens like \
         [SSN_1] (the same token always means the same value), or a list of child handles for \
         a folder handle.",
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
            "List every private document and folder INKER holds, as handles (🐙/… or ink/…) with \
             kind, year, state and how many values were inked. Use it when the user asks what \
             documents are available or mentions a document without giving its handle.",
            schema::<ListArgs>(),
        )
        .with_title("List INKER handles")
        .with_annotations(read_only.clone()),
        resolve,
        Tool::new(
            "describe",
            "Explain what each token in a document stands for (for example [SSN_1]: employee) \
             and how each kind of value was treated. Call it after `resolve` when a token's role \
             matters to the answer. Never returns the values themselves.",
            schema::<HandleArgs>(),
        )
        .with_title("Describe an INKER document's tokens")
        .with_annotations(read_only),
        Tool::new(
            "write",
            "Save output derived from a document into the user's private folder. Write the \
             content with tokens like [SSN_1]; INKER fills in the real values when saving, so \
             never write a real value yourself. Returns a new handle for the saved file.",
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

/// True when this connection's `initialize` named the client `inker-cli`.
/// Per-request `_meta` client info does not count: the gate is the
/// handshake only.
fn is_inker_cli(ctx: &RequestContext<RoleServer>) -> bool {
    ctx.peer
        .peer_info()
        .is_some_and(|i| i.client_info.name == INKER_CLI_CLIENT)
}

fn agent_name(ctx: &RequestContext<RoleServer>) -> String {
    let from_meta = ctx.meta.client_info().map(|i| i.name);
    let from_init = || ctx.peer.peer_info().map(|i| i.client_info.name.clone());
    display_agent_name(&from_meta.or_else(from_init).unwrap_or_default())
}

/// The product name for a client's `clientInfo.name`, for the popover's
/// "Claude read 🐙/w2-2025" line. Unknown clients keep their own name.
pub fn display_agent_name(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return UNKNOWN_AGENT.to_string();
    }
    let key: String = raw
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    match key.as_str() {
        "claudeai" | "claude" | "claudedesktop" => "Claude",
        "claudecode" => "Claude Code",
        "cursor" => "Cursor",
        "codex" | "codexcli" => "Codex",
        k if k.contains("chatgpt") => "ChatGPT",
        _ => raw,
    }
    .to_string()
}

impl InkerServer {
    pub fn new(backend: Arc<dyn Backend>, options: ServerOptions) -> Self {
        Self {
            backend,
            options,
            session: None,
        }
    }

    /// Register this connection in `sessions` under `id`; `initialize`
    /// names it there.
    pub fn in_sessions(mut self, sessions: Arc<Sessions>, id: u64) -> Self {
        self.session = Some((sessions, id));
        self
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

    async fn review(&self) -> Result<CallToolResult, McpError> {
        let qs = match self.blocking(|b| b.pending_reviews()).await? {
            Ok(qs) => qs,
            Err(e) => return Ok(self.fail("inker/review", None, &e)),
        };
        let views: Vec<ReviewView> = qs.iter().map(ReviewView::from).collect();
        let mut text = match views.len() {
            0 => "No open questions.".to_string(),
            1 => "1 open question:".to_string(),
            n => format!("{n} open questions:"),
        };
        for v in &views {
            text.push('\n');
            text.push_str(&v.lines());
        }
        Ok(ok(text, json!({ "questions": views })))
    }

    async fn answer(&self, a: AnswerArgs) -> Result<CallToolResult, McpError> {
        let id = QuestionId(a.question);
        let pending = match self.blocking(|b| b.pending_reviews()).await? {
            Ok(qs) => qs,
            Err(e) => return Ok(self.fail("inker/answer", None, &e)),
        };
        let Some(q) = pending.into_iter().find(|q| q.id == id) else {
            return Ok(tool_error(format!("no open question #{}", a.question)));
        };
        let answer = match a.answer {
            AnswerWord::Ink => ReviewAnswer::InkIt,
            AnswerWord::Leave => ReviewAnswer::LeaveIt,
        };
        let state = match self.blocking(move |b| b.answer_review(id, answer)).await? {
            Ok(s) => s,
            Err(e) => return Ok(self.fail("inker/answer", Some(&q.handle), &e)),
        };
        let what = match a.answer {
            AnswerWord::Ink => "inked",
            AnswerWord::Leave => "left as is",
        };
        let text = format!(
            "#{} {what}. {} is {}.",
            a.question,
            q.handle,
            state_label(state)
        );
        Ok(ok(
            text,
            json!({"question": a.question, "handle": q.handle.to_string(), "state": state}),
        ))
    }

    async fn status(&self) -> Result<CallToolResult, McpError> {
        match self.blocking(|b| b.status()).await? {
            Ok(s) => Ok(ok(
                format!("Private folder: {}", s.private_folder),
                json!({ "private_folder": s.private_folder }),
            )),
            Err(e) => Ok(self.fail("inker/status", None, &e)),
        }
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
    /// The default, plus naming this session after the client for the
    /// popover's "Claude connected".
    fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<InitializeResult, McpError>> + Send + '_ {
        context.peer.set_peer_info(request.clone());
        if let Some((sessions, id)) = &self.session {
            sessions.name(*id, display_agent_name(&request.client_info.name));
        }
        std::future::ready(self.negotiate_initialize(&request))
    }

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
            "inker/review" if is_inker_cli(&context) => self.review().await?,
            "inker/answer" if is_inker_cli(&context) => self.answer(args(&request)?).await?,
            "inker/status" if is_inker_cli(&context) => self.status().await?,
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

/// `serve_stream`, with the connection registered in `sessions`.
pub async fn serve_stream_in<S>(
    backend: Arc<dyn Backend>,
    stream: S,
    options: ServerOptions,
    sessions: Arc<Sessions>,
    ct: CancellationToken,
) -> Result<(), ServeError>
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (r, w) = tokio::io::split(stream);
    serve_pair_in(backend, r, w, options, Some(sessions), ct).await
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
    serve_pair_in(backend, reader, writer, options, None, ct).await
}

/// `serve_pair`, registering the connection in `sessions` when given one:
/// opened before the handshake, named by `initialize`, closed when the
/// connection ends however it ends.
pub async fn serve_pair_in<R, W>(
    backend: Arc<dyn Backend>,
    reader: R,
    writer: W,
    options: ServerOptions,
    sessions: Option<Arc<Sessions>>,
    ct: CancellationToken,
) -> Result<(), ServeError>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let mut server = InkerServer::new(backend, options);
    let registered = sessions.map(|s| {
        let id = s.open();
        (s, id)
    });
    if let Some((s, id)) = &registered {
        server = server.in_sessions(s.clone(), *id);
    }
    let result = serve_server(server, reader, writer, ct).await;
    if let Some((s, id)) = registered {
        s.close(id);
    }
    result
}

async fn serve_server<R, W>(
    server: InkerServer,
    reader: R,
    writer: W,
    ct: CancellationToken,
) -> Result<(), ServeError>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
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

#[cfg(test)]
mod tests {
    use super::display_agent_name;

    #[test]
    fn chat_clients_get_their_product_names() {
        assert_eq!(display_agent_name("claude-ai"), "Claude");
        assert_eq!(display_agent_name("Claude Desktop"), "Claude");
        assert_eq!(display_agent_name("claude-code"), "Claude Code");
        assert_eq!(display_agent_name("cursor"), "Cursor");
        assert_eq!(display_agent_name("codex"), "Codex");
        assert_eq!(display_agent_name("chatgpt"), "ChatGPT");
        assert_eq!(display_agent_name("openai-chatgpt"), "ChatGPT");
    }

    #[test]
    fn unknown_clients_keep_their_own_name() {
        assert_eq!(display_agent_name("my-client"), "my-client");
        assert_eq!(display_agent_name("  "), super::UNKNOWN_AGENT);
    }
}

#[cfg(test)]
mod tool_definition_tests {
    use super::*;

    /// Chat clients weigh tool descriptions more than server instructions,
    /// so the handle rule lives in both: a handle is not a path on disk.
    #[test]
    fn resolve_tells_the_model_a_handle_is_not_a_path() {
        let tools = tool_definitions(ServerOptions::default());
        let resolve = tools.iter().find(|t| t.name == "resolve").unwrap();
        let text = resolve.description.as_deref().unwrap_or_default();
        assert!(
            text.contains("🐙/folder-0412"),
            "a folder handle looks most like a path"
        );
        assert!(text.contains("A handle is not a path"));
        assert!(text.contains("do not look for the file"));
        let list = tools.iter().find(|t| t.name == "list").unwrap();
        assert!(
            list.description
                .as_deref()
                .unwrap_or_default()
                .contains("🐙/")
        );
    }
}
