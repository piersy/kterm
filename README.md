# kterm

A terminal UI for browsing and managing Kubernetes resources. Navigate clusters, namespaces, and resources (Pods, PVCs, StatefulSets) with real-time updates, view descriptions and logs, and perform actions -- all without leaving the terminal.

```
+--------------------------------------------------------------------+
| Context: [gke-prod v]  | Namespace: [default v]  | Type: [Pods v] |
+--------------------------------------------------------------------+
| NAME               | STATUS  | AGE  | RESTARTS | NODE             |
|--------------------------------------------------------------------|
| > my-pod-0         | Running | 3d2h | 0        | node-a1          |
|   my-pod-1         | Pending | 1h   | 0        | <none>           |
+--------------------------------------------------------------------+
| q:Quit Tab:Selector j/k:Nav Enter:Detail l:Logs d:Delete r:Restart|
+--------------------------------------------------------------------+
```

## Features

- **Multi-cluster support** -- switch between kubeconfig contexts on the fly
- **Resource browsing** -- Pods, PersistentVolumeClaims, StatefulSets with type-specific columns
- **Real-time updates** -- watches resources via the Kubernetes API; changes appear automatically
- **Detail view** -- formatted description with conditions, containers, events, and full YAML
- **Log streaming** -- tail pod logs with follow mode, scroll through history
- **Actions** -- delete, restart (rollout restart for StatefulSets), edit YAML in `$EDITOR`
- **Filtering** -- search resources by name with `/`
- **Color-coded status** -- green for Running/Bound, yellow for Pending, red for Failed/CrashLoopBackOff

## Install

### Prerequisites

- **Rust 1.75+** -- install via [rustup](https://rustup.rs/)
- **A valid kubeconfig** at `~/.kube/config` (or `$KUBECONFIG`)

### From crates.io

```sh
cargo install kterm
```

### From GitHub

```sh
cargo install --git https://github.com/piersy/kterm.git
```

### From source

```sh
git clone https://github.com/piersy/kterm.git
cd kterm
cargo build --release
```

The binary is at `./target/release/kterm`. Copy it somewhere on your `$PATH`:

```sh
cp target/release/kterm ~/.local/bin/
```

### From a local checkout

```sh
cargo install --path .
```

This builds and installs `kterm` to `~/.cargo/bin/`, which is on your `$PATH` if you used rustup.

## Usage

```sh
kterm
```

The app reads your kubeconfig and connects to the current context. If no cluster is reachable, it starts in offline mode.

## Keybindings

### Global

| Key | Action |
|-----|--------|
| `q` / `Ctrl+c` | Quit (or back from subview) |
| `Tab` / `Shift+Tab` | Cycle focus: Context -> Namespace -> Type -> List |
| `?` | Help overlay |

### Selector focused (Context / Namespace / Type)

| Key | Action |
|-----|--------|
| `h` / `Left` | Previous value |
| `l` / `Right` | Next value |

### Resource list focused

| Key | Action |
|-----|--------|
| `j` / `Down` | Move selection down |
| `k` / `Up` | Move selection up |
| `Enter` | Open detail view |
| `l` | View logs (Pods only) |
| `d` | Delete (with confirmation) |
| `r` | Restart (with confirmation) |
| `e` | Edit YAML in `$EDITOR` |
| `/` | Filter by name |

### Detail view

| Key | Action |
|-----|--------|
| `Esc` | Back to list |
| `j` / `k` | Scroll up/down |
| `g` / `G` | Jump to top/bottom |
| `l` | View logs |
| `d` | Delete |
| `r` | Restart |
| `e` | Edit |

### Logs view

| Key | Action |
|-----|--------|
| `Esc` | Back to list |
| `f` | Toggle follow mode |
| `j` / `k` | Scroll up/down |
| `g` / `G` | Jump to top/bottom |

## Architecture

```
src/
  main.rs             Entry point, terminal setup, async event loop
  app.rs              App state, key handling, action dispatch
  event.rs            AppEvent enum, EventHandler (crossterm + tick + K8s)
  types.rs            ResourceType, ViewMode, Focus, ResourceItem
  ui/
    mod.rs            Top-level render(), layout splitting
    header.rs         Context/namespace/type selector bar
    resource_list.rs  Table widget with resource rows
    detail.rs         Scrollable description panel
    logs.rs           Log viewer with follow mode
    help.rs           Footer keybindings, confirmation dialog
  k8s/
    mod.rs            Re-exports
    client.rs         K8sManager: kubeconfig, context switching
    resources.rs      Watch streams, describe, resource conversion
    actions.rs        Delete, restart, edit/apply YAML
    logs.rs           Pod log streaming
```

The event loop multiplexes three sources into a single `tokio::sync::mpsc` channel:
1. **Crossterm** -- keyboard and resize events
2. **Tick timer** -- 250ms interval for UI updates (spinner, error timeout)
3. **K8s watcher** -- `kube::runtime::watcher` streams with `BTreeMap` caching

## Testing

```sh
cargo test
```

65 tests:
- **34 unit tests** -- key handling, state transitions, type logic (`src/app_test.rs`)
- **31 integration tests** -- full UI rendering via ratatui `TestBackend`, verifying rendered output for all views and navigation flows (`src/ui_test.rs`)

## Stack

- [ratatui](https://ratatui.rs) + [crossterm](https://docs.rs/crossterm) -- TUI rendering and input
- [kube-rs](https://kube.rs) + [k8s-openapi](https://docs.rs/k8s-openapi) -- Kubernetes API client
- [tokio](https://tokio.rs) -- async runtime

## License

MIT
