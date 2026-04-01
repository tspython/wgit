# wgit Product Roadmap (Baseline: current `work` branch)

> Note: there is no local `master` branch in this checkout, so this roadmap is grounded in the current codebase baseline available in this repository.

## 1) Repository Audit Summary (Detailed Search)

This roadmap is based on a code-and-test audit of:

- `src/git_model.rs` for Git capability surface and command safety boundaries
- `src/app.rs` and `src/models.rs` for UX surface and interaction modes
- `src/main.rs` + `src/repo_store.rs` for startup and repository lifecycle
- `tests/git_model_backend.rs` + in-file `git_model` tests for backend behavior coverage
- `Cargo.toml` for dependency and platform constraints

### Implemented capabilities (today)

#### Core desktop UX shell
- Native `winit` + `wgpu` app shell and custom renderer
- Split view: files/status pane and diff pane
- Keyboard/mouse navigation, pane focus, divider/zoom state
- Toolbar + status feedback patterns

#### Repository lifecycle
- Open current repo (`GitModel::open()`)
- Open explicit repo path (`--repo` or positional path)
- Recover from non-repo cwd by trying recent repos
- Persist up to 12 recent repositories in `~/.wgit/recent_repos.txt`

#### Working tree and staging flows
- Porcelain status parsing with grouped sections:
  - `STAGED`
  - `UNSTAGED`
  - `UNTRACKED`
- Single-file stage/unstage
- Stage all / unstage all
- Discard selected file changes (tracked/untracked handling)

#### Diff and readability
- Selected-file diff rendering
- Cached/unstaged diff behavior via backend
- Syntax highlighting (Rust + TOML) using Tree-sitter
- Diff line style classification (add/remove/hunk/meta/file headers)
- Hunk header parsing for line numbers

#### Commit + remote basics
- Commit from UI message fields
- Fetch / pull / push operations
- Branch tracking status (ahead/behind/upstream)
- Branch list + checkout branch

#### Existing test coverage
- Status classification and section summarization coverage in:
  - unit tests inside `git_model.rs`
  - integration-style tests in `tests/git_model_backend.rs`
- Recent repo parsing/writing tests in `repo_store.rs`

## 2) Product Positioning

`wgit` should become the **fastest native desktop Git workstation for keyboard-centric developers** by doubling down on:

1. **Performance and clarity** (large repos, large diffs)
2. **Safe operations** (guardrails for destructive Git actions)
3. **Flow completeness** (terminal-free daily work)
4. **Progressive power** (advanced flows without UI overload)

## 3) Gap Analysis (Current vs Desired)

### A. Daily workflow gaps (highest user value)
- No hunk-level stage/unstage/discard
- No explicit commit validation UX (e.g., empty summary behavior guidance)
- No robust operation progress/cancellation model
- Limited filtering/search in changed files
- Pull/push UX is basic (limited remote/branch intent controls)

### B. History/inspection gaps
- No commit graph/log view
- No commit details drill-down
- No file history and no blame view
- No comparison mode for branch-to-branch ranges

### C. Collaboration gaps
- Remote management is not modeled in UI
- No upstream configuration workflow UX
- No conflict-resolution guided flow in-app

### D. Recovery/power gaps
- No stash UX
- No rebase/cherry-pick UX
- No reflog/recovery visibility
- No safety tiers communicated directly in UI

### E. Platform quality gaps
- Minimal QA matrix and release process artifacts
- No benchmark harness for large repo/diff rendering
- Limited telemetry/diagnostics for operation failures

## 4) Roadmap Principles

- **Principle 1: finish daily loop first** (`review -> stage -> commit -> sync`)
- **Principle 2: safety before power** (guardrails precede destructive features)
- **Principle 3: architecture before breadth** (stabilize command/state model)
- **Principle 4: keyboard-first parity** (every primary flow keyboard reachable)
- **Principle 5: measurable milestones** (acceptance criteria + success metrics)

## 5) 4-Phase Product Roadmap

## Phase 0 (2–3 weeks): Foundation Hardening

### Objective
Turn prototype-quality behavior into a reliable product core.

### Scope
- Define explicit command lifecycle states:
  - idle / running / success / error
- Standardize backend action wrappers for all Git operations with consistent error payloads
- Add operation-result surfaces in UI status area
- Add fixture tests for:
  - status parsing edge cases
  - diff formatting edge cases (binary/renames/empty hunks)
  - branch tracking parsing
- Introduce command safety tiers in UX copy for destructive actions

