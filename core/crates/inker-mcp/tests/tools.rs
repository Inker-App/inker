#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use common::{RawClient, is_error, text};
use inker_mcp::fake::FAKE_REAL_VALUES;
use inker_mcp::{Backend, FakeBackend, ServerOptions, serve_stream};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

type Client = RawClient<
    tokio::io::ReadHalf<tokio::io::DuplexStream>,
    tokio::io::WriteHalf<tokio::io::DuplexStream>,
>;

fn start(fake: Arc<FakeBackend>, options: ServerOptions) -> Client {
    let (client_end, server_end) = tokio::io::duplex(1 << 16);
    let backend: Arc<dyn Backend> = fake;
    tokio::spawn(async move {
        let _ = serve_stream(backend, server_end, options, CancellationToken::new()).await;
    });
    let (r, w) = tokio::io::split(client_end);
    RawClient::new(r, w)
}

async fn ready(name: &str) -> (Client, Arc<FakeBackend>) {
    let fake = Arc::new(FakeBackend::sample());
    let mut c = start(fake.clone(), ServerOptions::default());
    c.initialize(name).await;
    (c, fake)
}

fn assert_no_real_values(c: &Client) {
    for line in &c.transcript {
        for real in FAKE_REAL_VALUES {
            assert!(!line.contains(real), "leaked {real:?} in {line}");
        }
    }
}

#[tokio::test]
async fn initialize_returns_instructions() {
    let fake = Arc::new(FakeBackend::sample());
    let mut c = start(fake, ServerOptions::default());
    let r = c.initialize("claude-code").await;
    assert_eq!(
        r["result"]["instructions"],
        json!(inker_mcp::instructions())
    );
    assert_eq!(r["result"]["serverInfo"]["name"], json!("inker"));
    assert!(r["result"]["capabilities"]["tools"].is_object());
}

#[tokio::test]
async fn lists_four_tools_with_annotations_and_always_load() {
    let (mut c, _) = ready("claude-code").await;
    let r = c.request("tools/list", json!({})).await;
    let tools = r["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["list", "resolve", "describe", "write"]);
    let by = |n: &str| {
        tools
            .iter()
            .find(|t| t["name"] == json!(n))
            .unwrap()
            .clone()
    };
    for n in ["list", "resolve", "describe"] {
        assert_eq!(by(n)["annotations"]["readOnlyHint"], json!(true), "{n}");
    }
    assert_eq!(by("write")["annotations"]["readOnlyHint"], json!(false));
    assert_eq!(by("resolve")["_meta"]["anthropic/alwaysLoad"], json!(true));
    assert_eq!(by("resolve")["inputSchema"]["required"], json!(["handle"]));
    assert_eq!(
        by("write")["inputSchema"]["required"],
        json!(["handle", "name", "content"])
    );
}

#[tokio::test]
async fn always_load_can_be_turned_off() {
    let fake = Arc::new(FakeBackend::sample());
    let mut c = start(fake, ServerOptions { always_load: false });
    c.initialize("x").await;
    let r = c.request("tools/list", json!({})).await;
    let resolve = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == json!("resolve"))
        .unwrap()
        .clone();
    assert!(resolve.get("_meta").is_none());
}

#[tokio::test]
async fn list_tool_shows_every_handle() {
    let (mut c, _) = ready("claude-code").await;
    let r = c.call("list", json!({})).await;
    assert!(!is_error(&r));
    let t = text(&r);
    for h in [
        "🐙/w2-acme-2025",
        "🐙/1099-int-bank-2025",
        "🐙/1098-mortgage-2025",
        "🐙/taxes-2025",
    ] {
        assert!(t.contains(h), "{t}");
    }
    let rows = r["result"]["structuredContent"]["entries"]
        .as_array()
        .unwrap();
    assert_eq!(rows[0]["kind"], json!("w2"));
    assert_eq!(rows[0]["year"], json!(2025));
    assert_eq!(rows[0]["state"], json!("ready"));
    assert_eq!(rows[0]["inked"], json!(4));
    assert_eq!(rows[3]["is_folder"], json!(true));
    assert_no_real_values(&c);
}

