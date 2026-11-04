//! Resources, prompts and completions: the ways a chat app's "+" menu can
//! reach an INKER document without the model having to pick a tool.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use common::{RawClient, text};
use inker_mcp::fake::FAKE_REAL_VALUES;
use inker_mcp::{Backend, FakeBackend, ServerOptions, serve_stream};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

type Client = RawClient<
    tokio::io::ReadHalf<tokio::io::DuplexStream>,
    tokio::io::WriteHalf<tokio::io::DuplexStream>,
>;

async fn ready() -> (Client, Arc<FakeBackend>) {
    let fake = Arc::new(FakeBackend::sample());
    let (client_end, server_end) = tokio::io::duplex(1 << 16);
    let backend: Arc<dyn Backend> = fake.clone();
    tokio::spawn(async move {
        let _ = serve_stream(
            backend,
            server_end,
            ServerOptions::default(),
            CancellationToken::new(),
        )
        .await;
    });
    let (r, w) = tokio::io::split(client_end);
    let mut c = RawClient::new(r, w);
    c.initialize("claude-ai").await;
    (c, fake)
}

fn assert_no_real_values(c: &Client) {
    for line in &c.transcript {
        for real in FAKE_REAL_VALUES {
            assert!(!line.contains(real), "leaked {real:?} in {line}");
        }
    }
}

async fn read(c: &mut Client, uri: &str) -> Value {
    c.request("resources/read", json!({ "uri": uri })).await
}

