# Source attribution

System originated in [anreonyr/sqware](https://github.com/anreonyr/sqware).
The API and its initial fixtures were imported from commit
`e6302af2156a6fe3766d3ccc22f080a481948bf3`; server/client/support continued from
`a29707fb83191647c63cc9ccfef1e8721bbe8dad`. Original history remains at that
repository; import mappings and subsequent adaptations remain in Git history.

Author identities recorded in the original history include:

- `anreonyr <anreonyr@gmail.com>`
- `anreonyr <88045926+anreonyr@users.noreply.github.com>`

Those source checkpoints declared no license and tracked no project LICENSE,
COPYING or NOTICE. This repository adds no license grant; publication alone
does not grant a license. Preserve third-party notices with their code.

The protocol v2 guest runner, independent-Team RX mapping fixture and minimal linker/build setup in
`tests/guest/client` were adapted from the Twine Share guest at
`17554b0e5fba149796c5696c3eca9f20cc125fc7`, adding a separate writable
segment and System-specific lifecycle assertions. Both projects are part of
Shuyra. This attribution does not add a license grant.

The native System heap's Kernel page backing in `server/src/runtime/heap/pages.rs`
adapts `rust/library/std/src/sys/pal/vanllia/broker/pages.rs` from Shuyra Toolchain
at `cd6a6f0d3bcd180cbf835deda08aa07e9b9d4e97`. System owns its initial-supervisor
servicing protocol; Toolchain remains the language runtime for ordinary std
Units. This attribution adds no license grant.
