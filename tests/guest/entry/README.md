# Native entry contract

This standalone fixture compiles the exact production `server/src/main.rs`,
production linker, boot decoder, schema-2 interpreter, heap and internal-service
bootstrap. The host helper uses the pure Unit provider to pack a genuine VNI1
archive containing its VNU2 System declaration and a current Kernel capsule.
The initial Task receives and validates all four Kernel boot words, projects the
actual DTB with native Reserve/Insert/Publish and explicitly retires that temporary mapping, decodes schema 2 and allocates through the production heap. Actual
Operator and Identity workers complete Root/Wired and retire through native
Task reclaim observations on that same heap.

Its library supplies a bounded contract-test application body. This tests entry
and runtime contracts separately from the complete production application and
does not launch a std User Unit or prove supervision recovery.

```sh
export QEMU_RISCV64=/path/to/qemu-system-riscv64
export QEMU_OPENSBI=/path/to/fw_jump.bin
python3 tests/guest/entry/run.py /path/to/shuyra
cargo clippy --locked --manifest-path tests/guest/entry/Cargo.toml --target riscv64gc-unknown-none-elf -- -D warnings
```

Requires boot-data, entry and actual-service markers within 90 seconds per guest.
Boot-data checks immutable DTB/initrd grants, rejection of writable Publish and
permission widening, the complete initrd contents and zeroed tail padding, and
restored Pend count after projection retirement. Use the runtime component
platform with an explicitly qualified CPU topology. The default includes one
and four Harts; this does not qualify hardware or External requesters.

`--abort --harts 1 4` starts two real workers reading root-owned heap storage,
then invokes production fatal retirement. Kernel must admit the root's intended
exit, all Tasks must finish, and no worker may page-fault. `--stall --harts 1`
freezes the startup Operator and requires the unchanged five-second inactivity
bound to expire despite continued polling; fatal retirement must then reclaim
that worker. All modes require Kernel's actual root exit, not just fixture
markers, within the original 90-second guest bound. Test features affect only
this fixture package; the production binary and termination owner are shared.
