# INKER: handoff brief

Read this whole file before writing any code. Then read the design files in `design/` that it points to.

## What INKER is

INKER (inker.sh) is a macOS menu bar app plus a local MCP server. It lets people use their coding agent (Claude Code, Cursor, Codex, anything that speaks MCP) on sensitive documents like tax forms, without the agent or the model provider ever seeing the identifying details.

The flow, from the user's side:

1. The user drags a file or a whole folder onto the INKER icon in the menu bar, instead of dropping it into their agent's chat.
2. INKER moves the file out of wherever it was (usually `~/Downloads`, which agents can read) into a private folder the user chose, like `~/Private`. That folder lives outside any agent workspace.
3. INKER extracts the text (PDF text layer, form fields, or OCR for scans), finds sensitive values, and replaces them with stable tokens. Example: an SSN becomes `[SSN_1]` and an employer name becomes `[EMPLOYER_1]`. Amounts, form boxes, dates and the state are kept, because the agent needs them to do useful work.
4. The user gets a handle on their clipboard, like `🐙/w2-acme-2025`, and pastes it into their agent's chat.
5. The agent sees the handle and calls INKER's MCP tools to read a tokenized version of the file. If the agent writes an output, INKER swaps the real values back in locally and saves the result to the private folder only.

The point: the agent can reason about your W-2 without ever seeing your SSN.

## Decisions already made (don't relitigate these without asking Ben)

- **Move, not copy, by default.** The first drop asks "Move or copy?" with an "Always do this" checkbox. Moving matters because agents can read Downloads.
- **No mirror folder.** The handle is a reference the MCP resolves. There is no second, redacted copy of the folder on disk.
- **Review happens at drop time.** Each file shows a processing state in the popover. If detection is unsure about something, the file is held in a "needs attention" state and its handle isn't served until the user answers. Agents that ask for a held file get "pending review".
- **The popover is a drop zone plus a queue.** There's no file browser; Finder already does that.
- **Handle syntax is `🐙/<codename>`.** INKER proposes the codename from the document type, the payer and the year (for example `w2-acme-2025`), and folder drops get a folder handle. Plan an ASCII fallback (something like `ink/w2-acme-2025`) as a setting.
- **Tokens are stable across documents and sessions.** The same SSN is always `[SSN_1]`. The mapping lives in a local encrypted vault, with the key in the macOS Keychain.
- **Three treatments per field.** Tokenize (SSN, EIN, account numbers), generalize (a full address becomes "Brooklyn, NY"), keep (amounts, boxes, dates).
- **Rehydrated outputs never go back into an agent-visible location.**
- **Use open-source detection.** Don't build a PII detector from scratch. Candidates: Microsoft Presidio, OpenAI's open privacy-filter model, pattern rules with checksums (Luhn, routing numbers), and form templates for the top tax forms (W-2, 1099-NEC/INT/DIV/B/K/MISC, 1098, 1095, K-1).
- **Taxes are the first use case, but the core is generic.** The vault, tokenization, handles and MCP should work for any sensitive document. Tax-specific logic (form templates, classification) is a pluggable pack.
- **Keeping agents out has three layers.** (1) The private folder lives outside agent workspaces; this is what actually protects the files. (2) During onboarding INKER offers to write path-deny rules into the configs of agents it finds, like `~/.claude/settings.json`. (3) Hooks that warn about direct reads, for agents that support them, are a later nice-to-have.

## The brand and the world (this matters for implementation)

The visual identity is a small world:

- **The Nosy Ones:** eyeballs, the villains. They want to see everything and dart around constantly. They stand in for models, logs and servers. They always have **red irises**, and that red (`#E0473E`) is the only color in the product.
- **The Octopus:** the hero. It doesn't fight; it inks. It stands in for the user.
- **The Ink:** the redaction, the loaders and the logo are all made of the same dots. "Pooled" means hidden or held. "Dispersed" means moving, loading or leaving.

Rules:
- The app is **all dark**: background `#0D0F10`, inset panels `#08090A`, paper/text `#E6E4DE`, muted text `#8A8C8E`, hairlines `rgba(230,228,222,0.14)`, ink stain rim `#3A3D40`.
- **Everything is 3D, rendered down to dots.** Characters are lit 3D shapes whose brightness is converted into a halftone dot grid. Nothing is flat-drawn.
- **Big motion only on a state change or a touch.** At rest, things barely breathe.
- **Type:** Archivo (variable width; labels at width 125, weight 600, tracked about 0.2em, all caps), Archivo for body, JetBrains Mono for handles and tokens.
- The UI says "ink", not "redact": "Ink it", "6 inked", "3 inking".

## The design files

`design/` contains the exported design canvas. Each `*.dc.html` file is one artboard. They need the canvas runtime (`support.js`, not included) to render, so **treat them as source reference, not runnable pages**. The useful parts are:
- the markup, which gives layout, copy, sizes and colors
- the `<script type="text/x-dc" data-dc-script>` block, which holds the actual rendering and animation code as plain JS

