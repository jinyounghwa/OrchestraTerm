# OrchestraTerm

OrchestraTerm is a macOS GUI terminal app with a multi-pane engine implemented in this repository.

## What Is Implemented

- Independent pane/session core (no tmux runtime dependency)
- Pane operations: split horizontal/vertical, focus move, close, zoom
- Per-pane interactive PTY shell (`/bin/zsh -i`)
- ANSI rendering (16/256/truecolor), wide-char handling (Korean/CJK), blinking cursor
- Right-side fixed shortcuts panel
- Workspace folder picker (`Open Folder`) and pane-wide `cd` sync
- Team engine + server/CLI (mode, delegation, plan gating, task deps, messages, usage)
- macOS app + DMG packaging scripts with icon assets

## Run

```bash
cargo run
```

## Keyboard Shortcuts

- `Ctrl+B, S`: split horizontally
- `Ctrl+B, V`: split vertically
- `Ctrl+B, X`: close focused pane
- `Ctrl+B, Z`: zoom toggle
- `Ctrl+B, Arrow`: focus move
- `Ctrl+Enter`: send Enter to focused terminal
- `Cmd+O`: open workspace folder
- `Ctrl+B, [`: copy mode
- `Copy mode /`: search
- `Copy mode Space + Enter`: copy selection

## Render Presets

Selectable in right panel (`Render Preset`):

- `Balanced`
- `Compact`
- `Pixel`

## Team CLI

```bash
orchestraterm server start
orchestraterm team create frontend --mode auto
orchestraterm team add-member frontend lead --lead
orchestraterm team add-member frontend worker --require-plan-approval
orchestraterm team add-task frontend "Implement parser" --deps 0 --files src/gui.rs,src/terminal.rs
orchestraterm team submit-plan frontend 1 "steps..."
orchestraterm team plan frontend 1 --status approved
orchestraterm team claim frontend 1 0
orchestraterm team done frontend 1 0 --input-tokens 1000 --output-tokens 300 --cost-usd 0.02
orchestraterm team message frontend --from-member 0 --to-member 1 --priority high "handoff"
orchestraterm team messages frontend --viewer-member 1 --unread-only
orchestraterm team read-message frontend 1 0
orchestraterm team usage frontend
```

Supported team features:

- Modes: `in_process`, `split_pane`, `auto`
- Delegation-only toggle
- Plan submit/approve/reject gate
- Dependency-based task states (`pending`, `blocked`, `in_progress`, `done`)
- Auto-claim next available task
- Message priority + unread/read tracking
- Member terminate/restart/prune + recovery policy
- Token/cost usage aggregation and file-conflict avoidance

## Test

```bash
cargo test --all-targets
```

Current compatibility tests include:

- ASCII + cursor movement
- ANSI 256/truecolor parsing
- Korean wide-cell tracking
- Erase-in-line (`CSI K`)
- Alternate screen (`?1049h` / `?1049l`)
- ANSI block char (`█`) color preservation

## Package DMG

```bash
./scripts/release-macos.sh
./scripts/verify-release.sh
```

Output:

- `dist/orchestraterm-0.2.0-macos-arm64.dmg`
