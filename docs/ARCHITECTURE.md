# yellod notes + calendar — complete architecture & feature guide

This document describes the whole product, both apps, the shared core, the
database, the web deployment, the build/release pipeline, and how the pieces
fit together. It is the authoritative map of the codebase: what exists, why
it is shaped the way it is, and how to change it safely.

---

## 1. Product overview

**yellod notes + calendar** is a suite of privacy-first, local-first apps:

| App | Purpose | Native app | Web app |
|---|---|---|---|
| **Notes** | Google Keep–style notes with markdown, tags, folders, attachments and an end-to-end-encrypted vault | `apps/notes` | `/notes.html` (self-hosted) |
| **Calendar** | Native calendar with desktop notifications, week views, recurring events | `apps/calendar` | `calendar.yellod.dpdns.org` (calendar-only deployment) |

### Design pillars

1. **Local-first.** Everything is stored locally (SQLite in the app data
   directory) and works offline. The web build talks to the same backend.
2. **Privacy.** An optional vault encrypts every note end-to-end with a key
   derived from the user's password. The server only ever holds ciphertext
   plus minimal metadata (timestamps, colours, sizes — exactly what a mail
   index would show).
3. **Fast and lean.** Native Slint UI (no Electron), Rust backend, SQLite
   with FTS5. Release binaries are stripped, LTO'd and a few MB each.
4. **Exact parity with the deployed site.** The visual design is reverse-
   engineered from the public CSS of `notes.yellod.dpdns.org` (Google Keep's
   exact palette), so the same design language appears everywhere.

### Supported platforms

Native apps: **Windows** x86_64/ARM64, **Linux** x86_64/ARM64, **macOS** Apple
Silicon (ARM64; Intel is deliberately unsupported), **Android** ARM64.

Web: self-hosted Rust (axum + SQLite) single binary, deployed as the public
calendar at `calendar.yellod.dpdns.org`; a full-suite mode serves notes too.

---

## 2. Repository layout

```
notes+calendar/
├── Cargo.toml                 # workspace: apps/notes, apps/calendar, crates/ycore, web/suite
├── README.md
├── LICENSE (MIT)
├── apps/
│   ├── notes/                 # the Notes app (Slint UI + Rust logic)
│   ├── calendar/              # the Calendar app
│   └── shared-ui/             # shared design system (Theme, components, icons)
├── crates/
│   └── ycore/                 # shared core: models, SQLite storage, crypto
├── web/
│   └── suite/                 # web app: axum server, static frontend, REST-ish API
├── deploy/                    # nginx/systemd/package assets for self-hosting
├── packaging/                 # deb/rpm/Android assets
├── web/suite/static/          # the prebuilt static frontends (notes.html, calendar.html)
├── docs/                      # this file + PLAN.md
├── .github/workflows/         # CI (build/test) + Release (matrix build, signed APK)
├── assets/                    # fonts, icons, app icons, generator scripts
├── tools/                     # capture/winsnap (pixel-accurate screenshots, DPI-aware)
└── releases/                  # release staging (v1.0, v1.1 artifacts)
```

### Workspace members & dependency direction

```
apps/notes ─┐
            ├──► crates/ycore ──► SQLite (rusqlite, bundled), AES-GCM, PBKDF2
apps/calendar ─┘        ▲
web/suite ──────────────┘
apps/shared-ui ── used by both apps (import "../../shared-ui/...")
```

`ycore` has no UI dependencies. `apps/*` never talk to each other; all shared
behaviour lives in `ycore` or `apps/shared-ui`.

---

## 3. The shared core (`crates/ycore`)

`ycore` is the single source of truth for data correctness. Everything that
must be identical on every platform lives here.

### 3.1 `model.rs` — the data model

```
Note {
    id: Uuid, title: String, body: String,
    folder_id: Option<Uuid>, tags: Vec<String>,
    pinned: bool, favorite: bool, color: Option<String>,
    archived: bool, trashed: bool,
    created_at: DateTime<Utc>, updated_at: DateTime<Utc>,
    encrypted: bool,          // vault notes only
    sort_order: i64,          // manual order
}
```

Related structures: `Folder` (hierarchical, `icon` + `color`), `NoteImage` /
`NoteImageMeta` (attachments), `ChecklistItemData`, `GraphPosition`, `Prefs`
(denort, layout, density, clamp, sort...), `VaultConfig`, `NoteSummary` (the
list projection used by the board), `NotesSort` (Updated | Created | Title |
Manual). `strip_markdown` + `truncate_preview` produce the card previews.

**Preview pipeline:** `NoteSummary::from(&Note)` strips markdown to plain
text, keeps the first ~160 chars backed off to a word boundary, and appends an
ellipsis. The board estimates card heights from those previews; the editor
renders full bodies through `styled_markdown`.