Ben can also share the live canvas link if you need to see them move.

`design/canvas.json` maps files to pages. The pages are:

| Page | Files | Status |
|---|---|---|
| **App v2** | `AppOnboard1-3`, `AppPopEmpty`, `AppPopProcessing`, `AppPopAttention`, `AppPopReady`, `AppDropDialog` | **Build target.** These are the screens to implement. |
| **World** | `WorldEyes`, `WorldOctopus`, `WorldBible` | Character and lore reference. `WorldBible` is the written world. |
| Brand round 1 | `Brand*` | Earlier exploration. Use `BrandWordmark` (the lockup row), `BrandRedaction` and `BrandMotion` as references. The rest is superseded. |
| Flows v1 | `Main`, `Onboard2-3`, `Popover*`, `DropDialog` | Superseded wireframes. Ignore except for flow logic. |

### The two renderers (port these, don't redraw them)

1. **Dot-shaded 3D renderer.** See `WorldEyes.dc.html`, `WorldOctopus.dc.html`, and `eyeDraw` in the App v2 files.
   - The eyes are analytic spheres. For each grid cell, compute the surface normal, then Lambert lighting plus a weak specular term. The iris and pupil are set by the angle between the normal and the gaze vector. Blinks squash the visible band.
   - The octopus is a raymarched signed-distance-field model: an ellipsoid head, 8 tentacles made of tapered capsule chains blended with a smooth union, and slit-pupil eyes. It renders at 12 fps on purpose, for a stop-motion feel.
   - Brightness maps to dot radius: `r = cell/2 * sqrt(b)`. Glare is deliberately low. Cell size is adjustable; the World boards are tuned to 9 px for the eyes and 11 px for the octopus.
   - Eye behaviour: random saccades every 0.4 to 2.8 s, look at the cursor when it's present, blink every 2 to 7 s, and squint at ink.
2. **Ink grid.** See `field` / `drawInk` / `bridge` in any App v2 file, and `BrandMaterial` / `BrandRedaction`.
   - Cells are circles with radius proportional to a value `v` from 0 to 1. Neighbouring cells merge through bridges whose width is `smoothstep(0.55, 0.9, min(v_a, v_b))`, and 2x2 blocks fill in.
   - Lowering `v` turns a pooled blob into separate dots. This single number drives the logo, the loaders, the dissolve animations and redaction pills.
   - The wordmark is the INKER letters plus the 7-cell octopus mark on this grid; diagonal strokes join corner to corner.
   - `BrandMaterial` also has a Gray-Scott reaction-diffusion field (f=0.0545, k=0.062), used faintly in the empty drop zone.

These will need native equivalents (Metal shaders, Core Animation, or SwiftUI Canvas). The JS is the spec for behaviour and tuning.

## Open questions (ask Ben, don't decide)

- **Tech stack.** Native Swift/SwiftUI, Tauri, Electron, or another option. Ben likes to sit in the options before choosing. Lay out the tradeoffs (menu bar ergonomics, running the renderers, bundling a local detection model, MCP server language) and let him pick.
- **How the MCP server runs.** Either the agent launches it over stdio, or the menu bar app hosts it and the agent connects over HTTP. This affects whether the vault lives in a single process.
- **UI and UX decisions are Ben's.** If a screen needs something the designs don't show, ask. Internal code structure is yours to decide.

## Next steps, in order

1. **Read and orient.** Read this file, `design/WorldBible.dc.html`, and the App v2 files. Summarize your understanding back to Ben in a few lines.
2. **Propose the stack.** Write up 2 or 3 options with real tradeoffs. Don't scaffold anything until Ben picks.
3. **Spike: does an agent act on `🐙/` cold?** Build a minimal MCP server whose `instructions` field says that `🐙/` strings are private document references and to call `resolve`. Test in Claude Code with and without a CLAUDE.md line. This decides how much onboarding is needed.
4. **Core pipeline, headless.** Ingest, extract, detect, tokenize, vault, then MCP tools (`list`, `resolve`, `describe`, `write`). Test only on synthetic sample documents. **Never commit real PII**, and add a pre-commit check for SSN-shaped strings.
5. **Menu bar shell.** Drop target, the move/copy dialog, the queue with its four states, handle copy, and the badge count.
6. **Renderers.** Port the dot-shaded eye, the octopus and the ink grid. Set a CPU/GPU budget for an always-running menu bar app, respect Reduce Motion, and pause rendering when the popover is closed.
7. **Onboarding.** The folder picker, deny-rule writers for each agent, MCP config writers, and the CLAUDE.md / .cursorrules line.
8. **Separate tools, later.**
   - **Ink Lab:** a Storybook for tuning the redact and dissolve animations. Start by porting `BrandRedaction` and `BrandMotion` in as stories.
   - **Asset Studio:** a tool that uses the same renderers (eyes, octopus, ink grid, wordmark) to compose posters and social posts, with type packed onto the ink grid.

## Pinned for later

- The eyes are nosy. Could they turn into noses? Parked.
