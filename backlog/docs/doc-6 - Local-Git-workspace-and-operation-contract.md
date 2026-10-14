---
id: doc-6
title: Local Git workspace and operation contract
type: specification
created_date: '2026-10-12 23:09'
updated_date: '2026-10-12 23:09'
---
# Local Git workspace and operation contract

Launching ghtui inside a local Git repository (including a subdirectory or worktree) detects it and exposes the local Repository workspace immediately. Show its root, current branch or detached HEAD, local/unpushed commits, upstream relation, working/staged/untracked changes, and conflicts. Personal GitHub queues remain globally accessible. If there is no remote or gh authentication, local Git exploration and management still work.

Useful local management includes staging/unstaging files, hunks and selected lines; commit/amend; branch and tag create/rename/switch/delete; fetch/pull/push with explicit remote/ref/upstream; stash save/view/apply/pop/drop; worktree add/list/remove; merge/rebase/cherry-pick/revert with continue/skip/abort; conflict resolution; reflog-based recovery; and guided bisect.

The user chooses among “Uncommitted files,” “Staged for the next commit,” “This branch's saved changes,” and “Compare two saved versions.” Compare the working tree against HEAD, the index against HEAD, and branch changes using the documented merge-base mode. Present effects in plain language while keeping Git identifiers available.

Before local writes, recheck relevant HEAD/index/worktree state and active sequencer. Never discard unstaged work, force push, hard reset, or remove a dirty worktree implicitly. Destructive operations require exact-target/effect preview and an explicit product confirmation. Use safer recovery paths and explain limitations; not every operation is undoable.

Run installed Git with its configured signing/hooks and credential helpers. Interactive hooks/editor/credential prompts must have a deliberate terminal handoff or visible diagnostic; do not hang a hidden subprocess. User authorization for a TUI action governs that action only. Read-only browsing does not automatically fetch or checkout.

Conflicts show affected files, base/ours/theirs, relevant operation and continuation state. Selection/editing previews the resolved file and staged effect, preserves non-conflicted content, and does not infer that all conflicts are resolved from a successful editor exit. Directory/file and binary conflicts need explicit states.

Mouse and keyboard expose the same operation menu, diff selection, conflict tools, text fields, confirmation, and cancellation paths. Color shows added/modified/deleted/renamed/conflicted states and ancestry relations, with labels/symbols as redundant cues.

Tests use generated real Git repositories and local bare remotes, never a user's normal checkout or remote. Verify Git objects, refs, index and working files independently after PTY journeys for both input paths. Include dirty/stale state, hook failures, detached/unborn histories, conflicts, and operations interrupted before completion.
