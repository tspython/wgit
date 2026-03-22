# wgit — Comprehensive Project Roadmap Plan

## Executive Summary

wgit is a GPU-rendered, keyboard-first desktop Git client built in Rust with wgpu + winit. The project has a solid prototype (~5,400 lines of Rust) with status viewing, diffing, staging, commit, push/pull/fetch, syntax highlighting, and a two-pane layout. This plan maps out the path from prototype to production-grade Git client across engineering, testing, infrastructure, and product dimensions.

---

## Current State Assessment

### What's Built
- Custom wgpu + winit rendering pipeline with glyph atlas
- Repository discovery, opening, and recent repo tracking
- Three-section file list (staged / unstaged / untracked)
- Selected-file diff view with line numbers
- Stage / unstage individual files, stage all / unstage all
- Discard changes with confirmation
- Commit (summary + description)
- Fetch / pull / push operations
- Branch display and tracking status (ahead/behind)
- Syntax highlighting via tree-sitter (Rust, TOML)
- Semantic color theme system
- Two-pane layout (file tree left, diff right)
- Toolbar with action buttons
- Modal dialogs for confirmations
- Keyboard navigation and mouse support

### What's Missing
- No CI/CD pipeline
- Minimal tests (2 test cases)
- No linting/formatting config (clippy, rustfmt)
- Large monolithic app.rs (3,112 lines)
- No async command execution
- No branch management UI
- No commit history view
- No hunk/line-level staging
- No command palette or settings
- No cross-platform packaging

### Technical Debt
- `app.rs` is doing too much — event handling, layout, rendering, state management
- Several unused methods (compiler warnings)
- No structured error display in UI
- Synchronous Git operations block the UI thread
- Edition 2024 in Cargo.toml may cause compatibility issues

---

## Phase 1: Engineering Foundation (Weeks 1–3)

**Goal:** Make the codebase maintainable, testable, and CI-ready before adding features.

### 1.1 Project Infrastructure
- [ ] Add `rustfmt.toml` with project formatting rules
- [ ] Add `clippy.toml` and fix all clippy warnings
- [ ] Set up GitHub Actions CI pipeline:
  - `cargo check` on every PR
  - `cargo test` on every PR
  - `cargo clippy -- -D warnings` on every PR
  - `cargo fmt --check` on every PR
- [ ] Add branch protection rules on `main`
- [ ] Create `.github/ISSUE_TEMPLATE` for bug reports and feature requests

### 1.2 Architecture Refactor — Break Up app.rs
Split the 3,112-line `app.rs` into focused modules:

```
src/
├── app/
│   ├── mod.rs          # App struct, top-level event loop
│   ├── state.rs        # Application state model
│   ├── input.rs        # Keyboard and mouse event handling
│   ├── layout.rs       # Pane layout and sizing logic
│   ├── toolbar.rs      # Toolbar rendering and actions
│   ├── file_list.rs    # File tree pane logic
│   ├── diff_view.rs    # Diff pane logic
│   ├── modal.rs        # Modal dialog system
│   └── status_bar.rs   # Status bar rendering
├── commands/
│   ├── mod.rs          # Command trait and dispatcher
│   ├── stage.rs        # Stage/unstage commands
│   ├── commit.rs       # Commit command
│   ├── remote.rs       # Fetch/pull/push commands
│   └── branch.rs       # Branch operations
```

### 1.3 Typed Command Layer
- [ ] Define a `Command` enum for all Git operations
- [ ] Add safety tiers: `Safe`, `Guarded`, `Dangerous`
- [ ] Route all Git actions through the command layer
- [ ] Log commands for observability (command log panel later)
- [ ] Return structured `CommandResult` with success/error/warning states

### 1.4 Async Command Execution
- [ ] Introduce a background task channel (e.g., `std::sync::mpsc` or `crossbeam`)
- [ ] Run Git operations off the main thread
- [ ] Add loading/spinner state to UI during operations
- [ ] Handle operation cancellation for long-running commands

### 1.5 Test Foundation
- [ ] Add integration test harness with temp Git repos (`tempfile` crate)
- [ ] Parser tests: status parsing, diff parsing, branch parsing
- [ ] Command layer tests: stage, unstage, commit, push (against fixture repos)
- [ ] View-model tests: document generation from Git state
- [ ] Target: 80%+ coverage on `git_model.rs` and command layer

### 1.6 Clean Up Warnings
- [ ] Remove or use `side_padding()`, `top_padding()`, `build_grouped_document()`, `with_line_number()`
- [ ] Audit all `#[allow(dead_code)]` annotations

---

## Phase 2: Daily Workflow Completion (Weeks 4–7)

**Goal:** Make wgit a viable replacement for terminal Git in a normal dev loop.

