# tmux-sessions

A Rust TUI reimplementation of tmux's `choose-tree` — a tree-based session/window/pane picker with preview.

## Architecture

**TEA (Elm Architecture)**: App state + Action enum + pure update function + render function.

```
src/
  main.rs    — entry point, terminal setup/teardown, unified AppEvent loop, 4 worker threads
  app.rs     — App state, Mode, handle_action (TEA update), PreviewPane struct
  config.rs  — optional config loading (~/.config/tmux-sessions/config.json), format_session_name
  create.rs  — create-session popup sources: history/worktree/zoxide tabs and candidate types
  tmux.rs    — all tmux command interaction (list/kill/switch/capture); move_window, capture_pane_raw, get_mode_style, parse_style functions
  tree.rs    — NodeId enum, FlatEntry struct, flatten/format_line for tree rendering
  history.rs — recently-closed session history (~/.config/tmux-sessions/history.json): load/prune, upsert live sessions
  ui.rs      — render: vertical layout, List-based tree, preview, confirmation overlay
  event.rs   — map KeyEvent + Mode → Action enum
  procs/     — process monitor: pane enumeration, ps parsing, subtree ownership (mod.rs); procs/tree.rs holds MonitorEntry + flatten_process_tree, the pure ppid-nesting/subtree-sort/collapse logic behind the monitor's tree view
```

### Threading / event model

`main.rs` owns a single `mpsc::Receiver<AppEvent>` and four worker threads:

