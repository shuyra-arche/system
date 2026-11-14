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

The ABI 11 heap Flat/Reservation owner and bounded native-memory completion
logic adapt Toolchain `rust/library/std/src/sys/pal/vanllia/broker/{pages,memory}.rs`
at `efdcb7bff44fb9760149964bd51ac35eee0506f0`. System shares a Team-wide monotonic
request allocator across heap and RPC, retaining its initial-Task backing owner and explicitly returning closed Flat/Reservation Pends.
The CPU-only QEMU runtime platform and linker build follow Vanllia's platform
and memory fixture at `b5468739f2d7e36d64198de743a13d6ccabc6666`, with no External
route or executable-memory qualification. These adaptations add no license grant.
