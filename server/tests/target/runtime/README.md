# Native heap, Supply and RPC

Uses the production System heap, native memory result owner, Supply lease
backend, Client and authenticated ingress. Four real Tasks transfer live
allocations to the initial Task, exit with distinct results and are observed
reclaimed/deleted. ROOT reads those allocations after their original Task exits;
complete frees restore both backing mappings and the Task-local Pend baseline.
ROOT also allocates under worker contention; a later three-MiB allocation
exercises heap growth.

Supply checks real rights, caller/recipient scope, identical versus conflicting
replay, cancellation before/after derivation, delayed IDs, original-source
retention and complete withdrawal. Eight distinct page contents verify that
two relative narrowing steps deliver original page 2, once, through an actual
native read-only publication. Publication retirement precedes lease
withdrawal. The projection owner is cancelled at each partial-open phase; each
close restores the Pend baseline and leaves its borrowed source active.

RPC makes two actual protocol-v4 calls using production Client/Ingress and a
production native Worker. The caller and worker explicitly share their Team's
Internal Nest through recipient-local grants. Heap and reply mapping use the
same monotonic request allocator. The guest checks actual request/reply senders,
typed responses, retained cleanup, exit result 31 and Kernel reclamation. This
is a same-Team component scenario, not independent-Team or full System
acceptance.

## Run

The common invocation is the candidate Rust host runner described in the
[target scenario guide](../README.md). It builds the fixture and runs the
qualified data-only QEMU platform at one and four Harts:

```sh
export QEMU_RISCV64=/path/to/qemu-system-riscv64
export QEMU_OPENSBI=/path/to/fw_dynamic.bin
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- runtime --kernel PATH --output NEW --harts 1 4
```

The guest has a 120-second wall-clock bound per VM and finite progress
deadlines. The runner must require all four exact guest markers:

```text
PASS system-runtime heap=1 growing=1 shared=1 owner=1 worker-exit=1 reclaim=1 contention=1
PASS supply-native rights=1 scope=1 replay=1 cancel=1 reorder=1 withdrawal=1 cleanup=1
PASS supply-window relative=1 narrowed=1 once=1 cleanup=1
PASS system-rpc abi=13 protocol=4 context=1 sender=1 reply=1 cleanup=1 worker-reclaimed=1
```

It must also require a successful QEMU exit. Marker text alone is not proof of
physical reclamation. The fixture attests coherent CPU RAM only under the
paired QEMU `virt` invocation with 128 MiB, no hotplug/passthrough and no
External routes. It qualifies no executable publication and is not production
platform admission.

The selected platform dependencies are Kernel
`789ffd19b5359f3de172fb9aad8a61f6a7113a64` and Vanllia
`0eb086a2416eee8921416b12bf94819b30b6bb1f`, as recorded in
[`platform/Cargo.toml`](platform/Cargo.toml) and its lockfile. This is the
data-only platform. The separate `instructions` feature qualifies the Loader
1/2/4-Hart scenario; the normal platform still rejects executable memory.
Preserve both fail-closed boundaries.

Full Supply RPC/provider collections, Control/Instance/UnitRoot integration,
Toolchain startup, complete System and device retirement remain separate gates
in System #13 and Vanllia #39. The [Loader scenario](../loader/README.md)
checks native ELF construction and independent User Team retirement. Neither
component fixture substitutes for the complete System application. The Rust
runner and old/new CI parity are still migration work; the existing
`native.yml` checks remain in force until candidate equivalence is established.