fn contents_text(r: &Value) -> String {
    r["result"]["contents"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn initialize_declares_resources_prompts_and_completions() {
    let fake = Arc::new(FakeBackend::sample());
    let (client_end, server_end) = tokio::io::duplex(1 << 16);
    let backend: Arc<dyn Backend> = fake;
    tokio::spawn(async move {
        let _ = serve_stream(
            backend,
            server_end,
            ServerOptions::default(),
            CancellationToken::new(),
        )
        .await;
    });
    let (r, w) = tokio::io::split(client_end);
    let mut c = RawClient::new(r, w);
    let init = c.initialize("claude-ai").await;
    let caps = &init["result"]["capabilities"];
    assert!(caps["resources"].is_object(), "{caps}");
    assert!(caps["prompts"].is_object(), "{caps}");
    assert!(caps["completions"].is_object(), "{caps}");
    assert!(caps["tools"].is_object(), "{caps}");
}

#[tokio::test]
async fn resources_list_holds_every_ready_document_and_folder() {
    let (mut c, _) = ready().await;
    let r = c.request("resources/list", json!({})).await;
    let resources = r["result"]["resources"].as_array().unwrap();
    let uris: Vec<&str> = resources
        .iter()
        .map(|x| x["uri"].as_str().unwrap())
        .collect();
    // The held 1099 and the 1098 still inking are not offered.
    assert_eq!(uris, ["inker:///w2-acme-2025", "inker:///taxes-2025"]);
    let w2 = &resources[0];
    assert_eq!(w2["name"], json!("🐙/w2-acme-2025"));
    assert_eq!(w2["title"], json!("🐙/w2-acme-2025"));
    assert_eq!(w2["description"], json!("w2 · 2025 · 4 inked"));
    assert_eq!(w2["mimeType"], json!("text/plain"));
    assert_eq!(
        resources[1]["description"],
        json!("folder · 2025 · 6 inked")
    );
    assert_no_real_values(&c);
}

#[tokio::test]
async fn reading_a_document_resource_is_what_resolve_returns() {
    let (mut c, fake) = ready().await;
    let r = read(&mut c, "inker:///w2-acme-2025").await;
    let contents = &r["result"]["contents"][0];
    assert_eq!(contents["uri"], json!("inker:///w2-acme-2025"));
    assert_eq!(contents["mimeType"], json!("text/plain"));
    let resolved = c
        .call("resolve", json!({"handle": "🐙/w2-acme-2025"}))
        .await;
    assert_eq!(contents_text(&r), text(&resolved));
    assert!(contents_text(&r).contains("[SSN_1]"));
    assert!(contents_text(&r).contains("58,250.00"));
    // Both reads are recorded for the board's last-read line.
    assert_eq!(fake.reads().len(), 2);
    assert_eq!(fake.reads()[0].0, "Claude");
    assert_no_real_values(&c);
}

#[tokio::test]
async fn reading_a_folder_resource_lists_its_children() {
    let (mut c, _) = ready().await;
    let r = read(&mut c, "inker:///taxes-2025").await;
    let resolved = c.call("resolve", json!({"handle": "🐙/taxes-2025"})).await;
    assert_eq!(contents_text(&r), text(&resolved));
    assert!(contents_text(&r).contains("🐙/1099-int-bank-2025"));
    assert_no_real_values(&c);
}

#[tokio::test]
async fn reading_a_held_document_gives_the_resolve_refusal() {
    let (mut c, fake) = ready().await;
    let r = read(&mut c, "inker:///1099-int-bank-2025").await;
    assert_eq!(
        r["error"]["message"],
        json!(
            "pending review: the user has not finished reviewing 🐙/1099-int-bank-2025. Ask them to answer in INKER, then try again."
        ),
        "{r}"
    );
    let r = read(&mut c, "inker:///1098-mortgage-2025").await;
    assert_eq!(
        r["error"]["message"],
        json!("not ready: 🐙/1098-mortgage-2025 is still being inked."),
        "{r}"
    );
    assert!(fake.reads().is_empty());
    assert_no_real_values(&c);
}

#[tokio::test]
async fn reading_an_unknown_or_foreign_uri_is_an_error() {
    let (mut c, _) = ready().await;
    let r = read(&mut c, "inker:///no-such-doc").await;
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown handle"),
        "{r}"
    );
    for uri in ["file:///etc/passwd", "inker:///../etc", "inker:///"] {
        let r = read(&mut c, uri).await;
        assert!(r["error"].is_object(), "{uri}: {r}");
        assert!(r["result"].is_null(), "{uri}: {r}");
    }
}

#[tokio::test]
async fn the_resource_template_is_one_ascii_scheme() {
    let (mut c, _) = ready().await;
    let r = c.request("resources/templates/list", json!({})).await;
    let templates = r["result"]["resourceTemplates"].as_array().unwrap();
    assert_eq!(templates.len(), 1, "{r}");
    assert_eq!(templates[0]["uriTemplate"], json!("inker:///{codename}"));
    assert_eq!(templates[0]["mimeType"], json!("text/plain"));
    let desc = templates[0]["description"].as_str().unwrap();
    assert!(desc.contains("🐙/") && desc.contains("INKER"), "{desc}");
}

#[tokio::test]
async fn the_read_prompt_takes_one_handle() {
    let (mut c, _) = ready().await;
    let r = c.request("prompts/list", json!({})).await;
    let prompts = r["result"]["prompts"].as_array().unwrap();
    assert_eq!(prompts.len(), 1, "{r}");
    let p = &prompts[0];
    assert_eq!(p["name"], json!("read"));
    assert_eq!(p["title"], json!("Read an INKER handle"));
    assert_eq!(
        p["arguments"],
        json!([{
            "name": "handle",
            "description": "The INKER handle, like 🐙/w2-acme-2025 or ink/w2-acme-2025.",
            "required": true
        }])
    );
}

#[tokio::test]
async fn the_read_prompt_is_one_user_message() {
    let (mut c, _) = ready().await;
    for given in ["🐙/w2-acme-2025", "ink/w2-acme-2025", " w2-acme-2025 "] {
        let r = c
            .request(
                "prompts/get",
                json!({"name": "read", "arguments": {"handle": given}}),
            )
            .await;
        let messages = r["result"]["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1, "{r}");
        assert_eq!(messages[0]["role"], json!("user"));
        assert_eq!(
            messages[0]["content"],
            json!({"type": "text", "text": "Read 🐙/w2-acme-2025 with INKER's `resolve` tool, then tell me what it contains. It is a private document handle managed by INKER, not a file path; do not look for it on disk. Keep tokens like [SSN_1] exactly as written."}),
            "{given}"
        );
    }
}

#[tokio::test]
async fn the_read_prompt_refuses_bad_input() {
    let (mut c, _) = ready().await;
    for params in [
        json!({"name": "read", "arguments": {}}),
        json!({"name": "read", "arguments": {"handle": "../etc/passwd"}}),
        json!({"name": "nope", "arguments": {"handle": "🐙/w2-acme-2025"}}),
    ] {
        let r = c.request("prompts/get", params.clone()).await;
        assert!(r["error"].is_object(), "{params}: {r}");
    }
}

async fn complete(c: &mut Client, reference: Value, name: &str, value: &str) -> Vec<String> {
    let r = c
        .request(
            "completion/complete",
            json!({"ref": reference, "argument": {"name": name, "value": value}}),
        )
        .await;
    r["result"]["completion"]["values"]
        .as_array()
        .unwrap_or_else(|| panic!("{r}"))
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn handle_completions_match_on_either_prefix() {
    let (mut c, _) = ready().await;
    let prompt = json!({"type": "ref/prompt", "name": "read"});
    assert_eq!(
        complete(&mut c, prompt.clone(), "handle", "🐙/1").await,
        ["🐙/1099-int-bank-2025", "🐙/1098-mortgage-2025"]
    );
    assert_eq!(
        complete(&mut c, prompt.clone(), "handle", "ink/w").await,
        ["ink/w2-acme-2025"]
    );
    assert_eq!(
        complete(&mut c, prompt.clone(), "handle", "tax").await,
        ["🐙/taxes-2025"]
    );
    assert_eq!(
        complete(&mut c, prompt.clone(), "handle", "").await.len(),
        4
    );
    assert!(
        complete(&mut c, prompt.clone(), "handle", "🐙/zzz")
            .await
            .is_empty()
    );
    assert!(complete(&mut c, prompt, "other", "🐙/").await.is_empty());
    let template = json!({"type": "ref/resource", "uri": "inker:///{codename}"});
    assert_eq!(
        complete(&mut c, template, "codename", "w").await,
        ["w2-acme-2025"]
    );
    assert_no_real_values(&c);
}