- **Input thread**: polls crossterm events → sends `AppEvent::Input`
- **Capture worker**: receives `CaptureRequest`, runs `tmux capture-pane` (blocking, off UI thread) → sends `AppEvent::CaptureDone { generation, node_id, panes }`
- **Formatter worker**: receives `FormatRequest`, runs the configured formatter script → sends `AppEvent::NameFormatted { raw_name, formatted }`
- **Worktree worker**: receives `WorktreeCreateRequest { generation, command, branch, cwd }`, runs the configured `worktree_create_command` with its output captured (not inherited, so it can't corrupt the alternate screen) → sends `AppEvent::WorktreeCreateDone { generation, branch, result }`

The main loop blocks on `recv` (or `recv_timeout` when Monitor mode or debounce is pending). On timeout: dispatch the debounced capture request and/or tick the monitor.

### Preview caching and debounce

`update_preview()` sets `pending_preview_request` with a 40ms deadline (not immediate). `dispatch_capture_request` only sends to the worker when `Instant::now() >= deadline`. This debounces `j`/`k` scrolling to ~one capture when the cursor settles.

`preview_cache: HashMap<NodeId, Vec<PreviewPane>>` stores the last successful capture per node. On selection change: show cached content immediately (SWR), kick a background refresh. Show "capturing..." only when no cache entry exists.

### Formatter caching (SWR)

Sessions start with raw names. `formatter_cache: HashMap<String, String>` is in-memory only. On startup and after refresh, uncached live sessions are enqueued to the formatter worker. Dead sessions are formatted lazily when the filter (`/`) is active. `apply_name_formatted()` updates `display_name` for matching live and dead sessions and rebuilds flat_entries.

## Key Conventions

- **Delimiter**: `\x1f` (ASCII unit separator) in tmux format strings to avoid issues with names containing colons
- **Target sessions by id, not name**: tmux `-t` targets must always be ids (`$N` / `@N` / `%N`) for a session — never the session name. tmux splits the target on `.` looking for a `session.pane` component, so any session name containing a dot (worktree paths under `.claude/`) misresolves; `=name` exact-match syntax does not avoid this. Names are still correct for `new-session -s` and rename's new-name argument.
- **No destructuring**: Access struct fields directly (`obj.field`), never `let { field } = obj`
- **No dummy/fallback values**: Propagate errors properly, don't use `unwrap_or("")` style fallbacks
- **Flat-entry model**: Tree is flattened into `Vec<FlatEntry>` based on which nodes are in the `opened` set, rebuilt on expand/collapse/refresh
- **NodeId**: Enum with `Group(prefix)` / `Session(id)` / `Window(session_id, window_id)` / `Pane(session_id, window_id, pane_id)` — used as tree identifier and for resolving actions; `Group` nodes are no-ops for kill/pin. `Enter` on a `Group` switches to its `@` peer session (if one exists), same as pressing `Enter` on the `@` row directly.
- **Tree rendering**: Manual connector characters (`├─>`, `└─>`, `│`) and `+`/`-` symbols matching tmux's native choose-tree
- **Mode-style**: tmux mode-style is read at startup to derive `highlight_style` and `primary_color`

## Configuration

Optional config file at `~/.config/tmux-sessions/config.json`:

```json
{
  "formatter": "/path/to/format-session.sh",
  "group_name_separator": "/",
  "recents": { "enabled": true, "max_age_secs": 3600 },
  "zoxide": true,
  "worktree_create_command": "wt switch -y -c {branch}"
}
```

- **formatter**: Path to a script that receives the raw session name as its first argument and prints the formatted name to stdout
- **group_name_separator**: Groups sessions by the prefix before the first occurrence of this separator in their `display_name`. Sessions without the separator appear ungrouped at the root level. Groups start expanded and can be collapsed/expanded with `h`/`l`. Pinned sessions are pulled out of their group and shown at the top (with the same separator as in flat mode); group counts reflect only unpinned members. A session whose `display_name` exactly equals a group's prefix (e.g. `fdarian/rheya` alongside `fdarian/rheya/artifacts-tools`) is folded into that group as its first child, displayed as `@` instead of repeating the prefix.
- **recents**: Optional opt-in view config. `enabled: true` turns on a labeled `recents` section. `max_age_secs` defaults to `3600` when omitted and limits eligibility to live sessions whose `session_activity` is within that age window.
- **zoxide**: Enables the create-session popup's zoxide tab when set to `true` and the `zoxide` binary is installed
- **worktree_create_command**: Template command to create a new git worktree. `{branch}` is substituted with the typed branch name. Run directly (no shell), cwd = the create-session popup's resolved cwd (see Create session below). After running, git worktree list is re-queried to find the new worktree path; the session is created there. When set, the Worktree tab appears whenever cwd is inside any git repo (not just repos with >1 existing worktree). Example: `"wt switch -y -c {branch}"`.
- Missing config file → raw session names used (no error)
- Invalid JSON → app fails to start with error
- Formatter failure (missing script, non-zero exit, empty output) → per-session fallback to raw name
- `Session.name` is always the raw tmux name (used for tmux commands); `Session.display_name` is what the UI shows

## Recents

- `recents` is a view, not a partition: eligible sessions still appear in the main tree.
- It is opt-in via config and only considers live sessions at the session level.
- Eligible sessions must be recent enough, not pinned, and not hidden.
- The section renders as a labeled `recents` block between pinned and main. With grouping enabled, recents groups are ordered by their most-recent eligible child and contain only recent children, not the full group.
- Recent rows use distinct wrapped `NodeId`s so they can be expanded independently while actions still target the underlying session/window/pane.

## Reopening closed sessions

- Every refresh snapshots live sessions (name + `#{session_path}` cwd + `last_seen`) into `~/.config/tmux-sessions/history.json` (`history::upsert_live_sessions`). A name in history but not in the live list is a "dead" session.
- Dead sessions surface **only** in `/` filter results — `flatten_filtered` fuzzy-scores them and appends below live matches, dimmed (`Modifier::DIM`), as `NodeId::DeadSession(name)`. They never appear in the unfiltered tree, and are no-ops for pin/kill/preview.
- `Enter` on a dead session resumes it: `tmux new-session -d -s <name> -c <cwd>` then `switch-client` (single window at the captured cwd — no layout restore).
- History is pruned on load: entries older than 30 days (`HISTORY_MAX_AGE_SECS`) or whose `cwd` no longer exists are dropped.

## Dependencies

- `ratatui` — TUI framework
- `crossterm` — terminal backend
- `ansi-to-tui` — ANSI escape sequence to ratatui Text conversion
- `serde` + `serde_json` — config file deserialization

## Build & Run

```sh
cargo build
# Must be run inside tmux:
cargo run
```

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `h` / `←` | Collapse / parent |
| `l` / `→` | Expand / child |
| `Space` | Open fullscreen preview |
| `Enter` | Switch to selected |
| `p` | Toggle pin selected session |
| `Shift+H` | Hide / unhide selected session |
| `.` | Reveal / collapse hidden sessions |
| `Shift+K` / `Shift+J` | Move pinned session up / down (no-op if not pinned) |
| `x` | Kill selected, or batch-delete the visual selection (with confirmation) |
| `r` | Rename selected (session/window) |
| `v` | Toggle visual selection mode (j/k extends the range) |
| `M` | Move selected windows to a session |
| `o` | Create/open a new session (history / worktree / zoxide) |
| `c` | Quick-create a session: type a name and press Enter (empty name gets a random slug) |
| `R` | Refresh tree |
| `m` | Open process monitor |
| `q` | Quit |
| `Esc` | Clear the selection / exit selection mode, otherwise quit |

In move-window mode:
- type to search sessions or enter a new session name
- `↓` / `Ctrl-N` — move down through candidates
- `↑` / `Ctrl-P` — move up through candidates
- `Enter` — confirm move (or create the target session, then move)
- `Esc` — cancel

## Create session

Press `o` to open a create/resume popup with Tab / Shift+Tab cycling across the available sub-tabs. The popup's cwd is the highlighted tree row's session cwd (its dead session cwd for a `NodeId::DeadSession` row, its `@` peer session's cwd for a `Group` row), falling back to the process cwd when the row doesn't resolve to a session (empty tree, a separator/header row, or a `Group` with no `@` peer). Every "cwd" below refers to this resolved value.

- **History** — always visible. Fuzzy-matches recently closed sessions and can resume them or create a new named session from the current query.
- **Worktree** — visible when the cwd is inside a git repo with linked worktrees (>1 entry in `git worktree list --porcelain`), OR whenever `worktree_create_command` is configured and cwd is inside any git repo (even with 0 linked worktrees). When `worktree_create_command` is set and the query matches no existing branch, a synthetic "+ Create worktree" candidate appears at the bottom; Enter hands the branch off to the worktree worker thread and switches to `Mode::CreatingWorktree`, showing `Creating worktree "<branch>"…` in place of the candidate list while only `Esc` is active. The worker runs the configured command (output captured, not inherited) and re-queries `git worktree list --porcelain` for the new path. On success the session list is refreshed and the app switches to the live session the command already created at that path (matched by cwd), only creating a new one if none matches, then quits. On failure — or if no session can be switched to — the popup stays open in `Mode::CreateSession` with the error shown via `create_load_error`, the same surface used for tab-load failures. `Esc` during `Mode::CreatingWorktree` returns to Normal immediately; a result that arrives afterward still refreshes the tree but does not switch or quit.
- **Zoxide** — visible only when `"zoxide": true` is set in `config.json` and `zoxide` is installed on `PATH`.

In create-session mode:
- type to search candidates or enter a new session name
- `Tab` / `Shift+Tab` — cycle tabs
- `↓` / `Ctrl-N` — move down through candidates
- `↑` / `Ctrl-P` — move up through candidates
- `Enter` — switch to an existing live session, resume a dead one, or create a new one
- `Esc` — cancel

`c` opens a separate, minimal quick-create prompt (`Mode::QuickCreate`) instead of the `o` popup: type a name and press Enter to create + switch to a new session in the cwd, or press Enter on an empty buffer for a random `adjective-noun` slug. `Esc` cancels.

In process monitor mode:
- `j` / `↓` — move down
- `k` / `↑` — move up
- `h` / `←` — collapse selected, or jump to parent if already collapsed
- `l` / `→` — expand selected, or descend to first child if already expanded
- `s` — toggle sort (MEM / CPU)
- `Space` — process detail popup
- `Enter` — switch to owning pane
- `x` — kill selected process (with confirmation)
- `Esc` / `q` — return to tree

In process detail popup:
- `Space` / `Esc` / `q` — close popup

In fullscreen preview mode:
- `h` / `←` — previous pane
- `l` / `→` — next pane
- `Enter` — switch to previewed pane
- `Esc` — return to tree