### Acceptance criteria
- Every Git action reports consistent status semantics
- Error messages include command intent + actionable next step
- No silent failures in stage/unstage/commit/sync paths

### Success metrics
- 0 known crashers in normal repo interactions
- >= 80% backend parser path coverage for status/diff/branch parsing modules

## Phase 1 (4–6 weeks): Daily Workflow MVP

### Objective
Make `wgit` viable for day-to-day solo development.

### Scope
- File list productivity:
  - text filter/search
  - section collapse/expand
  - better selection persistence on refresh
- Staging improvements:
  - multi-select stage/unstage
  - hunk-level stage/unstage (first iteration)
- Commit workflow:
  - enforce non-empty summary
  - optional amend toggle
  - clearer post-commit refresh and state reset
- Sync workflow:
  - explicit fetch/pull/push target controls (remote + branch)
  - ahead/behind indicator prominence
- Repo workflow:
  - recent repos quick switch UX improvements

### Acceptance criteria
- User can complete `edit -> review -> hunk/file stage -> commit -> push` without terminal
- Keyboard-only path exists for core actions

### Success metrics
- >= 90% successful completion in scripted QA tasks for daily loop
- Median action latency for stage/unstage/commit refresh < 200ms on medium repo

## Phase 2 (5–7 weeks): History, Branching, and Insight

### Objective
Enable users to understand change context and branch topology.

### Scope
- Commit log view with paging/virtualization
- Commit detail panel (message, files changed, diff)
- Branch panel improvements:
  - local/remote grouping
  - create/delete/rename branch with guardrails
- Branch compare mode (HEAD vs selected branch)
- File history view (per-path commit list)

### Acceptance criteria
- User can answer “what changed, where, and on which branch” fully in-app
- Branch switch operations are safe and legible

### Success metrics
- 100% of branch-management happy paths covered by integration tests
- <= 1% branch-operation failure rate in internal dogfooding

## Phase 3 (6–8 weeks): Collaboration + Power Safety

### Objective
Add team-oriented and advanced Git workflows safely.

### Scope
- Conflict-aware pull/merge UX with clear next actions
- Upstream setup and remote management workflows
- Stash flows:
  - create/apply/pop/drop with previews
- Rebase/cherry-pick assistant (guided, explicit state machine)
- Recovery tools:
  - reflog viewer
  - “undo-oriented” action prompts where feasible

### Acceptance criteria
- Advanced workflows remain understandable under failure states
- Destructive operations require explicit confirmations with previews

### Success metrics
- Reduced terminal fallback for conflict and stash flows in dogfooding
- High user confidence score on destructive action clarity

## 6) Cross-Cutting Engineering Tracks

### Performance track (runs across all phases)
- Diff rendering virtualization for large patches
- Incremental text shaping/cache invalidation strategy
- Benchmarks:
  - large monorepo status list
  - large binary-heavy repo fallback behavior

### Quality track
- Expand backend test fixtures for real-world porcelain outputs
- Add regression fixtures for branch-tracking edge cases
- Add deterministic golden outputs for grouped document rendering

### UX consistency track
- Unified command palette + shortcut discovery
- Consistent modal language for confirmations/errors
- Color/accessibility audit for line styles and section indicators

## 7) Release Plan

- **v0.2.0** = Phase 0 + initial Phase 1 (stable daily basics)
- **v0.3.0** = complete Phase 1 (daily workflow MVP)
- **v0.4.0** = Phase 2 (history + branch fluency)
- **v0.5.0** = Phase 3 baseline (collab/power workflows)

Each release should include:
- migration notes for UX shortcuts
- known limitations section
- benchmark delta report vs prior release

## 8) Risks and Mitigations

### Risk: Git edge cases create brittle UX
Mitigation:
- use fixture-driven parser tests
- define structured error taxonomy
- ship guarded workflows incrementally

### Risk: Performance regressions from feature breadth
Mitigation:
- introduce benchmark gates before each release
- prioritize virtualization before heavy history views

### Risk: Unsafe destructive operations
Mitigation:
- command safety tiers
- preview + confirm flows
- explicit recovery guidance in errors

## 9) Immediate Next Sprint Backlog (Recommended)

1. Standardize command result model (`ok/error/loading`) across all toolbar actions
2. Add file filter/search in status pane
3. Add commit validation + amend toggle
4. Add multi-select file stage/unstage
5. Add integration fixtures for branch tracking and diff edge cases

These five items provide the highest ROI toward terminal-free daily usage while improving trust and product stability.
