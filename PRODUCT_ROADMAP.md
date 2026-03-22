# wgit Product Roadmap

## Vision

`wgit` becomes a fast, keyboard-friendly, GPU-rendered desktop Git client for developers who want the clarity of terminal Git with the discoverability of a visual client.

The product should feel:

- Native and responsive on large repositories
- Safe for high-impact Git actions
- Strong for daily workflows before edge-case power features
- Visually distinct from Electron-style Git clients

## Current State

Today the app already has a strong prototype foundation:

- Custom `wgpu` + `winit` desktop shell
- Repository discovery
- Working tree status list
- Selected-file diff view
- Stage / unstage selected file
- Basic syntax coloring for Rust and TOML diff content

That means `wgit` is not starting from zero. The right next step is to evolve it from a "status + diff viewer" into a safe, complete Git workstation.

## Product Pillars

### 1. Everyday Git First

Make the common path excellent:

- Review changes
- Stage selectively
- Commit confidently
- Pull / push safely
- Manage branches without context switching to terminal

### 2. Speed and Clarity

Lean into the custom renderer:

- Very fast list rendering
- Large diff performance
- Dense information layout
- Strong keyboard navigation

### 3. Safe Power

Git clients fail when dangerous actions are too easy or too opaque. `wgit` should emphasize:

- Clear previews
- Undo / recovery where possible
- Confirmation for destructive actions
- Explicit state transitions

### 4. Progressive Depth

The app should work well for both:

- Users doing `status`, `commit`, `pull`, `push`, `branch`
- Users handling rebase, cherry-pick, stash, history inspection, conflict resolution

## Target Personas

### Primary

- Developers who know Git, but want a faster visual workflow
- Rust / native-tooling enthusiasts who care about responsiveness

### Secondary

- Developers learning Git who need guardrails
- Users managing medium-to-large repositories where terminal diffing is noisy

## Product Strategy

Sequence the roadmap in this order:

1. Complete the daily workflow
2. Add history and branch fluency
3. Add collaboration and remote workflows
4. Add recovery and advanced Git operations
5. Polish performance, trust, and platform quality

This avoids over-investing in advanced Git before the app can replace day-to-day terminal usage.

## Milestones

## Milestone 0: Stabilize The Core Prototype

Goal: turn the current prototype into a reliable base for shipping features.

### Outcomes

- Clean separation between Git state, view state, and render state
- Predictable refresh model
- Better error handling and user feedback
- Testable command layer around Git operations

### Work

- Introduce an application state model for repo, selection, panels, modal state, and async tasks
- Wrap Git actions in typed operations instead of scattered shell calls
- Add structured error surfaces in the UI
- Add fixture-based tests for status parsing and diff parsing
- Define command safety tiers:
  - Safe: refresh, open history, inspect diff
  - Guarded: stage, unstage, commit, checkout branch
  - Dangerous: reset, discard, force push, rebase continue/abort

### Exit Criteria

- App no longer feels prototype-fragile
- Core Git actions can be tested without rendering
- UI can display loading, success, and failure states consistently

## Milestone 1: Daily Workflow MVP

Goal: make `wgit` viable for normal solo development.

### Must-have features

- Open any repo, not just current working directory
- Repo switcher / recent repositories
- Working tree sections:
  - Staged
  - Unstaged
  - Untracked
- Multi-select file staging
- Stage all / unstage all
- Discard changes for selected file or hunk with confirmation
- Commit panel:
  - Summary + description
  - Commit validation
  - Amend last commit
- Pull, fetch, push
- Ahead / behind status
- Diff view improvements:
  - Side-by-side and unified modes
  - Intra-line highlighting
  - Image / binary-file fallback messaging
- Search / filter changed files

### UX requirements

- Keyboard shortcuts for all primary actions
- Clear action affordances even without mouse precision
- Empty states and no-repo states
- Progress indicators for Git operations

### Exit Criteria

- A developer can finish a normal change-review, commit, and push cycle entirely in `wgit`

## Milestone 2: History And Branching

Goal: let users understand repository history and move across branches comfortably.

### Features

- Commit log view
- Commit details panel
- File history
- Blame view
- Branch list:
  - local branches
  - remote branches
  - current branch
- Create branch
- Rename branch
- Delete branch with guardrails
- Checkout branch
- Checkout detached commit with strong warning
- Branch comparison view
- Graph visualization for commit ancestry

### UX requirements

- Smooth transitions between status, history, and branch views
- Strong indicators for HEAD state
- Clear remote tracking information

### Exit Criteria

- User can inspect history, create / switch branches, and understand divergence without terminal help

## Milestone 3: Collaboration And Review

Goal: make the client useful in team workflows, not just local repo management.

### Features

