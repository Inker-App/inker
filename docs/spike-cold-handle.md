# Spike: does an agent act on a `🐙/` handle cold?

Brief step 3 / stack-decision.md section 7, step 2. Run 2026-10-28 on `bennet` (Linux) by an agent session.

## Question

An agent gets a prompt containing `🐙/w2-acme-2025` and nothing else. Does it call INKER's `resolve` tool, or does it guess, search the disk, or give up? And how much does each kind of help change that?

- **a.** No CLAUDE.md, no `alwaysLoad` anywhere. Only the server `instructions` and tool descriptions.
- **b.** Like a, plus a project `CLAUDE.md` holding the one-line teaching block that `inker setup` writes.
- **c.** Like a, plus `"alwaysLoad": true` on the server in the client config (`.mcp.json`).
- **d.** Like a, but `resolve` carries `_meta["anthropic/alwaysLoad"]: true` in its tool definition. This is INKER's default.

Tool search was forced on (`ENABLE_TOOL_SEARCH=true`) in every condition. With only one 4-tool server, `auto` might not defer anything, and then a, b, c and d would all look the same.

## Setup

- Claude Code 2.1.312, `claude -p`, default model (the JSON reports `claude-opus-5-5`).
- `inker serve --fake` hosts (one with the tool `_meta`, one with `--no-always-load`) on sockets in `~/.cache/tmp/inker-spike/`. Each project's `.mcp.json` launches the real shim, `inker mcp --socket …`, so the whole path runs: stdio, then the shim, then the unix socket, then the rmcp server, then `FakeBackend`.
- The fake holds a synthetic W-2 (SSN in the 9xx area, unissued EIN prefix). Box 1 is `58,250.00`, and that value exists only in `resolve`'s output.
- Flags: `--strict-mcp-config --mcp-config .mcp.json` (only INKER, none of the user's servers), `--setting-sources project,local` (no user settings or hooks), `--allowedTools mcp__inker__resolve mcp__inker__describe mcp__inker__list`, `--no-session-persistence`, `--output-format json`.
- Prompt: `Check 🐙/w2-acme-2025 and tell me the wages in box 1`.
- `resolve` calls were counted from the host's debug log (`tool call tool=resolve agent=claude-code`), not inferred from the answer.

Nested `claude -p` worked inside the agent session; no auth or sandbox problems.

## Results

3 runs per condition, 12 runs in total.

| Cond | Run | `resolve` calls | Right answer (58,250.00) | Turns | Seconds | Cost (USD) |
|---|---|---|---|---|---|---|
| a | 1 | 1 | yes | 3 | 6.6 | 0.116 |
| a | 2 | 1 | yes | 3 | 5.1 | 0.116 |
| a | 3 | 1 | yes | 3 | 9.1 | 0.023 |
| b | 1 | 1 | yes | 3 | 7.3 | 0.115 |
| b | 2 | 1 | yes | 3 | 8.3 | 0.023 |
| b | 3 | 1 | yes | 3 | 3.6 | 0.027 |
| c | 1 | 1 | yes | 2 | 5.7 | 0.199 |
| c | 2 | 1 | yes | 2 | 3.1 | 0.017 |
| c | 3 | 1 | yes | 2 | 8.6 | 0.013 |
| d | 1 | 1 | yes | 2 | 3.5 | 0.195 |
| d | 2 | 1 | yes | 2 | 6.0 | 0.016 |
| d | 3 | 1 | yes | 2 | 3.6 | 0.012 |

No permission denials. No run tried `Read`, `Bash` or a file search. Several answers named the employer as `[ORG_1]`, keeping the token as the instructions ask. The first run of each condition pays the prompt-cache write, which explains the higher cost.

**Turns** tell the story. In a and b, `resolve` was deferred behind tool search: the model spent a turn on ToolSearch and then called `resolve`. In c and d, `resolve` was already loaded and was called on the first turn.

## Conclusion

1. **The handle plus the server `instructions` is enough for Claude Code.** 6 of 6 cold runs (a, b) resolved the handle correctly, with no CLAUDE.md help needed. The handle format is self-explanatory once the instructions are loaded.
2. **`alwaysLoad` saves a round trip; it does not change the outcome.** Both mechanisms work: the server-level `"alwaysLoad": true` in client config, and the tool-level `_meta["anthropic/alwaysLoad"]`. Claude Code 2.1.312's config schema documents both, and says the tool-level `_meta` can opt a tool out again. Keep the tool `_meta` on `resolve`; it costs nothing and needs no client config.
3. **The CLAUDE.md / AGENTS.md line made no measurable difference here.** Keep it anyway. It is cheap, it is the only teaching channel for Cursor (whose docs do not mention server instructions), and this spike used the easiest possible prompt.
4. **Onboarding copy can be light** for Claude Code. "Paste the handle into your agent" is enough; no "tell your agent about INKER" step is needed.

## Limits, and what to run next

- One prompt, and an easy one: it says "Check" right next to the handle. Still to try: a handle buried in a larger task ("draft my tax summary from 🐙/taxes-2025 and 🐙/w2-acme-2025"), a handle in a pasted file, and a folder handle.
- INKER was the only MCP server. Real users have many, and tool search then has more to choose from. Rerun without `--strict-mcp-config` on a realistic setup.
- `--setting-sources project,local` excludes user settings; it is not clear whether it also stops Ben's global `~/.claude/CLAUDE.md` loading. That file says nothing about INKER, so it should not bias the result.
- Codex and Cursor were not tested (not installed here). Codex honours `instructions` per its docs; Cursor's docs do not mention them.
- Linux, not macOS. Agent behaviour should not depend on the OS, but rerun on the Mac before trusting this for onboarding copy.

## Rerunning

Build first (`cd core && cargo build -p inker`), then:

```sh
CLAUDE_BIN=claude ./spike-cold-handle.sh "$PWD/core/target/debug/inker" 3
cat ~/.cache/tmp/inker-spike/results.tsv
```

Results go to `results.tsv` and `runs/<cond>-<run>.json` under `$SPIKE_DIR` (default `~/.cache/tmp/inker-spike`). The two hosts stop when the script exits. The script:

```bash
#!/usr/bin/env bash
# Cold-handle spike: does Claude Code call `resolve` on a 🐙/ handle it has
# never been told about? Four conditions x RUNS runs, all against the fake
# backend (synthetic data only).
#
#   a  no CLAUDE.md, no alwaysLoad anywhere
#   b  CLAUDE.md with the one-line teaching block, no alwaysLoad
#   c  no CLAUDE.md, "alwaysLoad": true on the server in .mcp.json
#   d  no CLAUDE.md, resolve carries _meta["anthropic/alwaysLoad"] (INKER's default)
#
# Usage: spike-cold-handle.sh [path/to/inker] [runs]
# Env:   CLAUDE_BIN (default: claude), SPIKE_DIR (default: ~/.cache/tmp/inker-spike)
set -euo pipefail

INKER=${1:-inker}
RUNS=${2:-3}
CLAUDE_BIN=${CLAUDE_BIN:-claude}
SPIKE=${SPIKE_DIR:-$HOME/.cache/tmp/inker-spike}
PROMPT='Check 🐙/w2-acme-2025 and tell me the wages in box 1'
mkdir -p "$SPIKE/runs"

# Two hosts: one marks resolve with _meta alwaysLoad, one does not.
SOCK_META="$SPIKE/meta.sock"
SOCK_PLAIN="$SPIKE/plain.sock"
INKER_LOG=inker_mcp=debug "$INKER" serve --fake --socket "$SOCK_META" 2>"$SPIKE/host-meta.log" &
HOST_META=$!
INKER_LOG=inker_mcp=debug "$INKER" serve --fake --no-always-load --socket "$SOCK_PLAIN" 2>"$SPIKE/host-plain.log" &
HOST_PLAIN=$!
trap 'kill $HOST_META $HOST_PLAIN 2>/dev/null || true' EXIT
for s in "$SOCK_META" "$SOCK_PLAIN"; do
  for _ in $(seq 100); do if [ -S "$s" ]; then break; fi; sleep 0.1; done
done

TEACH='<!-- inker -->
Strings that start with 🐙/ or ink/ are private document handles managed by INKER: call the `resolve` tool of the inker MCP server with the handle to read one, and never guess its contents or look for the file on disk.
<!-- /inker -->'

setup_project() { # cond -> project dir
  local cond=$1 dir="$SPIKE/proj-$1" sock="$SOCK_PLAIN" always=""
  rm -rf "$dir"; mkdir -p "$dir"
  if [ "$cond" = d ]; then sock="$SOCK_META"; fi
  if [ "$cond" = c ]; then always=', "alwaysLoad": true'; fi
  cat > "$dir/.mcp.json" <<JSON
{"mcpServers": {"inker": {"type": "stdio", "command": "$INKER", "args": ["mcp", "--socket", "$sock"]$always}}}
JSON
  if [ "$cond" = b ]; then printf '%s\n' "$TEACH" > "$dir/CLAUDE.md"; fi
  echo "$dir"
}

count_resolves() { cat "$SPIKE/host-meta.log" "$SPIKE/host-plain.log" | sed 's/\x1b\[[0-9;]*m//g' | grep -c 'tool=resolve' || true; }

printf 'cond\trun\tresolve_calls\tanswer_has_wages\tcost_usd\tdenials\tresult\n' > "$SPIKE/results.tsv"
for cond in a b c d; do
  dir=$(setup_project "$cond")
  for run in $(seq "$RUNS"); do
    before=$(count_resolves)
    out="$SPIKE/runs/$cond-$run.json"
    ( cd "$dir" && ENABLE_TOOL_SEARCH=true "$CLAUDE_BIN" -p "$PROMPT" \
        --output-format json \
        --strict-mcp-config --mcp-config .mcp.json \
        --setting-sources project,local \
        --allowedTools mcp__inker__resolve mcp__inker__describe mcp__inker__list \
        --no-session-persistence --max-budget-usd 3 ) > "$out" 2>"$SPIKE/runs/$cond-$run.err" || true
    after=$(count_resolves)
    python3 - "$out" "$cond" "$run" "$((after - before))" >> "$SPIKE/results.tsv" <<'PY'
import json, sys
path, cond, run, calls = sys.argv[1:5]
try:
    r = json.load(open(path))
except Exception as e:
    print(f"{cond}\t{run}\t{calls}\t?\t?\t?\tno JSON: {e}"); sys.exit()
text = (r.get("result") or "").replace("\n", " ")
print("\t".join([cond, run, calls, "yes" if "58,250" in text else "no",
                 f"{r.get('total_cost_usd', 0):.2f}", str(len(r.get("permission_denials") or [])), text[:160]]))
PY
    tail -1 "$SPIKE/results.tsv"
  done
done
```

Two notes on this run. First, the script's own `resolve_calls` column read 0 because the binary used then wrote ANSI colour codes into the log. The counts in the table above were recounted from the same logs with the codes stripped. The CLI now disables colour when stderr is not a terminal (test `logs_to_a_pipe_have_no_ansi_escapes`), and the script strips codes anyway. Second, `\x1b` in BSD `sed` on macOS does not match, which is harmless with the fixed binary.
