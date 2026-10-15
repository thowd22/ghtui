# Offline dependency sources

The application and its actual-binary PTY tests build with `cargo test --offline
--locked --all-targets`. `.cargo/config.toml` selects the checked-in registry
packages; the Cargo patch table selects relative upstream source directories.
No Cargo cache, `/tmp` checkout, network, or extra Git checkout is required.
Rust, the system C toolchain, installed `git`, and external `gh` remain required.
Only macOS and Linux are product targets. Some other-target packages are retained
because Cargo resolves their metadata even on Unix; their inclusion does not add
platform support.

`upstream-sources.json` records the official origin and exact Git revision of
every upstream directory. Each upstream license is retained. Registry sources
were copied by `cargo vendor --versioned-dirs`; Cargo.lock package checksums and
each `.cargo-checksum.json` verify them. `upstream-checksums.json` freezes the
retained upstream file tree. Run `node scripts/audit-dependencies.mjs` to check
both source sets, relative path containment, pins, licenses, and checksums.

The registry transport was unavailable in the development environment; ordinary
approved Git clone/fetch commands supplied official sources. No sandbox-policy
bypass or unrelated modified dependency source was used. A handful of upstream
Cargo manifests were reduced to the selected Unix/default feature graph; the
exact changes are in `manifest-adaptations.patch` and
`search-manifest-adaptations.patch`. Runtime Rust sources and
package versions were not changed. Windows-only dependencies and unused Ratatui
backends/calendar/macros were removed from those manifests. Optional document
features remain available. The product's selected Crossterm events, bracketed
paste, Ratatui backend, and Unix PTY behavior are unchanged.

Embedded dependency `.git` directories and the large Zip/VT100 upstream test
fixture directories are excluded; the application's independent contract and
PTY tests remain in `tests/`. Exploratory unused source versions are not bundled.
During development these exclusions were moved recoverably to
`.verification/dependency-metadata` (ignored development backup); that backup is
not a build dependency. The backup originally lived under `/tmp` and was moved
inside the ignored workspace directory to relieve the temporary-files quota.

The old htmlescape repository supplies only the license declaration in its
original Cargo.toml (`Apache-2.0 / MIT / MPL-2.0`), not separate license texts.
That exact declaration and the unmodified source are retained and explicitly
recorded; the audit does not pretend an upstream license file was supplied.

SHA-1 is used only to verify Git object identity for GitHub content, not for new
security signatures or password hashing. Its unmodified `sha1` package subtree
from the pinned RustCrypto/hashes repository is retained with both original
licenses; `upstream_subdirectory` records that provenance. Unused sibling hash
packages and clone metadata were moved into the ignored development backup.

To update a dependency, obtain its official source, preserve its license, record
its new exact pin and any manifest-only adaptation, and rerun `cargo vendor
--versioned-dirs vendor/crates`. Deliberate source updates must then regenerate
the upstream checksum inventory with `node scripts/audit-dependencies.mjs
--refresh-upstream-checksums`. Ordinary builds/tests never refresh integrity
metadata or accept changed source checksums automatically.

## SQLite integrity update

The bundled runtime is SQLite 3.51.3 from official rusqlite v0.39.0,
commit `2a1790a69107cd03dae85d501dcbdb11c5b32ef3`, with libsqlite3-sys 0.37.0.
Its original amalgamation SHA3-256 is
`32d5424f97e0a7fc5ed2f6335afbb58be4e0298bd7117a34e39d345ff13d859e`,
matching the [official release](https://www.sqlite.org/releaselog/3_51_3.html).
The original C/Rust runtime and MIT/public-domain notices are unmodified.
Only unsupported wasm-unknown manifest dependencies and their bindgen feature
edge were removed for this macOS/Linux selected graph; the exact diff is
`sqlite-manifest-adaptations.patch`. Existing default statement caching remains
enabled, and `fallible_uint` explicitly preserves checked unsigned SQL values.

SQLite documents the [WAL-reset race](https://www.sqlite.org/wal.html#walreset)
as fixed in 3.51.3 and later. This upstream update is separate from the private
cache's lock-neutral/sidecar lifecycle safeguards. Linked-version/source and
real concurrent checkpoint/write/reopen tests verify the application runtime;
stress testing is not a claim to reproduce the rare upstream corruption race.
Native macOS and release verification remain separately required.

For this update an independent offline `cargo vendor --versioned-dirs` output
was compared with, but did not replace, the existing registry sources. All 117
package checksums and common file hashes match. Cargo's generated inventory
omits 218 retained upstream metadata/configuration files; the original registry
files and checksum inventories remain byte-for-byte unchanged.
