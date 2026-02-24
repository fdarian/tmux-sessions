# tmux-sessions

A drop-in replacement for tmux's `choose-tree`.

Same tree. Same keybindings. Same muscle memory. But faster, with fuzzy search and live pane previews rendered with full ANSI colors — and you can finally give your sessions readable names.

## Install

```sh
cargo install --path .
```

Bind it in your `tmux.conf` (replacing the default `choose-tree`):

```tmux
bind-key s display-popup -E -w 100% -h 100% tmux-sessions
```

> Must be run inside a tmux session.

## Custom Session Names

tmux names sessions by number. That's fine for two sessions. Less fine for twelve.

`tmux-sessions` lets you pipe each session name through your own script, so `0`, `1`, `2` can become `dotfiles`, `api-server`, `scratch` — or whatever makes sense to you.

### Setup

Create a config file at `~/.config/tmux-sessions/config.json`:

```json
{
  "formatter": "/path/to/your/format-session.sh"
}
```

The formatter receives the raw session name as its first argument and should print the display name to stdout.

**Example** — a script that maps session names from a lookup file:

```bash
#!/usr/bin/env bash
# ~/.local/bin/format-session.sh

name=$(grep "^$1=" ~/.config/tmux-sessions/names.txt | cut -d= -f2-)
echo "${name:-$1}"
```

With `~/.config/tmux-sessions/names.txt`:

```
0=dotfiles
1=api-server
2=scratch
```

The raw tmux name is always used under the hood for commands — the formatter only changes what you see.

If the formatter fails or returns nothing for a session, that session simply shows its raw name. No config file at all? Everything works as usual.

## Keybindings

Navigation mirrors tmux's native `choose-tree` and vim motions:

| Key | Action |
|-----|--------|
| `j` / `↓` / `C-n` | Move down |
| `k` / `↑` / `C-p` | Move up |
| `h` / `←` | Collapse / go to parent |
| `l` / `→` | Expand / go to child |
| `Space` | Toggle expand/collapse |
| `Enter` | Switch to selected |
| `x` | Kill selected (asks first) |
| `/` | Fuzzy search |
| `r` | Refresh |
| `q` / `Esc` | Quit |
| `0-9` / `M-a..z` | Jump to entry by index |

## License

Apache-2.0
