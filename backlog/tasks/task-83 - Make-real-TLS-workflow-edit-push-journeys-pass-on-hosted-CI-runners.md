---
id: TASK-83
title: Make real-TLS workflow-edit push journeys pass on hosted CI runners
status: Done
assignee: []
created_date: '2026-10-18 06:15'
updated_date: '2026-10-18 07:14'
labels:
  - ci
  - testing
dependencies: []
priority: low
ordinal: 12069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The 18 tui_e2e tests under task54_workflow_edit_v9::remote_v9 (real openssl s_server + CONNECT proxy + git-http-backend push) pass locally under git 2.53 and 2.55 but on ubuntu-24.04 GitHub runners the app shows 'Original repository scope is fenced' after the pre-push identity check (expected 172 fixture commands, saw 169). CI skips them via GHTUI_CI_SKIP; they remain in the local gate. Diagnose the runner-only fence (OpenSSL 3.0/curl, proxy env, or scope detection) and remove the skip.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Root cause of the runner-only fence identified and fixed
- [x] #2 GHTUI_CI_SKIP removed from .github/workflows/ci.yml and CI is green
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Cause: the real-TLS fixture's 30s self-deadline expired during push+PR journeys on ~2.6x slower hosted runners; an expired fixture surfaces as a fenced scope. Reproduced locally by shrinking it to 4s. Raised to 180s and removed the CI skip. Gate: cargo test --test tui_e2e -- task54_workflow_edit_v9::remote_v9:: (42 passed).
<!-- SECTION:NOTES:END -->