### 3.2 `storage/` — SQLite storage (`notes.rs`, `calendar.rs`, `mod.rs`)

The `notes` schema (v1.0):

```sql
notes (
    id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '', body TEXT NOT NULL DEFAULT '',
    folder_id TEXT, tags TEXT NOT NULL DEFAULT '[]',   -- JSON array
    pinned INTEGER NOT NULL DEFAULT 0,
    favorite INTEGER NOT NULL DEFAULT 0,
    color TEXT,
    archived INTEGER NOT NULL DEFAULT 0,
    trashed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
    encrypted INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (folder_id) REFERENCES folders(id) ON DELETE SET NULL
);
folders (id TEXT PRIMARY KEY, name, parent_id, icon, color, sort_order, created_at);
note_activity (id TEXT PRIMARY KEY, note_id, kind, created_at);  -- Pinned/Edited/Created events, shown in Timestamps
note_images   (id TEXT PRIMARY KEY, note_id, name, mime, size, bytes BLOB);
notes_fts     (FTS5, porter+unicode61, content='notes');         -- full-text search
```

Triggers keep the FTS index in sync on insert/update/delete. Migrations
apply idempotently on open (`CREATE TABLE IF NOT EXISTS`, plus a column-add
migration for `encrypted`).

Key operations in `storage::notes`:

* `list_notes_with_key(filter, key)` — list + decrypt-on-read (legacy plaintext
  passes through); used by the board and the vault view.
* `search_notes` — FTS5 `MATCH` query with snippet extraction.
* `set_note_order(ids)` — writes `sort_order` 0..n; used by drag to reorder.
* `purge_deleted` / `empty_trash` semantics (cascade-delete activity + images).
* `create_image` / `get_image` — images are stored as BLOBs, encrypted when
  the vault key is present.

### 3.3 `crypto.rs` — the vault (end-to-end encryption)

Password-derived keys, **two independent derivations from the same password**:

* `auth` branch — PBKDF2-SHA256(600k iters, salt = `"notes:yellod:<user>"`)
  produces `client_hash`, used as the server-side login verifier.
* `enc` branch — PBKDF2-SHA256(600k, salt = `"notes:enc:<user>"`) produces the
  AES-256-GCM **data key**, which never leaves the device.

Text encryption format: `v1.<iv-b64>.<ct-b64>`. Image bytes: raw
`iv(12B) || ciphertext`. Legacy (pre-vault) plaintext rows pass through
transparently and are migrated on unlock. `zeroize` wipes keys from memory.

This mirrors the web app exactly (`PBKDF2_ITERS = 600000`, same salts), so a
user can sign in on web and desktop and read the same notes.

### 3.4 `sys.rs`

Platform shims: data-directory resolution (`dirs::data_dir()`), plus helpers
for the desktop apps.

---

## 4. `apps/shared-ui` — the design system

Design tokens (`theme.slint`) are a 1:1 mirror of the deployed site's
`:root`/`[data-theme="dark"]` CSS custom properties:

