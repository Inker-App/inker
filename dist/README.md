# Installing INKER in Claude Desktop

The Claude app for macOS is INKER's first target. Claude starts INKER's MCP shim (`inker mcp`)
as a local stdio server; the shim forwards every request to the running INKER app, which holds
the vault. Both install paths below register the same shim. The INKER app must be installed and
running either way.

## Path 1: the Desktop Extension (`INKER.mcpb`)

A `.mcpb` file is a zip holding `manifest.json` and the `inker` binary. Build it with:

```sh
dist/make-bundle.sh                      # writes dist/INKER.mcpb from core/target/release/inker
dist/make-bundle.sh out/INKER.mcpb 0.2.0 # explicit path and version
INKER_BIN=/path/to/signed/inker dist/make-bundle.sh
```

or directly with `inker bundle --out INKER.mcpb [--version X] [--binary PATH]`. Without
`--binary`, `inker bundle` packs the binary that is running.

The user installs it by double-clicking `INKER.mcpb`, or in Claude under
Settings > Extensions > Advanced settings > Install Extension. Claude shows the manifest's
"Private folder" setting with a folder picker; it defaults to `~/Private`.

Anthropic's extension directory no longer takes MCPB submissions, so the bundle is side-loaded:
we ship it from inker.sh or from inside the app.

## Path 2: one entry in Claude's config file

`inker setup --agent claude-desktop` merges this into
`~/Library/Application Support/Claude/claude_desktop_config.json`, creating the file if needed
and keeping every other server and setting:

```json
{
  "mcpServers": {
    "inker": {
      "command": "/Applications/INKER.app/Contents/MacOS/inker-cli",
      "args": ["mcp"],
      "env": {}
    }
  }
}
```

The command is the absolute path of the `inker` binary that ran `setup` (the path above is an
the release workflow ships the CLI inside the app bundle as `Contents/MacOS/inker-cli`). Running setup again
changes nothing. If the file is not valid JSON, setup stops and says so rather than overwrite it.
`inker setup --agent all` does Claude Desktop first, then the coding agents (Claude Code, Cursor,
Codex). `inker doctor` reports whether Claude Desktop is installed and has INKER registered.

## Restart Claude after either path

Claude reads its MCP settings only at launch. After installing the extension or running setup,
quit Claude (Cmd-Q, not just closing the window) and open it again.

## Other chat apps

Raycast and Perplexity's Mac apps take the same stdio shape (`command` plus `args: ["mcp"]`);
INKER does not write their config, so copy the `inker` entry above into their MCP settings.
ChatGPT's desktop chat cannot use local servers.

## Release: sign and notarize first (Ben, on the Mac)

The `inker` binary inside the bundle must be signed with the Developer ID certificate (hardened
runtime) and notarized **before** it is packed, because signing rewrites the file. Unsigned
builds trigger Gatekeeper when Claude launches the shim. `make-bundle.sh` warns when the binary
it packs is not signed. Claude also labels any bundle without an MCPB signature as unverified;
signing the `.mcpb` itself (`mcpb sign`) is a separate, later step.
