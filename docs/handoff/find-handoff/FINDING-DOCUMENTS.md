# INKER: finding documents in the popover

Read this whole file, then the four design files in `design/`. This is additive to the App v2 popover: the four boards (empty, processing, attention, ready) and the drop card stay as designed. Everything here lives inside the 360x500 popover. There are no new windows.

## The problem being solved

Handles carry only the form type and year (`🐙/w2-2025`, `🐙/w2-2025-2`, `🐙/folder-0412`) because they get pasted into chats in the clear. So a handle never says which document it is. Two lookups have to work in a couple of seconds, at 30 files across three tax years:

1. Document to handle. "I want Claude to look at the W-2 from my second job."
2. Handle to document. "Claude just read `🐙/w2-2025-2`. Which file was that?"

Nothing here changes how handles are named, what the handle string contains, or what any agent receives. The only rule is that the handle string and anything sent over MCP stay clean. Showing the original file name and the detected payer inside the popover is fine, because the popover is private to the user's screen.

## The design files

`design/*.dc.html` are artboards from the design canvas. They won't run standalone (they need the canvas runtime); read the markup for layout, copy, sizes and colours. The `<script>` block only animates the INKER lockup.

| File | Shows |
|---|---|
| `AppFindScale.dc.html` | The ready board at 30 files: new header, tag chips, Find field, year groups, a collapsed folder group, the clickable last-read handle in the footer. |
| `AppFindSearch.dc.html` | Find with "mortgage interest" typed. Two 1098s match on their inked text, with the matching line shown under each row. |
| `AppFindHandle.dc.html` | The handle-to-document path: the clipboard chip, the located row with an ink pulse, the expanded row details, Show in Finder. |
| `AppFindRow.dc.html` | A row on hover with the `···` button, and the row menu open. |

## What to build

### 1. Header

Two icon buttons on the right, both always available on every board:

- **Open private folder.** A folder outline. Reveals the root of the private folder in Finder (`NSWorkspace.activateFileViewerSelecting` on the folder, or `open` on it). Tooltip "Open ~/Private" with the real path.
- **Settings.** Replace the current gear, which Ben says reads as a light-mode sun, with a two-line sliders icon (the SVG is in the design files). Same behaviour as today.

### 2. The ready row

Two lines, Copy on the right:

- Line 1: the **original file name**, Archivo 13 px, paper, single line, ellipsis.
- Line 2: the **handle** in JetBrains Mono 11 px paper, then `·` and muted text: form type `·` detected payer `·` inked count. Example: `🐙/w2-2025 · W-2 · Acme Robotics · 6 inked`. The year is not repeated on the row because the row sits inside a year group.

Payer is the employer, payer or lender the template attributed. If there is none, leave it out. Whether the payer shows at all is a new setting (see Decisions).

Processing rows already show the file name. Once a document is classified, add its type on the row before it is ready so the row does not reflow when it finishes. The attention card already names the file; add the pending handle under it in muted mono.

### 3. Grouping at scale