| Token | Value (dark) | CSS source |
|---|---|---|
| `bg` | `#121212` | `--bg` |
| `surface` (bar) | `#1B1B1B` | `--bar-bg` |
| `surface-2` | `#2A2B2D` | `--search-bg` |
| `surface-3` | `#333639` | `.searchbox:focus-within` |
| `card-bg` | `#1F1F1F` | `--card-bg` |
| `border` | `#5F6368` | `--border` |
| `divider` | `#2C2D2F` | `--divider` |
| `icon` | `#A7AAB0` | `--icon` |
| `text` / `text-dim` / `text-faint` | `#E8EAED` / `#9AA0A6` / `#8A919C` | `--text` / `--text-sec` |
| `placeholder` | `#9AA0A6` | `--placeholder` |
| `accent` | `#AECBFA` (dark) | `--link` (#8AB4F8 family); vault blue used for controls |
| `drawer-active` | `#47484C` | `.drawer-item.active` (dark) |
| note palette | n-red `#5C2B29`, n-yellow `#635D18`, ... | `.note-card[data-color=…]` |
| `scrim` | `rgba(0,0,0,.6)` | `--scrim` |
| `r-sm/md/lg/xl` | 8 / 12 / 16 / 24px | `--radius: 16px` |
| `topbar-height` | 64px | `.topbar { height: 64px }` |
| `sidebar-width` | 264px | persistent sidebar (web rail is 64px; we keep the 264px persistent panel for density) |

`components.slint` exports the reusable widgets:

* `IconButton` — 33%-radius icon button, hover background, `danger`, `selected`.
* `RailButton` — icon rail entry (used for the left icon rail in compact modes).
* `SidebarRow` — sidebar folder/tag rows (46px, 14px label, 19px icons, count
  badge, hover reveal for delete).
* `SearchBar` — 52px, 14px radius, 16px font, 22px glyph, placeholder.
* `Input` — styled single-line input.
* `NoteCard` — the Keep card (see §6.2).
* `TextButton` — plain text button (`emphasis`, `danger`).
* `ConfirmDialog` — modal confirm ("Delete forever?", "Empty trash?").
* `Toast` — bottom toast.
* `EmptyState` — centered empty illustration + title + subtitle.

`icons.slint` — hand-tuned Lucide-style 24×24 stroke icons + `IconPinFilled` /
`IconStarFilled` (webapp push-pin/star fill-outline pair, used for toggles).

### Fonts

Inter is bundled (`assets/fonts/Inter-*.ttf`) and imported at the theme level,
so every window renders identically on all platforms (no system font fallback).

---

## 5. `apps/notes` — the Notes app

### 5.1 Files

```
apps/notes/
├── build.rs               # slint_build::compile(ui/notes.slint)
├── Cargo.toml             # crate-type: lib + cdylib (Android), bin notes
├── src/
│   ├── main.rs            # CLI entry -> run()
│   └── lib.rs             # ~3300 lines: ALL app logic (see §5.3)
└── ui/
    ├── notes.slint        # root window, layout, wiring
    ├── note-grid.slint    # the board (keep/masonry + list/rows)
    ├── note-card*         # (components.slint)
    ├── editor.slint       # the modal editor sheet
    ├── composer.slint     # the "Take a note…" composer
    ├── graph-view.slint   # 3D graph canvas + selection panel
    ├── vault-view.slint   # Obsidian-style reading pane
    ├── editor.slint       # modal editor (Write/Split/Read, find bar, tags)
    ├── settings.slint     # settings dialog
    ├── markdown-guide.slint
    ├── doodle.slint       # freehand drawing canvas
    ├── graph-view.slint
    └── vault-view.slint
```

### 5.2 UI structure (`notes.slint`)

One `MainWindow` containing, in paint order:

1. **Top bar** — hamburger, brand mark (ghost SVG + "Notes"), centered search,
   view-mode toggle, accent **+** button, settings gear.
2. **Main row** — sidebar (folders / tags / Vault / Graph / Light-Dark) and
   the content region.
3. **Content region** — mutually exclusive:
   * Keep board (`boardDesktop`) — composer + masonry grid
   * Notion rows (`layout-style == 1`) — uniform 100px rows
   * Obsidian vault (`vault-view.slint`) — list + markdown reading pane
   * 3D graph (`graph-view.slint`) — canvas + selection panel
   * Compact (phone): board *or* full-screen editor
4. **Modal editor** (`editorScrim`) — `scrim + 600px card`, previously
   "split view"; now the single-click destination for every note.
5. **Status bar** — "Local-first · Private" + note count.
6. Overlays — doodle canvas, markdown guide, image preview, delete-forever /
   empty-trash confirmations, toast, timestamps dialog.

### 5.3 Application logic (`src/lib.rs`, ~3300 lines)

Organized into:

* **Model helpers** — `estimate_card_height_px` (width-aware), `styled_markdown`,
  `parse_checklist_items`, `parse_wiki_links`, `est`, `rel_time`, `word_count`.
* **Board layout** — `partition_heights` (order-preserving DP optimal partition
  minimizing the max column sum, greedy fallback for n>400),
  `compute_board_layout` (pinned block + others block, section-header space
  reservation), `BoardLayoutInfo`, `board_drag_target` (pointer→insertion slot
  hit-test), `move_note_to_index` (reorder + switch to Manual sort).
* **Seed & migration** — `unlock_and_seed` seeds the welcome note on first
  launch; `purge_legacy_blue_seed` removes the pre-1.1 blue pinned seed note
  (byte-identical match only, so user edits are never deleted).
* **Vault** — `VaultConfig` unlock/create/reset/change-password, encrypted
  note bodies, `get_from_state`/`update_from_state`/`create_from_state`.
* **Interaction handlers** — every Slint callback: search debounce, view
  switching, folder/tag selection+creation, archive/trash/restore/delete,
  editor content (350ms debounce), images, checklists, find-in-note, wiki-link
  suggestions, graph pointer events + auto-orbit, doodle, timestamps, import/
  export, vault unlock/create/reset, settings.
* **Board rendering state** — `refresh_list` recomputes the flattened
  `pinned ++ others` order, pushes `board_lefts/tops`, heights, column count,
  total canvas height to the UI, and persists `sort_order` on drag.

### 5.4 The board (Keep masonry)

Algorithm (identical semantics to the webapp's `layoutMasonry()`):

1. Flatten `pinned ++ others` (pinned only when `pinned_first` is on).
2. Estimate each card's height from its title/preview/line-clamp
   (`estimate_card_height_px`, clamped to `[150, 520]`).
3. Partition the two blocks independently across `cols` columns with an
   **order-preserving optimal partition** (DP over prefix sums, min-max
   objective). This is what guarantees columns end evenly filled regardless
   of window width — no left-column-only piling.
4. Push absolute canvas coordinates (`lefts`, `tops`) + `canvasHeight` to the
   UI; the board is one `Flickable` with an absolutely-positioned canvas of
   `NoteCard`s. The PINNED/OTHERS section labels sit in the canvas too.

**Column count** follows density (compact/cozy/roomy) and window width
breakpoints in `notes.slint` (`grid-cols`).

### 5.5 Drag & drop reordering

Pointer-based drag from any card:

* Cards accept pointer events (`drag-enabled`), a 7px threshold starts the drag.
* During drag the source card renders as a 25%-opacity "hole"; a floating ghost
  follows the pointer (`drag-abs-x/y`).
* `drag-move-rust(rel-x, rel-y)` → Rust `board_drag_target()` hit-tests the
  column by pointer x, counts cards above by center-vs-pointer-y, and pushes a
  drop-target index + highlight geometry to the UI.
* On drop, `move_note_to_index()` removes the note at its current position and
  inserts it at the target in the flattened order, persisting `sort_order` and
  switching the view to **Manual** sort. The webapp does the same.
* `mouse-drag-pan-enabled: false` on the `Flickable` so cards get the events;
  the canvas never steals card drags.

### 5.6 The modal editor

* Single click on any card opens it in a 600px modal sheet (scrim behind,
  click-through closes, `Esc` closes via the root `FocusScope`).
* Write / Split / Read segmented control (`editor-mode`) — Read renders the
  markdown; Split shows source + rendered side by side.
* Markdown toolbar-less editing, `[[wiki-links]]` with live suggestions drawn
  from existing note titles (linking feeds the graph), checklists
  (`- [ ]`/`- [x]`) with per-item toggle, in-note find bar (Ctrl+F, matches,
  next/previous, selection highlight), tags row, color picker, archive/trash/
  restore/copy/export, image attachments (encrypted with the vault key), a
  draw/doodle canvas, and a timestamps dialog showing an activity log per note.
* Save debounce is 350ms; `refresh_graph` runs after flush so the graph stays
  consistent.
* `flush_note` is guarded by `editor_note_id` — the editor only writes back
  to the note it actually holds. This prevents silently blanking a note when
  the editor was never populated (e.g., opening from the Vault pane).

### 5.7 Obsidian-style vault view

A split pane: filterable list on the left (`vault-filter`), rendered markdown
on the right. `[[wiki-links]]` navigate between notes and build backlinks;
`Links out` / `Backlinks` panels below the doc. Source mode is a toggle for
editing raw markdown in place. Also reachable from the sidebar and from the
`Obsidian` layout choice in Settings.

### 5.8 3D graph view

Notes become nodes; `[[wiki-link]]`s become edges (directed, drawn as
cylinders). Force-directed layout computed on-device from a deterministic
seeded circle, slow auto-orbit (33ms tick), pointer drag rotates, wheel zooms,
node press = select, press+move = drag node, pointer-release on empty = deselect,
`Alt`+drop on empty space = unlink all, double-click opens the note. Nodes are
depth-sorted so far ones fade behind near ones.

### 5.9 Composer ("Take a note…")

Collapsible bar at the top of the board: title, body (markdown/checklist),
pin toggle, color picker, pending-images chip row, checklist mode, preview
mode. Creating a note from the composer assigns `sort_order = min - 1` (it
lands on top). `Esc` collapses; `Enter` in title moves to body.

### 5.10 Settings

Settings is a full page (web `#settings`: 720 px centred column, group
headings 15 px/500, hairline dividers between groups) — Theme / View style /
Density / Notes behaviour / Markdown / Security, plus native Data/About.
Theme (Light / **Dark**), view style (Keep / Notion / Obsidian / Neomorph /
3D graph), density (Compact / **Cozy** / Roomy), phone two-up toggle, sort
order, pinned-first toggle, markdown-in-cards toggle, dates toggle, preview
clamp, markdown guide, Import .md / Export all, vault (set password, change
password, reset password), data (Empty trash, stats), About (app version from
`CARGO_PKG_VERSION`).

---

## 6. Building, running, testing

### 6.1 Dev workflow

```powershell
# isolated profile (never touches the real one):
$env:YELLOD_NOTES_DATA_DIR = "$env:TEMP\notes-dev"
$env:YELLOD_NOTES_OPEN_LAST = "0"
$env:YELLOD_NOTES_DEBUG = "1"   # eprintln diagnostics ([board] drag rel=… etc.)
cargo run -p notes
cargo run -p calendar
```

Environment knobs:

| Var | Default | Meaning |
|---|---|---|
| `YELLOD_NOTES_DATA_DIR` | `%APPDATA%\yellod\Notes` | profile directory (db + config) — must be set for tests |
| `YELLOD_NOTES_OPEN_LAST` | `1` | reopen the last-edited note on launch (`0` for tests) |
| `YELLOD_NOTES_DEBUG` | unset | eprintln debug traces |

### 6.2 Screenshot-based visual testing

The repo contains a DPI-aware Windows capture tool (`tools/winsnap.rs`,
PrintWindow-based) because GDI/pinv virtualization misreports coordinates at
125% DPI. `tools/capture.py` is DPI-aware but less reliable; `winsnap` is the
source of truth. Screenshots are written, flipped, and inspected by eye.

### 6.3 CI

`.github/workflows/ci.yml` — fmt, clippy, build, test on Ubuntu/Windows/macOS.
`.github/workflows/release.yml` — tag push → release matrix
(Linux gnu x86_64/aarch64, Windows x86_64/ARM64, macOS ARM64, Android ARM64
APK via cargo-apk2 with the artifact-resolver panic tolerated and APKs
verified by size), then Draft release with checksums + build provenance
attestation.

## 7. Web deployment (`web/suite`)

`suite` is a single-binary Rust server (axum + tokio + rusqlite/ycore) serving
the static frontends plus a small JSON API for the web apps.

* `static/notes.html` — the self-hosted notes web app (vanilla JS + WebCrypto).
* `static/calendar.html` — the calendar web app.
* `static/index.html` — suite landing / chooser.
* `src/auth.rs` — login/logout/me with opaque httpOnly session cookies.
* `src/notes.rs` — notes CRUD + image upload, used by the web app; the same
  PBKDF2 parameters as the native vault so accounts are interchangeable.
* `src/calendar.rs` — calendar events API.
* `src/state.rs` — env-driven config: `SUITE_DB`, `SUITE_BIND`,
  `SUITE_STATIC_DIR`, `SUITE_DISABLE_NOTES`.

### Calendar-only deployment

`SUITE_DISABLE_NOTES=1` mounts no notes routes and makes `/` redirect to
`/calendar.html`. This is how `calendar.yellod.dpdns.org` is deployed: notes
and the notes API are not served publicly at all — the native notes app and
the self-hosted suite are the supported way to run notes.

### nginx (see `deploy/`)

Reverse proxy with TLS (certbot), `/` -> suite, `/api/*` -> suite, static
assets cached aggressively; the suite serves the static files itself via
`tower-http`'s `ServeDir`.

## 8. Feature inventory

### Notes — everything the app does today

**Views** — All notes / Pinned / Favorites / Archive / Trash / Folder / Tag /
Vault (Obsidian-style) / 3D graph. View mode toggles between masonry grid and
uniform list rows; 5 layouts are available (Keep · Notion · Obsidian ·
Neomorph · 3D graph).

**Composer** — inline new-note bar: title, body, pin, color, markdown preview,
checklist mode, image attach, drawing canvas; Enter creates, Esc collapses.

**Notes** — pin/favorite/archive/trash/restore, 11 color swatches, tags
(`#tag` parsing + chips), word count, relative timestamps, undoable trash,
empty-state illustrations, markdown guide dialog.

**Editor** — Write / Split / Read modes, markdown rendering, checklists with
per-item toggle, `[[wiki-links]]` with live autocomplete, find bar (Ctrl+F,
next/prev, match highlight), tags row, color palette, timestamps (activity
log), export .md, copy to clipboard, image attachments with thumbnails and a
lightbox, drag-drawn doodles (Ctrl+Z undo), source-edit mode in the vault
pane.

**Board** — Google Keep-style balanced masonry (order-preserving optimal
partition, width-aware card height estimation), PINNED/OTHERS sections,
drag-to-reorder with a floating ghost and drop-slot highlight, auto-switch to
Manual sort on drag, density presets (compact/cozy/roomy), preview line clamp
(3/5/8/unlimited), pinned-first sectioning toggle.

**Vault (E2EE)** — password: all notes/titles/bodies/images/tags encrypted
AES-256-GCM with a PBKDF2(600k)-derived key that never leaves the device;
metadata (timestamps, colors, images counts) stays plaintext exactly like the
web app. Unlock/create/change/reset flows, wrong-password banner, status bar
indicator ("Local-first · Vault encrypted" / "Local-first · Private").

**Search** — FTS5 porter tokenizer, debounced, match highlighting through the
markdown/card renderers, in-note find bar, `[[title]]` link autocomplete fed
by note titles.

**Attachments** — images (jpg/png/gif/webp/bmp), encrypted with the vault key,
thumbnails in grid cards, lightbox with download, doodle PNG export
(1200×800).

**Import/Export** — import `.md` files as notes; export every note as one
`.md` file with front-matter (title, tags, created/edited).

### Calendar — everything the app does today

Month/week grid, all-day + timed events, recurring events (RRULE subset),
color-coded calendars, reminders, local notifications via `notify-rust`,
SQLite storage in the same app data dir, native Windows/Linux/macOS/Android.

### Cross-cutting

* 5 views + 5 layouts + 3 densities, hot-switchable.
* Doodle canvas (freehand, eraser, undo, PNG export).
* Built-in markdown guide.
* Persistent sidebar with folder tree, tag list, vault/graph shortcuts,
  theme toggle.

## 9. Release & packaging

* Notes + Calendar are packaged as bare binaries (Windows `.exe`, Linux
  x86_64/aarch64, macOS `.app`), `.deb`, `.rpm`, and Android APKs.
* Release CI matrix: Linux x86_64/aarch64 (glibc ≥ 2.35), Windows x86_64/
  ARM64 (msvc), macOS ARM64 (M1+), Android ARM64.
* Every binary is checked against a **20 MB ceiling** (Windows x86_64: notes
  ≈ 16 MB, calendar ≈ 13 MB).
* Release profile: `lto = "fat"`, `codegen-units = 1`, `opt-level = "s"`,
  `strip = "symbols"`, `panic = "abort"` — chosen to keep the ceiling with
  headroom (Windows: notes 16.1 MB, calendar 12.7 MB).
* Releases are published as GitHub drafts with `SHA256SUMS.txt` and build
  provenance attestations; a manual `upload` step ships them.

## 10. Storage locations & file formats

| What | Where |
|---|---|
| Notes DB + config | `%APPDATA%\yellod\Notes\notes.db`, `config.json` |
| Calendar DB | `%APPDATA%\yellod\Calendar\calendar.db` |
| Web suite DB | env `SUITE_DB` (default `./suite.db` in CWD) |
| note images | `note_images.bytes` BLOB, encrypted under vault |
| activity log | `note_activity` table |
| search index | `notes_fts` (FTS5, auto-synced via triggers) |

Config schema (`config.json`): `dark` (bool), `seeded` (bool), `vault`
(`enabled/salt/verifier`), `prefs` (`layout`, `density`, `sort`,
`pinned_first`, `md_cards`, `show_dates`, `clamp`, `mobile_two`).

## 11. Testing strategy

* **Unit tests** — `ycore::storage` (list/search/CRUD/empty-trash cascade),
  `strip_markdown`/preview truncation, checklist parsing, graph helpers.
* **Visual testing** — screenshots via `tools/winsnap.rs` (PrintWindow,
  pixel-accurate under DPI scaling); each UI iteration is verified against
  `notes.yellod.dpdns.org` CSS as the reference.
* **Manual QA matrix** — board reorder, modal editor round-trip, vault
  unlock/reset, search, import/export, empty states (fresh profile), light/
  dark themes.

### Synthetic input caveat (automation)

UI automation of the Windows app uses `SendInput`/`SetCursorPos` via a
DPI-aware helper (`pctl.py`, PMA v2). This works only while the session is
unlocked and no secure-desktop/input-grabbing process holds the input
queue. Mid-session, `SetCursorPos` began failing silently (`returns 0`,
`ERROR_SUCCESS`) and `SendInput` events were swallowed — likely caused by a
concurrent setup/KVM utility taking ownership of input on the desktop.
Symptoms: synthetic clicks and keystrokes behave as no-ops while rendering
(`PrintWindow`) keeps working. Real user keyboard/mouse input is unaffected.
Keyboard injection via `KEYEVENTF_UNICODE` never reached Slint's TextInputs
(only VK-code paths like Ctrl+N were processed), so text typing must be
verified with a real keyboard. When synthetic input dies, `winsnap.exe`
still captures the screen, which keeps visual verification possible without
interacting. NOTE: the modal-footer tests (Write/Split/Read tabs, Timestamps,
Export) all returned dead — while the cursor was provably warped to the
window's close button — so those results are **inconclusive, not a defect**.
Verify them with real clicks; do not "fix" the footer on the strength of
synthetic evidence.

## 12. Known issues & non-goals

* Prebuilt web assets under `web/suite/static/` are checked in; regenerating
  them is part of the web build, not the native build.
* macOS Intel, Windows 32-bit, Android x86: out of scope by design.
* Tray icon, multi-window: not implemented.
* Sync between devices is out of scope (local-first by design); the web suite
  is the sync path for self-hosters.
* The app intentionally does not phone home anywhere.

## 13. What this codebase looks like (session changelog)

* **1.1.0** — board restructure: split view removed in favour of a full-width
  masonry board, modal editor (scrim + 600px card), drag-to-reorder with
  Rust-computed drop targets, glyph/density cleanup, text-scale fixes, Google
  Keep palette adoption, PINNED/OTHERS sections, sidebar rows, pin/star
  filled-toggle icons, animation pass (hover lift, fade-in actions, modal
  fade/slide).
* **1.1.0 (backend hardening)** — empty-note prune (`prune_empty_editor_note`):
  notes with no title, no body and no attachments are deleted on editor
  teardown (Esc/scrim/Close), on switching notes, on new note, and at app
  exit — no more dead rows from clicking "+" and closing. SQLite integrity
  audited end-to-end (WAL, FTS5 triggers consistent, 18/18 fts rows match,
  `foreign_key_check` clean after test-profile cleanup). ycore unit suite
  14/14 green (notes/calendar CRUD, FTS sanitizing, vault crypto roundtrip).
  Empty-note leak observed and fixed in both the `+` button and Ctrl+N paths
  (both eagerly created an empty row). Verified live until synthetic input
  died: "+" → row created; Close → row pruned; window × with empty note open
  → row pruned at exit.
* **1.1.0 (release profile)** — the full board restructure pushed the Windows
  release binary from 16.2 MB to 22.6 MB (fat new code, same deps/toolchain).
  Workspace `[profile.release]` switched from `lto = "thin"` +
  `codegen-units = 16` to `lto = "fat"` + `codegen-units = 1` and
  `opt-level = "s"`: `notes.exe` 22.6 MB → **16.1 MB** (15.4 MiB),
  `calendar.exe` 15.2 MB → **12.7 MB** (12.2 MiB). CI and the release
  workflow inherit the workspace profile, so these sizes apply to all native
  targets. Trade-off: minimal autovectorization loss vs the hard 20 MB
  ceiling; UI remains fully responsive (verified by launching the release
  binary against a test profile).
* **Web suite E2E (same session)** — local calendar-only run
  (`SUITE_DISABLE_NOTES=1`): `/` 303 → `/calendar.html`, `/api/health` ok,
  notes routes unmounted (`/notes.html` 303, `/api/notes` 404), login wrong
  password → 401, login admin → 200 + session cookie, `/api/auth/me` ok,
  calendar create/list/patch/delete, event create/list-by-range/single-get/
  PUT-update/persisted-verify/delete → 204 → 404, logout → 401. Test
  artifact DB wiped afterwards.
* **Icon fix** — `IconPin` outline now uses the webapp's exact Material
  `push_pin` path with `fill-rule: evenodd` (inner cut-out), so the pinned
  (filled) and unpinned (outline) states share one silhouette.
* **Web-parity loops (2026-11-25, Mint Xvfb :99 + scrot/xdotool; test profile
  `/tmp/notes-vm-test`, `YELLOD_NOTES_OPEN_LAST=0`, `SLINT_SCALE_FACTOR`
  default 1.25)** — editor More popover rebuilt to match `#ed-more-pop`
  exactly: width `min(300px, vpw-16px)` (was 252), circular colour dots
  (38 px, 6×2, gap 8, selected ring 2 px `var(--text)` inset −4 px — was
  31 px rounded squares), uppercase `BACKGROUND`/`FORMATTING` labels
  (11 px, 500, letter-spacing 0.04 em, `var(--text-sec)` — was 12 px
  title-case `text-faint`), formatting grid 4-column × 4-row in `MD_TOOLS`
  order (`B I U S / code H1 H2 H3 / quote list 1. task / link wiki table
  div`, height 32 px, `var(--icon-hover-bg)`, radius 3 px, font 11.5 px —
  was 6/6/4, height 30, font 10), pop-items padding 9 px 10 px, font
  13.5 px, svg 18 px, radius 5 px, border `var(--border)`, shadow
  `var(--shadow-2)` approximation, height 510 px (was 404 — clipped
  "Download .md"), anchored to the More-button left edge (was screen-centre
  heuristic). Verified `vm-nem7.png` vs `web #ed-more-pop`: circles,
  uppercase labels, 4-col formatting, no clipping.
  Light board (`vm-nlight2.png`: white bg, `#DDD` borders, yellow
  `drawer-active` selected, 2-col Pinned, filled pins) and Compact
  `<720px` (`vm-ncompact2.png`: collapsed rail, full-width composer,
  2-col Pinned, 3-col Others) re-verified with self-contained
  `nohup Xvfb` loops (separate Xvfb start previously died on SSH exit,
  leaving stale screenshots — now the loop starts and owns its display).
  Notion (uniform 96 px rows, single-line elided previews) and Vault
  preview clipping verified (`vm-nlay1e.png`/`vm-nlay2c.png`); card
  previews use `ycore::model::strip_markdown` for plain-text snippets.
  Visible debug instance relaunched on `DISPLAY=:0` (pid 1969344) from
  `/tmp/notes-build/target/debug/notes` with the test profile.
  `cargo test --workspace` (ycore 15/15) and `cargo clippy --workspace
  --all-targets -- -D warnings` green; Windows release verified
  2026-11-25: `notes.exe` 16,801,280 bytes (16.0 MB) and `calendar.exe`
  12,839,424 bytes (12.2 MB), both under the 20 MB ceiling (fresh
  fat-LTO rebuild, timestamps 25 Nov 2026). Linux release on Mint was
  still compiling (fat-LTO deps) at time of writing — superseded by the
  second-pass entry below (19,779,160 bytes after `opt-level = "z"`).
  Still to loop at that point: search highlights, toast/undo, drag
  visuals, editor checklist/read/find/images/doodle/timestamps,
  Neomorph and Graph side-by-side — all covered in the second pass.
* **Component/icon audit (2026-11-25)** — web-facing icons converted from
  Lucide strokes to the webapp's exact Material filled paths (search, menu,
  X, plus, archive/unarchive, trash, settings gear, layout grid, image,
  edit, download, chevrons, checklist box, external-link, lock, check;
  new `IconRestore`/`IconHistory`/`IconNoteFile`/`IconDoodleUndo`), so
  glyphs no longer blur or misread at 17–22 px. `IconButton` is now web
  geometry (radius 25 %, hover/active `--icon-hover-bg`). std-widgets
  `CheckBox` replaced everywhere by `NoteCheckBox` (20 px editor/composer,
  18 px settings, 15 px cards; centred Material check), and checklist
  notes render real card checkboxes (clickable, Rust-toggled). Card pin
  moved to the top-right title corner — solid when pinned, outline on
  hover when not — and left the bottom action row. The composer now lives
  inside the board's scroll area (web page flow: it scrolls away instead
  of sticking), board cards FLIP-animate `x`/`y` over 190 ms like
  `playFlip`, and the editor find bar animates height/opacity 150 ms.
  Vault list is single-line web rows (260 px side, 13.5 px titles, pin
  glyph + image count), Graph sits on the web's black stage with its
  bottom toolbar, and Settings is a full page (720 px column, 15 px
  group headings, dividers) instead of a modal card. Card height ceiling
  halved: `estimate_card_height_px` now clamps to 1000 px (was 2000 px).
