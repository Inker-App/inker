//! `inker` CLI.
//!
//! `mcp` is the stdio shim agents launch: in that mode stdout is the MCP
//! channel, so logging goes to stderr only, everywhere, always.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand, ValueEnum};
use inker_agents::codex::CodexOptions;
use inker_agents::guard::{self, Decision};
use inker_agents::{Agent, DryRun, Env, Plan};
use inker_core::{DocState, Handle, HandleStyle, IngestMode};
use inker_host::{Host, HostConfig};
use inker_ipc::{host, paths, shim};
use inker_mcp::{Backend, FakeBackend, McpClient, ServerOptions};
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Parser)]
#[command(
    name = "inker",
    version,
    about = "INKER: let coding agents work on private documents without seeing the values"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Host the MCP server on the unix socket (the app does this; this is for development).
    Serve {
        /// Serve the built-in synthetic data set instead of the real core.
        #[arg(long)]
        fake: bool,
        /// Speak MCP on stdin/stdout instead of hosting the socket.
        #[arg(long)]
        stdio: bool,
        /// Do not mark `resolve` with `_meta["anthropic/alwaysLoad"]`.
        #[arg(long)]
        no_always_load: bool,
        #[arg(long)]
        socket: Option<PathBuf>,
        /// The private folder (default `~/Private`). Ignored with `--fake`.
        #[arg(long)]
        private: Option<String>,
    },
    /// The stdio shim agents launch: forwards MCP to the running app.
    Mcp {
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Print the MCP server instructions.
    Instructions,
    /// Configure agents: MCP registration, exclusion of the private folder, the teaching line.
    Setup {
        #[arg(long, value_enum)]
        agent: AgentArg,
        /// Show what would change without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// The private folder (default ~/Private).
        #[arg(long)]
        private: Option<String>,
        /// Codex only: make an "inker" permission profile the default. Replaces sandbox_mode.
        #[arg(long)]
        codex_permission_profile: bool,
    },
    /// Add the line that explains 🐙/ handles to ./AGENTS.md.
    Teach {
        #[arg(long)]
        dry_run: bool,
    },
    /// Report what is installed and configured for each agent.
    Doctor {
        #[arg(long)]
        private: Option<String>,
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Hook helper: reads the hook JSON on stdin, exits 2 if it touches the private folder.
    Guard {
        #[arg(long)]
        path: PathBuf,
        #[arg(long, value_enum, default_value = "claude")]
        format: GuardFormat,
    },
    /// Move (or copy) files into the private folder and ink them, without
    /// the app. Refuses while the app is serving, to keep one writer.
    Ingest {
        /// Files or folders to add.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// The private folder (default `~/Private`).
        #[arg(long)]
        private: Option<String>,
        /// Copy instead of move.
        #[arg(long)]
        copy: bool,
    },
    /// List handles held by the running app.
    List {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Show the tokenized view of a handle.
    Resolve {
        handle: String,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Show what each token in a document stands for.
    Describe {
        handle: String,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        socket: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AgentArg {
    Claude,
    Cursor,
    Codex,
    All,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum GuardFormat {
    /// Claude Code and Codex: exit 2 with the reason on stderr.
    Claude,
    /// Cursor: a JSON permission object on stdout.
    Cursor,
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("INKER_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    // Colour only on a terminal: logs piped to a file must stay greppable.
    let ansi = std::io::IsTerminal::is_terminal(&std::io::stderr());
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(ansi)
        .with_env_filter(filter)
        .try_init();
}

fn main() -> ExitCode {
    init_logging();
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("inker: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")
}

fn run(cmd: Cmd) -> Result<ExitCode> {
    match cmd {
        Cmd::Serve {
            fake,
            stdio,
            no_always_load,
            socket,
            private,
        } => serve(fake, stdio, !no_always_load, socket, private),
        Cmd::Mcp { socket } => {
            let socket = socket.unwrap_or_else(paths::socket_path);
            match runtime()?.block_on(shim::run(&socket)) {
                Ok(()) => Ok(ExitCode::SUCCESS),
                Err(shim::ShimError::NotRunning) => Ok(ExitCode::FAILURE),
                Err(e) => Err(e.into()),
            }
        }
        Cmd::Instructions => {
            println!("{}", inker_mcp::instructions());
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Setup {
            agent,
            dry_run,
            private,
            codex_permission_profile,
        } => setup(
            agent,
            DryRun(dry_run),
            private,
            CodexOptions {
                permission_profile: codex_permission_profile,
            },
        ),
        Cmd::Teach { dry_run } => {
            let cwd = std::env::current_dir()?;
            let plan = inker_agents::teach_project(&cwd, DryRun(dry_run))?;
            print_plan("This project", &plan, dry_run);
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Doctor { private, socket } => doctor(private, socket),
        Cmd::Guard { path, format } => guard_cmd(&path, format),
        Cmd::Ingest {
            paths: files,
            private,
            copy,
        } => ingest(files, private, copy),
        Cmd::List { json, socket } => client_call(socket, "list", json!({}), json),
        Cmd::Resolve {
            handle,
            json,
            socket,
        } => client_call(socket, "resolve", json!({"handle": handle}), json),
        Cmd::Describe {
            handle,
            json,
            socket,
        } => client_call(socket, "describe", json!({"handle": handle}), json),
    }
}

fn utf8(p: PathBuf) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(p).map_err(|p| anyhow!("path is not UTF-8: {}", p.display()))
}

/// Open the real core: vault, index and key under the data dir, documents in
/// the private folder. The CLI uses the file key store; the app uses the
/// Keychain.
fn open_host(private: Option<String>) -> Result<Host> {
    let config = HostConfig {
        private_folder: utf8(resolve_private(private)?)?,
        data_dir: utf8(paths::data_dir())?,
        packs_dir: None,
    };
    Host::open(config).context("opening the core")
}

fn ingest(files: Vec<PathBuf>, private: Option<String>, copy: bool) -> Result<ExitCode> {
    if paths::socket_path().exists() {
        bail!(
            "INKER is serving on {}; drop the files on the app instead, or stop it and retry",
            paths::socket_path().display()
        );
    }
    let host = open_host(private)?;
    let files = files.into_iter().map(utf8).collect::<Result<Vec<_>>>()?;
    let mode = if copy {
        IngestMode::Copy
    } else {
        IngestMode::Move
    };
    let outcomes = host.ingest(&files, mode)?;
    let mut all_ok = true;
    for o in outcomes {
        let rec = host.core().document(o.id)?;
        let word = match rec.state {
            DocState::Ready => "ready",
            DocState::NeedsAttention => "needs you",
            DocState::Failed => "failed",
            DocState::Queued => "queued",
            DocState::Reading => "reading",
            DocState::Inking => "inking",
        };
        all_ok &= rec.state == DocState::Ready;
        let handle = Handle::new(rec.codename).render(HandleStyle::Octopus);
        match rec.status_note {
            Some(note) if rec.state != DocState::Ready => println!("{handle}\t{word}\t{note}"),
            _ => println!("{handle}\t{word}\t{} inked", rec.inked),
        }
    }
    Ok(if all_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn serve(
    fake: bool,
    stdio: bool,
    always_load: bool,
    socket: Option<PathBuf>,
    private: Option<String>,
) -> Result<ExitCode> {
    let backend: Arc<dyn Backend> = if fake {
        Arc::new(FakeBackend::sample())
    } else {
        Arc::new(open_host(private)?)
    };
    let options = ServerOptions { always_load };
    let rt = runtime()?;
    if stdio {
        rt.block_on(inker_mcp::serve_stdio(backend, options))?;
        return Ok(ExitCode::SUCCESS);
    }
    let socket = socket.unwrap_or_else(paths::socket_path);
    eprintln!("inker: serving on {}", socket.display());
    rt.block_on(host::serve_until_signal(
        &socket,
        backend,
        options,
        CancellationToken::new(),
    ))?;
    Ok(ExitCode::SUCCESS)
}

fn home() -> PathBuf {
    paths::home_dir()
}

fn resolve_private(arg: Option<String>) -> Result<PathBuf> {
    let raw = arg.unwrap_or_else(|| "~/Private".into());
    let p = if raw == "~" {
        home()
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(&raw)
    };
    Ok(if p.is_absolute() {
        p
    } else {
        std::env::current_dir()?.join(p)
    })
}

fn inker_bin() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("locating the inker binary")?;
    Ok(exe.canonicalize().unwrap_or(exe))
}

fn env_for(private: Option<String>) -> Result<Env> {
    Ok(Env::detect(
        &home(),
        &inker_bin()?,
        &resolve_private(private)?,
    ))
}

fn setup(
    agent: AgentArg,
    dry: DryRun,
    private: Option<String>,
    codex: CodexOptions,
) -> Result<ExitCode> {
    let env = env_for(private)?;
    let agents: &[Agent] = match agent {
        AgentArg::Claude => &[Agent::ClaudeCode],
        AgentArg::Cursor => &[Agent::Cursor],
        AgentArg::Codex => &[Agent::Codex],
        AgentArg::All => &Agent::ALL,
    };
    if dry.0 {
        println!("Dry run: nothing will be written.\n");
    }
    for a in agents {
        let plan = match a {
            Agent::ClaudeCode => inker_agents::claude::setup(&env, dry),
            Agent::Cursor => inker_agents::cursor::setup(&env, dry),
            Agent::Codex => inker_agents::codex::setup(&env, codex, dry),
        }
        .with_context(|| format!("setting up {}", a.name()))?;
        print_plan(a.name(), &plan, dry.0);
    }
    Ok(ExitCode::SUCCESS)
}

/// Lines only in `after`, prefixed `+`, and only in `before`, prefixed `-`.
fn change_lines(before: Option<&str>, after: &str) -> Vec<String> {
    let old: Vec<&str> = before.unwrap_or_default().lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let mut out: Vec<String> = old
        .iter()
        .filter(|l| !new.contains(l))
        .map(|l| format!("- {l}"))
        .collect();
    out.extend(
        new.iter()
            .filter(|l| !old.contains(l))
            .map(|l| format!("+ {l}")),
    );
    out
}

fn print_plan(title: &str, plan: &Plan, dry: bool) {
    println!("== {title} ==");
    if plan.is_empty() {
        println!("Nothing to change.");
    }
    let (edit, run) = if dry {
        ("would edit", "would run")
    } else {
        ("edited", "ran")
    };
    for e in &plan.edits {
        let new = if e.before.is_none() {
            " (new file)"
        } else {
            ""
        };
        println!("{edit} {}{new}: {}", e.path.display(), e.summary);
        for l in change_lines(e.before.as_deref(), &e.after) {
            println!("    {l}");
        }
    }
    for c in &plan.commands {
        println!("{run}: {}  ({})", c.display(), c.summary);
    }
    for w in &plan.warnings {
        println!("warning: {w}");
    }
    println!();
}

fn doctor(private: Option<String>, socket: Option<PathBuf>) -> Result<ExitCode> {
    let socket = socket.unwrap_or_else(paths::socket_path);
    let running = std::os::unix::net::UnixStream::connect(&socket).is_ok();
    println!(
        "MCP: {} ({})",
        if running { "running" } else { "not running" },
        socket.display()
    );
    let env = env_for(private)?;
    println!("Private folder: {}", env.private.display());
    for s in inker_agents::doctor(&env) {
        let state = match (s.installed, s.registered) {
            (false, false) => "not installed".to_string(),
            (false, true) => "not installed, registered".to_string(),
            (true, r) => format!(
                "installed, {}",
                if r { "registered" } else { "not registered" }
            ),
        };
        println!("{}: {state}", s.agent.name());
        for (layer, on) in s.layers {
            println!("  {layer}: {}", if on { "yes" } else { "no" });
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn guard_cmd(private: &Path, format: GuardFormat) -> Result<ExitCode> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let decision = guard::decide(&input, private, &home());
    match format {
        GuardFormat::Claude => match decision {
            Decision::Allow => Ok(ExitCode::SUCCESS),
            Decision::Deny(reason) => {
                eprintln!("{reason}");
                Ok(ExitCode::from(2))
            }
        },
        GuardFormat::Cursor => {
            println!("{}", guard::cursor_response(&decision));
            Ok(if decision == Decision::Allow {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            })
        }
    }
}

fn client_call(
    socket: Option<PathBuf>,
    tool: &str,
    args: serde_json::Value,
    as_json: bool,
) -> Result<ExitCode> {
    let socket = socket.unwrap_or_else(paths::socket_path);
    runtime()?.block_on(async {
        let Ok(stream) = tokio::net::UnixStream::connect(&socket).await else {
            eprintln!("{}", shim::NOT_RUNNING_MESSAGE);
            return Ok(ExitCode::FAILURE);
        };
        let mut client = McpClient::connect(stream, "inker-cli").await?;
        let out = client.call_tool(tool, args).await?;
        if out.is_error {
            eprintln!("{}", out.text);
            return Ok(ExitCode::FAILURE);
        }
        match (as_json, out.structured) {
            (true, Some(v)) => println!("{}", serde_json::to_string_pretty(&v)?),
            _ => println!("{}", out.text),
        }
        Ok(ExitCode::SUCCESS)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_lines_marks_additions_and_removals() {
        assert_eq!(change_lines(Some("a\nb\n"), "a\nc\n"), ["- b", "+ c"]);
        assert_eq!(change_lines(None, "x\n"), ["+ x"]);
    }

    #[test]
    fn cli_parses() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
