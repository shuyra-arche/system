# Native entry contract

This standalone fixture compiles the exact production `server/src/main.rs`,
production linker, boot decoder, schema-2 interpreter, heap and internal-service
bootstrap. The host helper uses the pure Unit provider to pack a genuine VNI1
archive containing its VNU2 System declaration and a current Kernel capsule.
The initial Task receives and validates all four Kernel boot words, projects the
actual DTB with native Reserve/Insert/Publish and explicitly retires that temporary mapping, decodes schema 2 and allocates through the production heap. Actual
Operator and Identity workers complete Root/Wired and retire through native
Task reclaim observations on that same heap.

Its library supplies a bounded contract-test application body in place of the
unmigrated complete supervisor. It does not replace the production application,
claim a buildable full Server, launch a std User Unit or prove M4 recovery.

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
platform and pass `--harts 1` for the supported single-Hart gate. The default
still includes four Harts; Kernel rejects that memory topology before entry
([Kernel #179](https://github.com/shuyra-arche/kernel/issues/179)). The manifest explicitly names the fixture package, its library and
the production binary source; no production feature disables old dependencies
or swaps out the complete Server for this test.
