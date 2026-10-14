---
id: doc-8
title: Offline terminal dependency provenance and build recovery
type: guide
created_date: '2026-10-12 23:41'
updated_date: '2026-10-14 08:44'
---
# Offline terminal dependency provenance and build recovery

This guide records actual Linux build recovery during foundation implementation. Cargo registry requests could not resolve index.crates.io; approved direct Git clone and ls-remote commands could reach official GitHub source repositories. The terminal dependencies were obtained through that route. This is local verification support, not a change to the product requirement that end users need only gh and git.

## Pinned official source packages

| Package | Version | Official source and revision | Local package path |
| --- | --- | --- | --- |
| crossterm | 0.29.0 | [crossterm-rs/crossterm tag 0.29](https://github.com/crossterm-rs/crossterm/tree/0.29), 36d95b26a26e64b0f8c12edfe11f410a6d56a812 | /tmp/ghtui-vendor/crossterm |
| vt100 | 0.15.2 | [doy/vt100-rust tag v0.15.2](https://github.com/doy/vt100-rust/tree/v0.15.2), 4c9e5da3de10d838d792929f1691024d6bf59c85 | /tmp/ghtui-vendor/vt100 |
| portable-pty / filedescriptor | 0.9.0 / 0.8.3 | [wezterm/wezterm pinned snapshot](https://github.com/wezterm/wezterm/tree/e2a98e1b8af156988ad0ac778658ac27b60a55fb), e2a98e1b8af156988ad0ac778658ac27b60a55fb | /tmp/ghtui-vendor/wezterm-full/pty and /filedescriptor |
| vte and bundled parser helpers | 0.11.0 | [alacritty/vte tag v0.11.0](https://github.com/alacritty/vte/tree/v0.11.0), dc861b1dae166183c66848fcbd47b8e81a43d972 | /tmp/ghtui-vendor/vte |
| unicode-width | 0.1.14 | [unicode-rs/unicode-width tag v0.1.14](https://github.com/unicode-rs/unicode-width/tree/v0.1.14), 9eaafa5f91b737bb5958d46f9647aa11104c6f25 | /tmp/ghtui-vendor/unicode-width-0.1 |
| nix | 0.29.0 | [nix-rust/nix tag v0.29.0](https://github.com/nix-rust/nix/tree/v0.29.0), 1dad4d8d04a2cd187fae87cb91c4f4e95ff0decd | /tmp/ghtui-vendor/nix |
| serial2 | 0.2.38 | [de-vri-es/serial2-rs tag v0.2.38](https://github.com/de-vri-es/serial2-rs/tree/v0.2.38), 448a20ece5e6e37236d247ed957042420672029b | /tmp/ghtui-vendor/serial2 |
| shell-words | 1.1.1 | [tmiasko/shell-words tag v1.1.1](https://github.com/tmiasko/shell-words/tree/v1.1.1), 8d2868b0edadbd5572acd7ed25b6be890c2781b7 | /tmp/ghtui-vendor/shell-words |
| signal-hook / signal-hook-mio / bundled registry | 0.3.18 / 0.2.5 / 1.4.6 | [vorner/signal-hook tag mio-v0.2.5](https://github.com/vorner/signal-hook/tree/mio-v0.2.5), ac4c81cb0cc67d52af392f7d6a46655eab672477 | /tmp/ghtui-vendor/signal-hook-mio and /signal-hook-mio |

License files remain with each source checkout. Portable PTY and filedescriptor inherit their dependency definitions from the pinned WezTerm workspace. Using the signal-hook-mio checkout for both the root signal-hook package and its child preserves the existing upstream path relationships and avoids duplicate signal-handler registries.

## Temporary manifest adaptations

Only temporary Cargo manifests under /tmp were adapted; no third-party Unix Rust implementation was edited and no package version was fabricated.

- crossterm: remove excluded Windows dependency entries and unused derive_more metadata. Use default-features=false and features=[events, bracketed-paste], retaining actual keyboard, mouse, resize and paste event functionality.
- portable-pty and filedescriptor: remove Windows-only dependency sections.
- serial2: remove its Windows-only dependency section.

Cargo attempted to resolve missing Windows metadata even on Linux. Additional official metadata-source checkouts were explored, but the crossterm-winapi source manifest was version 0.9.0 while crossterm requested 0.9.1. Removing excluded metadata was preferable to inventing a version or changing native implementations. These adaptations are for local offline dependency resolution; normal builds use published dependencies when the registry is available.

## Actual verification

The terminal-only manifest and its complete patch table are at /tmp/ghtui-terminal-check.Ivg1L5/Cargo.toml. It instantiates the real Crossterm event type, portable-pty native system, and vt100 parser.

Commands that passed:

```sh
CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 cargo check --offline --manifest-path /tmp/ghtui-terminal-check.Ivg1L5/Cargo.toml
CARGO_TARGET_DIR=/tmp/ghtui-provider-check.gVlcVM/target CARGO_BUILD_JOBS=2 cargo clippy --offline --manifest-path /tmp/ghtui-terminal-check.Ivg1L5/Cargo.toml -- -D warnings
```

The check resolved 44 packages and compiled the actual terminal dependency set. Upstream crates produced compiler warnings but the primary check crate passed the requested warning gate. This proves these dependencies can compile on this Linux box; it is not full ghtui code or PTY verification, nor macOS execution evidence.

The actual ghtui provider sources are also included directly by /tmp/ghtui-provider-check.gVlcVM/Cargo.toml. Its provider_contracts integration suite passed 21 tests with zero failures and zero ignored tests; its all-targets Clippy warning gate passed. Required real-binary PTY journeys and the full application checks remain separate requirements.

## Disk recovery

Concurrent source/build work hit Disk quota exceeded. Automatic review rejected direct rm -f style removal. Recoverable Git sparse checkouts reduced the downloaded WezTerm worktree from 248 MB to 84 MB and an unused WinAPI worktree from 181 MB to 9.2 MB while keeping their source objects and licenses. Cargo clean removed 77.8 MiB of failed terminal build output. The existing provider target directory was then reused for terminal dependency compilation. All removed build output is regenerable, and sparse checkout content can be restored from its preserved Git objects.

The initial blob-filtered WezTerm clone could not fetch lazy blobs under sparse-checkout because that command had no network route. A full shallow official clone succeeded under the existing approved Git clone prefix; the final sparse checkout operated only on already downloaded objects.

## Current portable recovery verification (2026-10-14)

The earlier `/tmp` paths and failures above are historical recovery evidence, not current build dependencies. All locked registry packages and pinned upstream sources now live beneath project `vendor/`, with relative Cargo patches and `.cargo/config.toml`; see vendor/README.md. The immutable source audit passed86 official origins,10,300 file hashes,108 relative patches and117 registry packages without refreshing checksums. No third-party runtime Rust sources or package versions were fabricated.

TASK-70 is complete as a dependency/environment-recovery task. A new private ignored `.verification/portable-copy.current.w8hEHr` compiled all actual Cargo targets offline/locked with an initially empty task-specific Cargo home and a separate target/private test TMP/Git ceiling. Independent metadata realpath verification found all226 package manifests within that copy. Initial compile1m23s and subsequent synchronized compile gates passed; compile-observed-frames.log records the latest4.34s gate. Copied checked dependency_transport2-code/2-real-terminal passed with actual keyboard/mouse and restored TTY, artifact test-tmp/ghtui-test-coverage-SUeCNH. No shared/global Cargo cache, `/tmp` source checkout, dependency download, remote or live mutation was required.

The corresponding current main Linux checkpoint passed1,256 code/provider and356 real-binary PTY tests with0 failed/ignored, formatting and strict all-target Clippy; exact environment/commands/logs and retained negative runs are recorded in Task70. This does not complete native macOS verification, release packaging, CI/CD, performance, or the remaining Git/GitHub workflows.
