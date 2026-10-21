---
id: TASK-104
title: >-
  Show cleanup-unconfirmed separately from draft retained or deleted after a
  late publication ACK
status: To Do
assignee: []
created_date: '2026-10-21 06:33'
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
- [ ] #1 A late or unknown publication ACK shows 'cleanup unconfirmed' distinct from retained/deleted, by key and click
- [ ] #2 Lib and keyboard+mouse PTY coverage verify the stored draft state independently
<!-- AC:END -->
