# herdr-nav

Pane, tab, and workspace navigation plugin for [herdr](https://herdr.dev).

Replaces shell-script keybindings that shell out to the `herdr` CLI
per navigation step (each a fresh process spawn) with a single
compiled binary that talks to herdr's local socket API directly. See
[How it's fast](#how-its-fast) for benchmarks.

## Install

```sh
herdr plugin link /path/to/herdr-nav
cargo build --release --locked   # link does not run build steps
```

There is no separate plugin update in v1; reinstall from the git host
to refresh a managed plugin.

## Actions

| Action             | Behavior                                                                             |
| ------------------- | ------------------------------------------------------------------------------------- |
| `nav-left`          | Move focus left; cross into the adjacent tab (entry-aligned) at the pane edge.        |
| `nav-right`         | Same, moving right.                                                                    |
| `tab-left`          | Move to the previous tab in the current workspace; cross into the adjacent workspace's entry tab at the tab edge. |
| `tab-right`         | Same, moving right.                                                                    |
| `workspace-left`    | Cycle to the previous workspace (wraps), landing on its entry-side pane.               |
| `workspace-right`   | Same, moving right.                                                                    |

## Keybindings

Add to `~/.config/herdr/config.toml` (reload with
`herdr server reload-config`):

```toml
[[keys.command]]
key = "alt+left"
type = "plugin_action"
command = "herdr-nav.nav-left"

[[keys.command]]
key = "alt+right"
type = "plugin_action"
command = "herdr-nav.nav-right"

[[keys.command]]
key = "alt+shift+left"
type = "plugin_action"
command = "herdr-nav.tab-left"

[[keys.command]]
key = "alt+shift+right"
type = "plugin_action"
command = "herdr-nav.tab-right"

[[keys.command]]
key = "ctrl+alt+left"
type = "plugin_action"
command = "herdr-nav.workspace-left"

[[keys.command]]
key = "ctrl+alt+right"
type = "plugin_action"
command = "herdr-nav.workspace-right"
```

## How it's fast

herdr's socket server closes the connection after every response (one
request per connection), so the win isn't a persistent multiplexed
session — it's replacing repeated `herdr <subcommand>` process spawns
(fork + exec + dynamic link) with repeated Unix socket connects from
one already-running compiled process (~0.03ms each). Benchmarked with
`hyperfine`:

| | time |
| --- | --- |
| pane nav | 3.0ms |
| tab/workspace cycle | 8.7ms |

## Development

```sh
cargo build --release
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo test
```

## License

MIT