#[tokio::test]
async fn resolve_document_in_all_handle_forms_and_records_the_read() {
    let (mut c, fake) = ready("claude-code").await;
    for h in ["🐙/w2-acme-2025", "ink/w2-acme-2025", "w2-acme-2025"] {
        let r = c.call("resolve", json!({"handle": h})).await;
        assert!(!is_error(&r), "{r}");
        let t = text(&r);
        assert!(t.contains("[SSN_1]") && t.contains("58,250.00"), "{t}");
        assert_eq!(r["result"]["structuredContent"]["type"], json!("document"));
    }
    let reads = fake.reads();
    assert_eq!(reads.len(), 3);
    assert!(
        reads
            .iter()
            .all(|(a, h)| a == "claude-code" && h.to_string() == "🐙/w2-acme-2025")
    );
    assert_no_real_values(&c);
}

#[tokio::test]
async fn resolve_folder_returns_listing() {
    let (mut c, _) = ready("codex").await;
    let r = c.call("resolve", json!({"handle": "🐙/taxes-2025"})).await;
    assert!(!is_error(&r));
    let t = text(&r);
    assert!(
        t.contains("🐙/w2-acme-2025") && t.contains("pending review"),
        "{t}"
    );
    assert_eq!(
        r["result"]["structuredContent"]["children"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn resolve_held_document_is_pending_review() {
    let (mut c, fake) = ready("claude-code").await;
    let r = c
        .call("resolve", json!({"handle": "ink/1099-int-bank-2025"}))
        .await;
    assert!(is_error(&r));
    assert_eq!(
        text(&r),
        "pending review: the user has not finished reviewing 🐙/1099-int-bank-2025. Ask them to answer in INKER, then try again."
    );
    assert!(fake.reads().is_empty());
}

#[tokio::test]
async fn resolve_inking_document_is_not_ready() {
    let (mut c, _) = ready("claude-code").await;
    let r = c
        .call("resolve", json!({"handle": "🐙/1098-mortgage-2025"}))
        .await;
    assert!(is_error(&r));
    assert_eq!(
        text(&r),
        "not ready: 🐙/1098-mortgage-2025 is still being inked."
    );
}

#[tokio::test]
async fn resolve_unknown_and_invalid_handles_are_tool_errors() {
    let (mut c, _) = ready("claude-code").await;
    let r = c.call("resolve", json!({"handle": "🐙/no-such-doc"})).await;
    assert!(is_error(&r));
    assert!(text(&r).contains("unknown handle"), "{r}");
    let r = c.call("resolve", json!({"handle": "../etc/passwd"})).await;
    assert!(is_error(&r));
    assert!(text(&r).contains("not an INKER handle"), "{r}");
}

#[tokio::test]
async fn describe_gives_roles_and_treatments() {
    let (mut c, fake) = ready("cursor").await;
    let r = c
        .call("describe", json!({"handle": "🐙/w2-acme-2025"}))
        .await;
    assert!(!is_error(&r));
    let t = text(&r);
    assert!(
        t.contains("[SSN_1]: employee") && t.contains("[EIN_1]: employer"),
        "{t}"
    );
    assert!(t.contains("generalize"), "{t}");
    assert_eq!(fake.reads()[0].0, "cursor");
    assert_no_real_values(&c);
}

#[tokio::test]
async fn write_saves_and_returns_new_handle_without_echoing_values() {
    let (mut c, fake) = ready("claude-code").await;
    let r = c
        .call(
            "write",
            json!({"handle": "🐙/w2-acme-2025", "name": "summary.md", "content": "[PERSON_1] earned 58,250.00; SSN [SSN_1]."}),
        )
        .await;
    assert!(!is_error(&r), "{r}");
    let t = text(&r);
    assert!(t.contains("🐙/w2-acme-2025-summary"), "{t}");
    assert!(!t.contains('\n'), "one line: {t}");
    assert!(!t.contains("[SSN_1]"), "content is not echoed: {t}");
    assert_eq!(
        r["result"]["structuredContent"]["handle"],
        json!("🐙/w2-acme-2025-summary")
    );
    assert!(
        fake.written("w2-acme-2025-summary")
            .unwrap()
            .contains(FAKE_REAL_VALUES[0])
    );
    // And it resolves back tokenized.
    let r = c
        .call("resolve", json!({"handle": "🐙/w2-acme-2025-summary"}))
        .await;
    assert!(text(&r).contains("[SSN_1]"));
    assert_no_real_values(&c);
}

#[tokio::test]
async fn missing_arguments_are_rejected() {
    let (mut c, _) = ready("claude-code").await;
    let r = c.call("resolve", json!({})).await;
    assert!(r.get("error").is_some() || is_error(&r), "{r}");
    let r = c.call("nope", json!({})).await;
    assert!(r.get("error").is_some() || is_error(&r), "{r}");
}

#[tokio::test]
async fn unknown_agent_when_client_info_is_absent() {
    // Stateless 2026-07-28 style: no initialize, client info only in _meta (here: absent).
    let fake = Arc::new(FakeBackend::sample());
    let mut c = start(fake.clone(), ServerOptions::default());
    c.initialize("").await;
    let _ = c
        .call("resolve", json!({"handle": "🐙/w2-acme-2025"}))
        .await;
    assert_eq!(fake.reads()[0].0, "unknown agent");
}

#[tokio::test]
async fn no_response_ever_contains_a_real_value() {
    let (mut c, _) = ready("claude-code").await;
    let calls: Vec<(&str, Value)> = vec![
        ("list", json!({})),
        ("resolve", json!({"handle": "🐙/w2-acme-2025"})),
        ("resolve", json!({"handle": "🐙/taxes-2025"})),
        ("resolve", json!({"handle": "🐙/1099-int-bank-2025"})),
        ("describe", json!({"handle": "🐙/w2-acme-2025"})),
        ("describe", json!({"handle": "🐙/taxes-2025"})),
        (
            "write",
            json!({"handle": "🐙/w2-acme-2025", "name": "a.txt", "content": "[SSN_1] [EIN_1] [PERSON_1] [ORG_1]"}),
        ),
        ("resolve", json!({"handle": "🐙/w2-acme-2025-a"})),
    ];
    for (tool, args) in calls {
        c.call(tool, args).await;
    }
    c.request("tools/list", json!({})).await;
    assert_no_real_values(&c);
}

#[tokio::test]
async fn stateless_2026_era_discover_and_call_with_meta_client_info() {
    let fake = Arc::new(FakeBackend::sample());
    let mut c = start(fake.clone(), ServerOptions::default());
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "claude-code", "version": "2.1.312"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    let d = c.request("server/discover", json!({"_meta": meta})).await;
    assert_eq!(
        d["result"]["instructions"],
        json!(inker_mcp::instructions()),
        "{d}"
    );
    let r = c
        .request(
            "tools/call",
            json!({"_meta": meta, "name": "resolve", "arguments": {"handle": "🐙/w2-acme-2025"}}),
        )
        .await;
    assert!(text(&r).contains("[SSN_1]"), "{r}");
    assert_eq!(fake.reads()[0].0, "claude-code");
}

/// Chat clients such as Claude Desktop reportedly ignore the server
/// `instructions`, so the tool descriptions themselves must tell the model
/// when to call `resolve` and what never to do.
#[test]
fn tool_descriptions_carry_the_handle_rule_for_clients_that_ignore_instructions() {
    let tools = inker_mcp::tool_definitions(inker_mcp::ServerOptions::default());
    let desc = |name: &str| -> String {
        tools
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("tool {name}"))
            .description
            .clone()
            .map(|d| d.to_string())
            .unwrap_or_default()
    };
    let resolve = desc("resolve");
    assert!(
        resolve.contains("🐙/") && resolve.contains("ink/"),
        "{resolve}"
    );
    assert!(
        resolve
            .to_lowercase()
            .contains("whenever the user's message contains")
            || resolve
                .to_lowercase()
                .contains("when the user's message contains"),
        "resolve must say when to call it: {resolve}"
    );
    assert!(resolve.to_lowercase().contains("never guess"), "{resolve}");
    assert!(
        resolve.to_lowercase().contains("do not look for the file"),
        "{resolve}"
    );
    let list = desc("list");
    assert!(list.to_lowercase().contains("handle"), "{list}");
    assert!(
        list.to_lowercase().contains("mention") || list.to_lowercase().contains("asks"),
        "list must say when: {list}"
    );
    let describe = desc("describe");
    assert!(describe.contains("[SSN_1]"), "{describe}");
    assert!(
        describe.to_lowercase().contains("after `resolve`")
            || describe.to_lowercase().contains("after resolve"),
        "{describe}"
    );
    let write = desc("write");
    assert!(
        write.to_lowercase().contains("never") && write.to_lowercase().contains("real value"),
        "{write}"
    );
}

/// The INKER-only tools the CLI uses. Never listed, and served only to a
/// connection whose `initialize` named the client `inker-cli`.
const HIDDEN: &[&str] = &["inker/review", "inker/answer", "inker/status"];

#[tokio::test]
async fn hidden_tools_are_never_listed_even_to_the_cli() {
    for client in ["claude-code", inker_mcp::INKER_CLI_CLIENT] {
        let (mut c, _) = ready(client).await;
        let r = c.request("tools/list", json!({})).await;
        let names: Vec<&str> = r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["list", "resolve", "describe", "write"], "{client}");
    }
}

#[tokio::test]
async fn hidden_tools_refuse_any_client_but_the_cli() {
    let (mut c, fake) = ready("claude-code").await;
    for tool in HIDDEN {
        let r = c
            .call(tool, json!({"question": "1", "answer": "leave"}))
            .await;
        // The same refusal as a tool that does not exist.
        let msg = r["error"]["message"].as_str().unwrap_or_default();
        assert_eq!(msg, format!("unknown tool: {tool}"), "{r}");
    }
    assert_eq!(fake.pending_reviews().unwrap().len(), 1, "nothing answered");

    // Stateless: client info only in a request's _meta is not enough.
    let fake = Arc::new(FakeBackend::sample());
    let mut c = start(fake, ServerOptions::default());
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "inker-cli", "version": "1"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    let r = c
        .request(
            "tools/call",
            json!({"_meta": meta, "name": "inker/review", "arguments": {}}),
        )
        .await;
    assert!(r.get("error").is_some(), "{r}");
}

#[tokio::test]
async fn the_cli_reviews_and_answers_without_seeing_the_value() {
    let (mut c, fake) = ready(inker_mcp::INKER_CLI_CLIENT).await;
    let r = c.call("inker/review", json!({})).await;
    assert!(!is_error(&r), "{r}");
    let qs = r["result"]["structuredContent"]["questions"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(qs.len(), 1);
    let q = &qs[0];
    assert_eq!(q["handle"], json!("🐙/1099-int-bank-2025"));
    assert_eq!(q["kind"], json!("account"));
    assert_eq!(q["page"], json!(1));
    assert_eq!(q["page_count"], json!(2));
    assert!(q["prompt"].as_str().unwrap().contains('?'));
    assert!(q.get("context_before").is_some() && q.get("context_after").is_some());
    let t = text(&r);
    assert!(
        t.contains("🐙/1099-int-bank-2025") && t.contains("page 1 of 2"),
        "{t}"
    );

    let id = q["id"].clone();
    let r = c
        .call("inker/answer", json!({"question": id, "answer": "ink"}))
        .await;
    assert!(!is_error(&r), "{r}");
    assert_eq!(r["result"]["structuredContent"]["state"], json!("ready"));
    assert!(fake.pending_reviews().unwrap().is_empty());
    let r = c
        .call("resolve", json!({"handle": "🐙/1099-int-bank-2025"}))
        .await;
    assert!(
        !is_error(&r),
        "held document is ready after the answer: {r}"
    );

    // Answering again, or a question that never existed, is a tool error.
    let r = c
        .call("inker/answer", json!({"question": id, "answer": "leave"}))
        .await;
    assert!(is_error(&r), "{r}");
    let r = c
        .call("inker/answer", json!({"question": 1, "answer": "maybe"}))
        .await;
    assert!(r.get("error").is_some() || is_error(&r), "{r}");
    assert_no_real_values(&c);
}

#[tokio::test]
async fn the_cli_reads_the_servers_private_folder() {
    let (mut c, _) = ready(inker_mcp::INKER_CLI_CLIENT).await;
    let r = c.call("inker/status", json!({})).await;
    assert!(!is_error(&r), "{r}");
    assert_eq!(
        r["result"]["structuredContent"]["private_folder"],
        json!(inker_mcp::fake::FAKE_PRIVATE_FOLDER)
    );
}