### 2.1 Multi-Select and Bulk Operations
- [ ] Multi-select files with Shift+click and Ctrl+click
- [ ] Keyboard multi-select (Shift+↑/↓)
- [ ] Stage/unstage/discard selection (batch)
- [ ] Visual selection indicators

### 2.2 Commit Panel Enhancement
- [ ] Inline commit panel (not just modal)
- [ ] Summary line with character count and 72-char soft limit
- [ ] Extended description field
- [ ] Amend last commit option
- [ ] Commit message templates / recent messages
- [ ] Validation: prevent empty commits, warn on long summaries

### 2.3 Diff View Improvements
- [ ] Unified and side-by-side diff toggle
- [ ] Intra-line change highlighting (word-level diff)
- [ ] Binary file detection with fallback message
- [ ] Image diff placeholder (show dimensions, indicate binary)
- [ ] Expand/collapse unchanged context sections
- [ ] Copy selected diff lines

### 2.4 Search and Filter
- [ ] Filter changed files by name (fuzzy search)
- [ ] Search within diff content (Ctrl+F)
- [ ] Highlight matches in file list and diff view

### 2.5 Remote Operations Polish
- [ ] Progress indicators for fetch/pull/push
- [ ] Error messages with actionable suggestions
- [ ] Auto-fetch on repo open (configurable)
- [ ] Upstream setup prompt on first push

### 2.6 Additional Syntax Highlighting
- [ ] Add tree-sitter grammars: JavaScript/TypeScript, Python, Go, C/C++, JSON, YAML, Markdown
- [ ] Graceful fallback for unsupported languages (plain text highlighting)
- [ ] Language detection from file extension

### 2.7 Release Target: v0.1
- Tag a v0.1 release once daily workflow is complete
- Create binary builds for Linux (and macOS if feasible)

---

## Phase 3: History and Branching (Weeks 8–12)

**Goal:** Let users navigate repository history and manage branches without leaving wgit.

### 3.1 Commit Log View
- [ ] Scrollable commit list with author, date, summary
- [ ] Commit detail panel showing full message + changed files
- [ ] Click a commit to view its diff
- [ ] Pagination / virtualized scrolling for large histories
- [ ] Filter log by path, author, date range

### 3.2 Branch Management
- [ ] Branch list panel (local and remote branches)
- [ ] Create new branch from current HEAD or selected commit
- [ ] Rename branch
- [ ] Delete branch with confirmation (prevent deleting current branch)
- [ ] Checkout branch with dirty-working-tree warning
- [ ] Checkout detached HEAD with prominent warning

### 3.3 Graph Visualization
- [ ] Simple ASCII-style commit graph alongside log
- [ ] Branch/merge visualization
- [ ] Color-coded branch lines
- [ ] Click graph nodes to select commits

### 3.4 File History and Blame
- [ ] File history view (commits that touched a specific file)
- [ ] Blame view with line-level annotations
- [ ] Click blame annotation to jump to that commit

### 3.5 Release Target: v0.2
- Tag v0.2 with history and branching features

---

## Phase 4: Collaboration and Remote Workflows (Weeks 13–16)

**Goal:** Support team Git workflows — syncing, reviewing incoming changes, tagging.

### 4.1 Remote Management
- [ ] View configured remotes
- [ ] Add/remove remotes
- [ ] Rename remotes
- [ ] Per-remote fetch

### 4.2 Sync Workflow
- [ ] Pull with rebase option (configurable default)
- [ ] Push with force-push confirmation (dangerous tier)
- [ ] Compare local vs upstream diff
- [ ] Incoming/outgoing commit indicators
- [ ] Conflict-aware merge messaging

### 4.3 Review Mode
- [ ] View incoming changes before pulling
- [ ] Commit range diffing (compare two commits/branches)
- [ ] Review mode with "accept" / "reject" per-file visual indicators

### 4.4 Tags
- [ ] Tag list view
- [ ] Create lightweight and annotated tags
- [ ] Delete tags with confirmation
- [ ] Push tags to remote

### 4.5 Release Target: v0.3

---

## Phase 5: Power Git (Weeks 17–22)

**Goal:** Support advanced workflows for power users.

### 5.1 Hunk and Line Staging
- [ ] Visual hunk boundaries in diff view
- [ ] Stage/unstage individual hunks
- [ ] Stage/unstage individual lines
- [ ] Hunk-level discard with confirmation

### 5.2 Stash Operations
- [ ] Stash list view
- [ ] Create stash (with optional message)
- [ ] Apply / pop / drop stash
- [ ] Stash diff preview

### 5.3 Cherry-Pick and Revert
- [ ] Cherry-pick selected commit(s) from log
- [ ] Revert commit with preview
- [ ] Handle conflicts during cherry-pick/revert

