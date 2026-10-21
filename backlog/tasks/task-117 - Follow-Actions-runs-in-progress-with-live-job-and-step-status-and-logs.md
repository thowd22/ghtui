---
id: TASK-117
title: Follow Actions runs in progress with live job and step status and logs
status: To Do
assignee: []
created_date: '2026-10-21 17:54'
labels:
  - usability
dependencies: []
priority: high
ordinal: 50069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User request: view logs of runs while they are in progress. GitHub's REST API serves a job's log only after the job completes, and gh exposes no live step stream. So: auto-poll the run and its jobs (bounded interval, backoff, stop on completion or leave), show live job/step status with elapsed time, fetch and show each job's log as soon as that job completes (or the run's partial logs where GitHub provides them), and say plainly that a running job's lines appear when it finishes. A Follow toggle keeps the view at the newest output. Investigate whether gh api provides step-level logs for completed steps of a running job and use them if so.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An in-progress run shows live job and step status that updates without manual refresh and stops polling when done or left
- [ ] #2 Logs of completed jobs appear while other jobs still run; the running job explains when its log will be available
- [ ] #3 Keyboard+mouse PTY journey with a strict fake gh run that progresses queued -> in_progress -> completed
<!-- AC:END -->