- Fetch all remotes
- Remote management UI
- Pull with rebase option
- Push with upstream setup
- Conflict-aware pull / merge messaging
- Compare local branch to upstream
- Review mode for incoming changes
- Commit range diffing
- Tag list and tag creation

### Stretch features

- Optional hosting integration later for GitHub / GitLab review metadata

Do not start hosting-service integration until core Git flows are solid.

### Exit Criteria

- User can safely sync, inspect incoming/outgoing work, and prepare branch changes for collaboration

## Milestone 4: Power Git

Goal: support advanced workflows that make the app a serious long-term client.

### Features

- Hunk staging
- Line staging
- Stash create / apply / pop / drop
- Cherry-pick
- Revert commit
- Interactive rebase helper
- Merge UI
- Conflict resolution workflow
- Reset modes:
  - soft
  - mixed
  - hard
- Reflog browser
- Recoverability helpers after risky operations

### Guardrails

- Preview before destructive actions
- Dedicated confirmation dialogs with affected refs/files
- Plain-language explanation of consequences

### Exit Criteria

- Advanced users can stay in `wgit` for most non-forensic Git operations

## Milestone 5: Product Polish And Release Quality

Goal: make the client feel production-grade.

### Features

- Command palette
- Settings panel
- Theme system
- Better typography and information density controls
- Large repo performance tuning
- Background refresh / file watching
- Accessibility improvements
- Cross-platform packaging
- Crash reporting strategy
- Logging / diagnostics export

### Non-functional priorities

- Cold start time
- Frame pacing during scrolling
- Zero-jank diff rendering
- Robustness on monorepos and repos with many changed files

### Exit Criteria

- App is credible as a daily driver and ready for broader user testing

## Cross-Cutting Tracks

These should run alongside the milestones instead of being left to the end.

### Safety

- Confirmation UX for destructive actions
- Better messaging around HEAD, index, working tree, and remotes
- Recovery affordances wherever Git permits them

### Information Architecture

The likely long-term layout should evolve toward:

- Left sidebar: repos, branches, stashes, tags, filters
- Center pane: status list / history list / graph
- Right pane: diff, commit details, action panels
- Modal surfaces: commit, branch create, stash, confirmations

### Performance

- Virtualized lists for large status / history views
- Incremental diff loading
- Background command execution
- Smarter invalidation than full-document rebuilds

### Observability

- Command log for recent Git operations
- Error diagnostics with actionable messages
- Metrics for operation duration and failure rate during development

### Test Strategy

- Parser tests for status / diff / branch output
- Integration tests against temporary fixture repositories
- Snapshot-like view-model tests for document generation
- Manual test matrix for destructive operations

## Suggested Release Plan

### v0.1

Prototype quality:

- Current status view
- Better stability
- Open repo
- Commit
- Push / pull / fetch

### v0.2

Daily-driver candidate:

- Grouped file states
- Better diff modes
- Multi-select staging
- Branch checkout and create
- Recent repos

### v0.3

History release:

- Commit log
- Commit details
- Branch graph
- File history

### v0.4

Power-user release:

- Hunk staging
- Stash
- Cherry-pick
- Revert
- Early conflict tooling

### v1.0

Proper Git client:

- Strong daily workflow
- History and branch fluency
- Safe remote workflows
- Recovery tooling
- Stable packaging and performance

## What To Build Next

If we want the highest-leverage immediate sequence from the current codebase, the next 10 items should be:

1. Add repo picker and recent repos
2. Split changed files into staged / unstaged / untracked sections
3. Add commit UI
4. Add fetch / pull / push actions with visible progress and errors
5. Add multi-select files
6. Add stage all / unstage all / discard file
7. Refactor Git operations into a typed command layer
8. Add branch list and branch checkout
9. Add commit log view
10. Add hunk-level diff interaction

## Product Risks

### 1. Building too much custom UI too early

The renderer is a strength, but the product can stall if too much time goes into chrome before core Git workflows are complete.

### 2. Unsafe Git actions

If resets, discards, rebases, or force pushes are exposed without trust-building UX, the client will feel risky even if technically correct.

### 3. Synchronous command execution

As features expand, blocking Git commands will make the UI feel brittle and dated.

### 4. Weak state modeling

A richer client needs explicit models for selection, modal flows, async operations, and transient errors.

## Success Metrics

Track these as the product matures:

- Time to review and commit a change
- Percentage of daily Git actions completed without terminal fallback
- Failure rate of Git operations
- App startup time
- Scroll smoothness in large diffs
- User confidence in destructive actions

## Bottom Line

`wgit` already has an interesting foundation: a native-rendered Git UI with real performance potential. The smartest path is not to chase every Git feature immediately, but to become excellent at the daily loop first, then grow into history, collaboration, and power workflows with strong safety rails.
