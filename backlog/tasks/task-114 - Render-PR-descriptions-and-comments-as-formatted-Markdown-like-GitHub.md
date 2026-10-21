---
id: TASK-114
title: Render PR descriptions and comments as formatted Markdown like GitHub
status: To Do
assignee: []
created_date: '2026-10-21 17:54'
labels:
  - usability
dependencies: []
priority: high
ordinal: 47069
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User report (2026-10-21, worklappy): PR text shows raw HTML and Markdown (<!-- devin-review-badge -->, <a href>, <picture>). Build one shared GitHub-flavoured Markdown renderer for PR descriptions, conversation comments, review comments and previews: headings, emphasis, lists, task lists, quotes, fenced code with a distinct block style, tables, links shown as text with the URL available, images as [image: alt]. HTML comments are hidden; common inline HTML (a, picture/img, details/summary, br) is reduced to readable text. Use theme roles.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 No raw HTML comments or tags appear in the Inbox preview, PR Summary or Conversation for the worklappy-style badge body
- [ ] #2 Headings, lists, code blocks, quotes, tables and links render distinctly and wrap at 80 and 140 columns
- [ ] #3 Lib tests over a corpus of real-shaped PR bodies; keyboard+mouse PTY journey shows a rendered description and comment
<!-- AC:END -->
