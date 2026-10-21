---
id: TASK-104
title: >-
  Show cleanup-unconfirmed separately from draft retained or deleted after a
  late publication ACK
status: Done
assignee: []
created_date: '2026-10-21 06:33'
updated_date: '2026-10-21 08:55'
labels:
  - bug
dependencies: []
priority: low
ordinal: 37069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After a Remove is accepted into the ordered queue, DraftPublicationError::AcknowledgementUnknown does not prove body retention. The composer outcome should say 'cleanup unconfirmed' separately from 'draft retained' or 'draft deleted'. Follow-up from TASK-16.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A late or unknown publication ACK shows 'cleanup unconfirmed' distinct from retained/deleted, by key and click
- [x] #2 Lib and keyboard+mouse PTY coverage verify the stored draft state independently
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Composer reports cleanup unconfirmed vs draft retained/deleted after a late cleanup ACK (c0992adf). drafts/scoped_pr lib 84/0; composer PTY 78/0. Gate 17 (node scripts/test-all.mjs at 1c39e689): 6353 passed / 0 failed; fmt + clippy clean.
<!-- SECTION:NOTES:END -->
