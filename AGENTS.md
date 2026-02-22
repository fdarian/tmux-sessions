# tmux-sessions

A Rust TUI reimplementation of tmux's `choose-tree` — a tree-based session/window/pane picker with preview.

## Architecture

**TEA (Elm Architecture)**: App state + Action enum + pure update function + render function.

```
src/
  main.rs    — entry point, terminal setup/teardown, event loop
  app.rs     — App state, Mode, handle_action (TEA update)
  tmux.rs    — all tmux command interaction (list/kill/switch/capture)
  tree.rs    — NodeId enum, FlatEntry struct, flatten/format_line for tree rendering
  ui.rs      — render: vertical layout, List-based tree, preview, confirmation overlay
  event.rs   — map KeyEvent + Mode → Action enum
```

## Key Conventions

- **Delimiter**: `\x1f` (ASCII unit separator) in tmux format strings to avoid issues with names containing colons
- **No destructuring**: Access struct fields directly (`obj.field`), never `let { field } = obj`
- **No dummy/fallback values**: Propagate errors properly, don't use `unwrap_or("")` style fallbacks
- **Flat-entry model**: Tree is flattened into `Vec<FlatEntry>` based on which nodes are in the `opened` set, rebuilt on expand/collapse/refresh
- **NodeId**: Enum with `Session(id)` / `Window(session_id, window_id)` / `Pane(session_id, window_id, pane_id)` — used as tree identifier and for resolving actions
- **Tree rendering**: Manual connector characters (`├>`, `└>`, `│`) and `+`/`-` symbols matching tmux's native choose-tree

## Dependencies

- `ratatui` — TUI framework
- `crossterm` — terminal backend

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
| `Space` | Toggle expand/collapse |
| `Enter` | Switch to selected |
| `x` | Kill selected (with confirmation) |
| `r` | Refresh tree |
| `q` | Quit |