### 5.4 Rebase
- [ ] Rebase current branch onto target
- [ ] Interactive rebase helper (reorder, squash, edit, drop)
- [ ] Rebase continue/abort/skip controls
- [ ] Conflict resolution during rebase

### 5.5 Merge and Conflict Resolution
- [ ] Merge branch UI
- [ ] Three-way diff for conflicts
- [ ] Accept ours / theirs / manual edit per hunk
- [ ] Merge continue/abort controls

### 5.6 Reset and Recovery
- [ ] Reset modes (soft, mixed, hard) with clear explanations
- [ ] Reflog browser for recovery
- [ ] "Undo last operation" where Git allows it

### 5.7 Release Target: v0.4

---

## Phase 6: Product Polish and v1.0 (Weeks 23–30)

**Goal:** Ship a production-grade Git client.

### 6.1 Command Palette
- [ ] Fuzzy command search (Ctrl+Shift+P)
- [ ] All actions accessible via palette
- [ ] Recent commands
- [ ] Keyboard shortcut hints in palette

### 6.2 Settings and Configuration
- [ ] Settings panel UI
- [ ] Configurable keybindings
- [ ] Editor integration (open file in $EDITOR)
- [ ] Git config display

### 6.3 Theme System
- [ ] Light and dark themes
- [ ] User-customizable themes (TOML config)
- [ ] High-contrast accessibility theme

### 6.4 Performance
- [ ] Virtualized lists for status/history (render only visible rows)
- [ ] Incremental diff loading for large files
- [ ] Background file watching for auto-refresh
- [ ] Benchmark suite for startup time, scroll FPS, large-repo operations
- [ ] Target: <200ms cold start, 60fps scroll on 10k+ file repos

### 6.5 Cross-Platform Packaging
- [ ] Linux: AppImage, .deb, Flatpak
- [ ] macOS: .dmg with code signing
- [ ] Windows: .msi installer
- [ ] Auto-update mechanism

### 6.6 Accessibility
- [ ] Screen reader support (platform accessibility APIs)
- [ ] Keyboard-only navigation for all features
- [ ] Configurable font size and density

### 6.7 Observability
- [ ] Command log panel (recent Git operations with timing)
- [ ] Error diagnostics export
- [ ] Crash reporting opt-in
- [ ] Structured logging to file

### 6.8 Release Target: v1.0

---

## Cross-Cutting Concerns (Ongoing)

### Safety Guardrails
- Every `Dangerous`-tier command gets a confirmation dialog
- Confirmation dialogs explain consequences in plain language
- Preview diffs before destructive actions
- Undo/recovery hints after risky operations

### Documentation
- User-facing keyboard shortcut reference (built into app)
- README with screenshots, build instructions, and feature list
- CONTRIBUTING.md for external contributors
- CHANGELOG.md maintained per release

### Design System
- Evolve the layout toward:
  - Left sidebar: repos, branches, stashes, tags
  - Center: file list / commit log / graph
  - Right: diff / commit details / action panels
  - Modal: commit, branch create, stash, confirmations
- Consistent spacing, sizing, and interaction patterns

---

## Success Metrics

| Metric | v0.1 Target | v1.0 Target |
|--------|------------|------------|
| Review + commit + push cycle | < 60s | < 30s |
| Daily Git actions without terminal | 60% | 95% |
| Cold start time | < 2s | < 200ms |
| Scroll FPS (large diff) | 30fps | 60fps |
| Test coverage (core modules) | 50% | 80% |
| Git operation failure rate | < 5% | < 1% |

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Custom UI work slows feature delivery | Set UI budget per phase; reuse components aggressively |
| Synchronous Git blocks UI | Async command layer in Phase 1 before feature expansion |
| app.rs grows unmanageable | Refactor in Phase 1; enforce module boundaries |
| Unsafe Git actions erode trust | Safety tiers from day one; test destructive paths thoroughly |
| Scope creep on Git edge cases | Follow milestone sequence; defer power features to Phase 5 |
| Cross-platform rendering issues | Test on Linux first; macOS/Windows in Phase 6 |

---

## Immediate Next Steps (Priority Order)

1. **Set up CI** — GitHub Actions with check/test/clippy/fmt
2. **Refactor app.rs** — Split into state, input, layout, toolbar, file_list, diff_view, modal modules
3. **Build typed command layer** — Route all Git ops through `Command` enum with safety tiers
4. **Add async execution** — Background channel for Git operations + loading states
5. **Expand test coverage** — Fixture-based integration tests for core Git operations
6. **Multi-select staging** — Keyboard and mouse multi-select with batch operations
7. **Enhance commit panel** — Inline panel, amend, validation, character limits
8. **Diff improvements** — Side-by-side mode, intra-line highlighting
9. **Branch management UI** — List, create, checkout, delete
10. **Commit log view** — Scrollable history with commit details
