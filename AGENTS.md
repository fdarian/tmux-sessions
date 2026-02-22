# tmux-sessions

A Rust TUI reimplementation of tmux's `choose-tree` — a tree-based session/window/pane picker with preview.

## Architecture

**TEA (Elm Architecture)**: App state + Action enum + pure update function + render function.

```
src/
  main.rs    — entry point, terminal setup/teardown, event loop
  app.rs     — App state, Mode, handle_action (TEA update)
  tmux.rs    — all tmux command interaction (list/kill/switch/capture)
  tree.rs    — NodeId enum, build TreeItem list from tmux data
  ui.rs      — render: layout split, tree widget, preview, confirmation overlay
  event.rs   — map KeyEvent + Mode → Action enum
```

## Key Conventions

- **Delimiter**: `\x1f` (ASCII unit separator) in tmux format strings to avoid issues with names containing colons
- **No destructuring**: Access struct fields directly (`obj.field`), never `let { field } = obj`
- **No dummy/fallback values**: Propagate errors properly, don't use `unwrap_or("")` style fallbacks
- **Tree ownership**: `TreeItem<'static, NodeId>` — items own their display text, rebuilt on each refresh
- **NodeId**: Enum with `Session(id)` / `Window(session_id, window_id)` / `Pane(session_id, window_id, pane_id)` — used as tree identifier and for resolving actions

## Dependencies

- `ratatui` — TUI framework
- `crossterm` — terminal backend
- `tui-tree-widget` — tree widget for ratatui

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