- Group by **tax year**, newest first. Group header: `2025 · 14 FILES` in the label style (Archivo 11 px, 600, width 125, 0.2em tracking, muted) with a chevron. The newest year opens by default; older years are collapsed. Remember open/closed state across launches.
- A **dropped folder** is a sub-group inside its year, collapsed by default. Its row shows the folder's original name and file count on line 1, and its folder handle `· all ready · N inked` on line 2, with its own Copy (which copies the folder handle). Expand to see children indented by 12 px.
- Documents with no year (a scan that didn't classify, a bank statement without one) go in a group called `UNDATED` at the bottom.
- Within a group, newest drop first. No sort controls in v1.

### 4. Tag chips

Templates now carry tags (Taxes, Banking, Medical, Legal). Show a chip row under the drop/Find row: `All` selected by default, then one chip per tag that has at least one file, with its count. A chip filters the list; year grouping still applies inside the filter. Only one chip active at a time. Manual tagging is **not** in v1.

### 5. Find

- The Find field sits beside "Drop more" in the same row, and only appears when there are more than 8 files. Below that the board is short enough to scan.
- Matching is case-insensitive substring over: original file name, handle (the `🐙/` or `ink/` prefix is optional in the query), form type, detected payer, year, folder name, tag, and the **inked text of the document**. The inked text is the tokenized text already in the index; searching it never touches a real value. Rank by field in that order, then by drop time.
- Results replace the grouped list with a flat list headed `N OF M MATCH` and a Clear button. Matched text is underlined (hairline, 50% paper) in whichever field matched. When the match is in the document text, show the matching line under the row in muted mono, ellipsised, with the match underlined.
- Typing a handle that exactly matches one document is the same as the handle-to-document path below.

### 6. Handle to document

Three ways in, all landing on the same result: the list scrolls to the row, the row expands, and an ink pulse (a ripple of dots fading out over about 1.5 s, from the ink grid renderer) plays once behind it.

- The footer's last-read handle is a **button** (paper pill, mono). Click it to locate the row.
- Paste or type a handle in Find.
- **Clipboard chip.** When the popover opens and the general pasteboard holds a string that parses as a handle for a document INKER has, show a chip above the list: `Your clipboard holds 🐙/w2-2025-2 · Find it`. Dismiss on tap or when the popover closes. This is a new setting (see Decisions). Never read the clipboard at any other time.

The expanded row shows, in muted label/value pairs: `In` with the path inside the private folder, `Dropped` with the date and "with N others" if it came in a folder, `Last read` with the client name, relative time and "saw only ink", then a full-width `Show in Finder` button.

### 7. The row menu

Copy stays the one visible action. On hover, a `···` button appears to the right of Copy; the same menu opens on right-click anywhere on the row. Items, in order:

1. `Copy 🐙/w2-2025-2` (default, same as the Copy button)
2. `Copy ink/w2-2025-2` (the ASCII form)
3. `Show in Finder`
4. `Ink again` (re-runs ingest on the original; useful after a template update)
5. separator
6. `Forget this file` (removes it from INKER's index and vault scope; the original stays where it is; asks for confirmation inline, not in a dialog)

Folder rows get the same menu with `Copy 🐙/folder-0412`, `Copy ink/folder-0412`, `Show in Finder`, `Ink again` (all children), `Forget this folder`.

## Decisions for Ben, with proposed defaults

1. **Show the detected payer on rows.** Proposed default on, with a Settings toggle "Show payer names in the list". Off makes every W-2 look alike, so on is the useful default.
2. **Clipboard chip.** Proposed default on, with a Settings toggle "Offer to find a handle from the clipboard". macOS does not prompt for pasteboard reads, so the toggle is there for people who'd rather the app never looked.

No change to handle naming or codename policy is needed for any of this.

## Copy rules

- The product is INKER. Never "the octopus" in copy.
- "Ink", "inked", "ink again". Never "redact".
- Handles always render in JetBrains Mono.
- Every string is a plain sentence or a plain label. Never the "Thing, supporting thing" shape.

## Visual constants

Background `#0D0F10`, panel `#08090A`, paper `#E6E4DE`, muted `#8A8C8E`, body-muted `#C9C7C1`, hairlines `rgba(230,228,222,0.14)`, row separators `rgba(230,228,222,0.08)`, hover row fill `rgba(230,228,222,0.04)`. No colour other than the red irises.

## Tests to write first

- Grouping: 30 synthetic documents across three years, two folders, two undated, produce the expected group tree and order.
- Find: each field matches; prefix-optional handle match; document-text match returns the right snippet; ranking order holds.
- Clipboard parse: accepts `🐙/x`, `ink/x`, rejects everything else and handles INKER doesn't hold.
- Row menu: Forget removes from index and vault scope and leaves the file; Ink again re-ingests and keeps the handle.
- Header: open-folder reveals the configured private folder.

## Next steps

1. Read this file and the four design files. Summarize back to Ben what you'll build and ask the two Decisions questions before starting.
2. Build test-first in the order above: grouping and search in Rust (`inker-core` index queries, exposed over FFI), then the Swift rows, header, menu and chip.
3. Show Ben the ready board at 30 synthetic files before polishing.