* **Parity loops, second pass (2026-11-25, same Mint Xvfb rig)** — search
  results now render (canvas height includes `composer-space`, so result
  cards are no longer clipped); timestamps dialog verified end-to-end
  (Created / Last edited / ACTIVITY log) with its Close pill back inside
  the card (dropped the redundant `height: 100%` on its Flickable);
  single-click editor open, find bar, editor More popover, card colour
  popover all re-verified (`a19`/`a20`). Two real bugs found and fixed:
  (1) drag-release also fired the card `clicked`, opening the editor after
  every drag — `NoteCard` now latches `suppress-click` when a drag begins
  (`apps/shared-ui/components.slint`); (2) the editor find bar stole focus
  back to the body on every keystroke (`bodyInput.focus()` in the
  find-selection handler), so typing after the first character edited the
  note text — focus now stays in the find field, Enter/Shift+Enter step
  matches, Esc returns focus to the body (`apps/notes/ui/editor.slint`),
  verified `e5` (query `roadmap`, `1 / 1`, body intact). Also verified:
  card image strip (cover + `+n more` chip) and editor attachment strip
  on a seeded 2-image note (`b2`/`f6`), Archive toast (`Archived`, web
  `.toast` geometry), mid-drag ghost, Settings as full page with working
  Done exit, all five layouts incl. Neomorph (borderless `#202124`
  cards), doodle canvas with stroke/undo (`c3`). The single-pinned-card
  full-width section matches this webapp's own `layoutMasonry()` clamp
  (it is not Google Keep behaviour). `opt-level` switched `s`→`z` in the
  workspace release profile: Linux release was 21,217,240 bytes (>20 MB),
  now 19,779,160 bytes; final Windows releases `notes.exe` 14,930,944
  bytes and `calendar.exe` 12,211,200 bytes — all ≤ 20 MB.
  `cargo test --workspace` (ycore 15/15) and workspace clippy green.
  Visible instance on Mint `DISPLAY=:0` runs the release binary with
  the test profile. **Known remaining gaps:** search-result `<mark>`
  highlighting (web `--mark-bg`) is not rendered — Slint plain `Text`
  has no per-span background and `StyledText` segments cannot be built
  publicly; filtering itself is exact. Long checklist items in the
  editor stay single-line (web wraps). Everything else looped this pass
  matched the web reference.
