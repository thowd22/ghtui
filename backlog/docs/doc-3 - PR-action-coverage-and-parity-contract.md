---
id: doc-3
title: PR action coverage and parity contract
type: specification
created_date: '2026-10-12 22:59'
updated_date: '2026-10-12 23:04'
---
# PR action coverage and parity contract

This checklist bounds “everything you can do in the GitHub UI” to the PR page and its connected review/check workflows. Scope is PR collaboration, not all GitHub administration, security, billing, or project-management products. Additional PR-page controls discovered during implementation must be added and tracked rather than silently omitted.

This is a proposed mapping, not a statement that endpoints have already been exercised. A capability inventory task must verify each row against the current gh version, GitHub.com, supported Enterprise versions, repository rules, and caller permissions.

| Workflow | Preferred access through gh | Required outcome |
| --- | --- | --- |
| Personal authored/assigned/review-requested queues | search prs/status; paginated api reads as needed | Cross-repo scope, distinct membership, explicit incomplete results |
| PR details, event timeline, commits, files, reviews | pr view JSON plus REST/GraphQL reads | All pages, stable identity, exact base/head, permission state |
| Checks, required status, jobs, logs, rerun/cancel | pr checks; run view/rerun/cancel; api for gaps | Correlate the PR SHA, display pending/blocked/failing states |
| Create PR, forks, head/base selection, draft | pr create; repo/git worktree workflow | Preview exact diff, safely publish, return canonical new PR |
| Edit title/body/base, labels, assignees, reviewers, milestone, linked issues/project placement | pr edit; issue/project commands or api | Read-back confirms each selected change; permission failures explained |
| Draft/ready, close/reopen, lock/unlock, update branch, branch deletion | pr ready/close/reopen/lock/unlock/update-branch; api for gaps | State transitions, race protection, explicit effect and result |
| Conversation comments: create/edit/delete, reactions, subscription | pr comment plus REST/GraphQL | Correct author/permissions, paginated discussion, immediate refresh |
| Inline/multiline comments and replies, edit/delete | api review-comment/review-thread operations | Canonical file/side/line/range and expected head, outdated anchors handled |
| Pending review: add/edit/delete drafts, submit/discard | api pending-review operations; pr review for simple submission | Restart-safe local drafts, explicit publish, approve/request-changes/comment |
| Review dismissal and re-request | api where available | Maintainer/reviewer permission and stale-head behavior verified |
| Resolve/unresolve threads; viewed/unviewed files | api GraphQL | Server state refreshed and invalidated when files change |
| Code suggestions and suggested-change application | api where supported; explicit isolated-worktree commit/push alternative | Exact selected edits, author credit where supported, changed-head/conflict handling |
| Edit PR branch files and resolve conflicts | Verified remote capability or explicit worktree workflow | Preview resulting commit and push; no silent edits to an unrelated checkout |
| Merge, squash, rebase, auto-merge, queue, cancellation | pr merge; api for capability gaps | Allowed methods, checks/rules, expected head, distinct queue/merge outcomes |
| Revert merged PR and restoration followups | pr revert where supported; api/git workflow otherwise | Preview scope and create a traceable reverting PR |
| Copy/open/export/navigation | gh browse/pr view --web plus local exports | Native workflow remains usable; browser handoff is visibly labelled |

Sources for extending the adapter: [gh command manual](https://cli.github.com/manual/), [gh api](https://cli.github.com/manual/gh_api), [review-comment REST API](https://docs.github.com/en/rest/pulls/comments), and [GraphQL PR reference](https://docs.github.com/en/graphql/reference/pulls). Generic API calls still inherit gh authentication.

For every action, the inventory records: native command/operation, minimum supported version/host, read/write classification, required permissions, preview/confirmation behavior, concurrency guard, pagination, fixture coverage, live disposable-repo evidence, and implementation task.

A row is complete only when it works inside the TUI with verified server state. A browser handoff or generic “run arbitrary gh” control does not close the row. If a GitHub website function has no suitable public operation, document the limitation and track an equivalent native workflow when feasible; do not claim exact parity. Platform limitations must be explicit in release notes and the action menu.

Review and merge actions target a stable PR identity and observed head. Refresh stale permission/head state before publishing. After an ambiguous network failure, reconcile rather than submitting the same write again. Multi-item workflows report per-item outcomes.

Release evidence must cover an author, assigned contributor, requested reviewer, maintainer, and read-only contributor; fork PRs; protected branches; queues when available; draft and merged PRs; force-pushed heads; multiline/deleted-side comments; outdated threads; and hosts lacking a feature. Automated live mutation checks are opt-in and confined to a disposable test repository.
