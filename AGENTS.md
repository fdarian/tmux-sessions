# tmux-sessions

A Rust TUI reimplementation of tmux's `choose-tree` — a tree-based session/window/pane picker with preview.

## Architecture

**TEA (Elm Architecture)**: App state + Action enum + pure update function + render function.

```
src/
  main.rs    — entry point, terminal setup/teardown, event loop
  app.rs     — App state, Mode, handle_action (TEA update), PreviewPane struct
  config.rs  — optional config loading (~/.config/tmux-sessions/config.json), session name formatter
  tmux.rs    — all tmux command interaction (list/kill/switch/capture); move_window, capture_pane_raw, get_mode_style, parse_style functions
  tree.rs    — NodeId enum, FlatEntry struct, flatten/format_line for tree rendering
  history.rs — recently-closed session history (~/.config/tmux-sessions/history.json): load/prune, upsert live sessions
  ui.rs      — render: vertical layout, List-based tree, preview, confirmation overlay
  event.rs   — map KeyEvent + Mode → Action enum
  procs.rs   — process monitor: pane enumeration, ps parsing, subtree ownership
```

## Key Conventions

- **Delimiter**: `\x1f` (ASCII unit separator) in tmux format strings to avoid issues with names containing colons
- **No destructuring**: Access struct fields directly (`obj.field`), never `let { field } = obj`
- **No dummy/fallback values**: Propagate errors properly, don't use `unwrap_or("")` style fallbacks
- **Flat-entry model**: Tree is flattened into `Vec<FlatEntry>` based on which nodes are in the `opened` set, rebuilt on expand/collapse/refresh
- **NodeId**: Enum with `Group(prefix)` / `Session(id)` / `Window(session_id, window_id)` / `Pane(session_id, window_id, pane_id)` — used as tree identifier and for resolving actions; `Group` nodes are no-ops for select/kill/pin
- **Tree rendering**: Manual connector characters (`├─>`, `└─>`, `│`) and `+`/`-` symbols matching tmux's native choose-tree
- **Mode-style**: tmux mode-style is read at startup to derive `highlight_style` and `primary_color`

## Configuration

Optional config file at `~/.config/tmux-sessions/config.json`:

```json
{
  "formatter": "/path/to/format-session.sh",
  "group_name_separator": "/"
}
```

- **formatter**: Path to a script that receives the raw session name as its first argument and prints the formatted name to stdout
- **group_name_separator**: Groups sessions by the prefix before the first occurrence of this separator in their `display_name`. Sessions without the separator appear ungrouped at the root level. Groups start expanded and can be collapsed/expanded with `h`/`l`. Pinned sessions are pulled out of their group and shown at the top (with the same separator as in flat mode); group counts reflect only unpinned members.
- Missing config file → raw session names used (no error)
- Invalid JSON → app fails to start with error
- Formatter failure (missing script, non-zero exit, empty output) → per-session fallback to raw name
- `Session.name` is always the raw tmux name (used for tmux commands); `Session.display_name` is what the UI shows

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
| `x` | Kill selected (with confirmation) |
| `r` | Rename selected (session/window) |
| `v` | Mark / unmark highlighted window |
| `M` | Move marked windows to a session |
| `R` | Refresh tree |
| `m` | Open process monitor |
| `q` | Quit |
| `Esc` | Clear marks when any are set, otherwise quit |

In move-window mode:
- type to search sessions or enter a new session name
- `↓` / `Ctrl-N` — move down through candidates
- `↑` / `Ctrl-P` — move up through candidates
- `Enter` — confirm move (or create the target session, then move)
- `Esc` — cancel

In process monitor mode:
- `j` / `↓` — move down
- `k` / `↑` — move up
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
