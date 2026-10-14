---
id: doc-5
title: GitHub Actions coverage contract
type: specification
created_date: '2026-10-12 23:09'
updated_date: '2026-10-12 23:09'
---
# GitHub Actions coverage contract

Full Actions support is a user requirement, including indicators and native viewers. This contract covers workflow and run operation plus related Actions settings exposed by public capabilities. An implementation inventory verifies the current gh version, GitHub.com/Enterprise behavior, permissions, and any unavailable operations before claiming parity.

| Area | Native TUI behavior | Access |
| --- | --- | --- |
| Indicators and discovery | Status beside PR/commit/repo, org/repo workflow lists, filters, latest runs | gh workflow/run JSON, gh api |
| Run detail | SHA/ref/event/actor/attempt, durations, dependencies, matrix jobs, steps and progress | gh run view and API; definition data where needed |
| Logs and annotations | Job/step logs, failed-step jump, exact/regex search, streaming/polling freshness, check annotations | gh run view/API |
| Dispatch and definitions | Read workflow YAML at a revision, typed workflow_dispatch inputs, ref selection, enable/disable, explicit local workflow edits | gh workflow commands and Git editor flow |
| Run control | Watch, rerun all/failed/specific jobs, debug logging, cancel/force-cancel where supported, delete runs/logs | gh run commands and API |
| Artifacts | Inventory, bounded content preview, download/export, retention/expiry state, explicit deletion | gh run download and artifacts API |
| Approvals and deployment gates | Fork-run approvals, pending environment approval/rejection, protection-rule explanations | workflow-runs API and related host capability |
| Cache | List/filter sizes/refs/age, quota context, explicit individual/selected cache deletion | gh cache commands and cache API |
| Secrets and variables | Repo/environment/org eligible scopes, names/metadata, variables CRUD, secret set/delete without pretending to reveal values | gh secret/variable commands |
| Environments and permissions | View/edit supported Actions policies, workflow token permissions, environments/protection rules with effect previews | gh api settings endpoints |
| Runners | See available/busy/offline runners and groups, labels and scoped configuration; manage supported labels/associations/removal | runners API; permission-gated |
| Cross-repo activity | Saved filters, actor avatars/logins, failure queues, elapsed states and dashboard drilldowns | Shared cache/query/identity layers |

Sources: [workflow dispatch](https://cli.github.com/manual/gh_workflow_run), [run rerun](https://cli.github.com/manual/gh_run_rerun), [artifact downloads](https://cli.github.com/manual/gh_run_download), and GitHub REST references for [workflow runs](https://docs.github.com/en/rest/actions/workflow-runs), [permissions](https://docs.github.com/en/rest/actions/permissions), [caches](https://docs.github.com/en/rest/actions/cache), [runners](https://docs.github.com/en/rest/actions/self-hosted-runners), [secrets](https://docs.github.com/en/rest/actions/secrets), and [variables](https://docs.github.com/en/rest/actions/variables).

The operational UI does not execute arbitrary workflows locally, install runner services, provision compute, change billing, or invent nonpublic GitHub capabilities. Supported runner and settings operations receive explicit scope/effect previews. Workflow YAML edits use the local Git edit/commit/PR flow. Website features unavailable publicly are recorded as gaps, with a native equivalent tracked where feasible; a browser link does not count as support.

Run/job IDs are not interchangeable; rerun attempts have their own identity. Correlate a status indicator with the observed commit SHA and expected run event, not just a similarly named branch. Workflow definitions can differ between refs and reusable workflows. Explain when dependencies cannot be fully inferred from static YAML. Polling through gh is bounded and labelled; do not claim an unavailable live log stream.

Artifacts are untrusted archives: validate paths and sizes before extraction, keep previews bounded, and never execute downloaded content. Secret values are entered transiently and passed through gh's input path; never cache, echo, include in logs, or claim existing secret values are retrievable.

Every workflow and settings action is clickable and keyboard accessible. Tests exercise exact command targets, UI outcomes, failed permissions, stale run states, paging, and mutations with offline fake-gh fixtures. Opt-in disposable-repository tests verify actual contracts. The release audit records native coverage, platform/host limits, and unresolved gaps.
