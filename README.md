# OrchestraTerm

OrchestraTerm is a macOS GUI terminal app with a multi-pane engine implemented in this repository.
The team orchestration model in this project was designed with the Claude Code Agent Teams guide in mind:
https://code.claude.com/docs/ko/agent-teams

- License: MIT
- Repository focus: GUI terminal + orchestration team engine for macOS

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

### Prerequisites (macOS)

- macOS (Apple Silicon target is currently packaged)
- Xcode Command Line Tools (`xcode-select --install`)
- Rust toolchain (`rustup` + stable toolchain)
- `hdiutil` (bundled with macOS)

### Build Steps

1. Build and package app + DMG

```bash
./scripts/release-macos.sh
```

2. Verify checksum and DMG metadata

```bash
./scripts/verify-release.sh
```

### Output

- DMG: `dist/orchestraterm-0.2.0-macos-arm64.dmg`
- SHA256: `dist/orchestraterm-0.2.0-macos-arm64.sha256`

### Install / Run from DMG

1. Open `dist/orchestraterm-0.2.0-macos-arm64.dmg`
2. Drag `OrchestraTerm.app` to `Applications`
3. Launch `OrchestraTerm.app`

### Agent Teams Design Notes

This project includes team orchestration primitives aligned to the ideas from the Agent Teams guide:

- Team display mode: `in_process`, `split_pane`, `auto`
- Delegation-only operation mode
- Plan approval gate before task execution
- Dependency-aware task claiming (`blocked` -> `pending` -> `in_progress` -> `done`)
- Team messaging with priority and read/unread workflow
- Member lifecycle controls (terminate/restart/prune) and recovery policy

## License

MIT. See `LICENSE`.

## Author
jin younghwa

## Contact

- Email: timotolkie@gmail.com
- linkedin: https://www.linkedin.com/in/younghwa-jin-05619643/ 

