"""Boot the System Client integration guest with a built Shuyra ABI 9 QEMU image."""

import argparse
import os
from pathlib import Path
import struct
import subprocess
import tempfile

PAGE = 4096
PASS = b"PASS system-client typed=1 sender=1 poll=1 variant=1 decode=1 partial=1 timeout=1 late=1 backpressure=1 send-timeout=1 cleanup=1 ingress=1 route=1 shutdown=1 identity=1"
REQUIRED = [
    b"PASS supervisor-bootstrap native=1 roles=2 root=1 wired=1 revision=1 partial=1 cleanup=1",
    b"PASS supervisor-worker native=1 dormant=1 arguments=1 exit=1 cancel=1 reclaim=1",
    b"PASS operator-service native=1 namespace=1 scope=1 source=1 unbind=1 revoke=1 replay=1 watch=1 cleanup=1",
    PASS,
    b"PASS reply-adjacency local=1 creator=1 sibling-denied=1",
    b"PASS identity-actor faces=20 fair=1 revision=1 coalesced=1 failed-reply=1 revoked=1 partial=1 shutdown=1",
    b"PASS wait-group dedup=1 replace=1 partial=1 conflict=1 cleanup=1",
    b"PASS authority-query native=1 sender=1 denied=1 unjudged=1 cleanup=1",
    b"PASS authority-life attach=1 grant=1 revoke=1 return=1 cleanup=1",
]


def initrd(elf: bytes) -> bytes:
    assert elf[:6] == b"\x7fELF\x02\x01"
    entry, table = struct.unpack_from("<QQ", elf, 24)
    width, count = struct.unpack_from("<HH", elf, 54)
    headers = [struct.unpack_from("<IIQQQQQQ", elf, table + i * width) for i in range(count)]
    regions = bytearray()
    payload = bytearray()
    for kind, flags, offset, address, _, length, size, _ in headers:
        if kind != 1:
            continue
        assert flags in (5, 6) and length <= size and address % PAGE == 0
        pages = (size + PAGE - 1) // PAGE
        access = ((flags & 4) >> 2) | (flags & 2) | ((flags & 1) << 2)
        regions += struct.pack("<QQQQQ", address, pages, pages, PAGE + len(payload), access)
        payload += elf[offset : offset + length].ljust(pages * PAGE, b"\0")
    total = PAGE + len(payload)
    region_count = len(regions) // 40
    header = struct.pack("<8sHBBIQQ", b"SQBOOT01", 2, 12, 9, region_count, entry, total)
    assert len(header) + len(regions) <= PAGE
    return (struct.pack("<II", PAGE, total).ljust(PAGE, b"\0")
            + (header + regions).ljust(PAGE, b"\0") + payload)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kernel", type=Path, help="Shuyra RV64 virt boot image")
    args = parser.parse_args()
    kernel = args.kernel.resolve(strict=True)
    guest = Path(__file__).resolve().parent
    qemu = os.environ.get("QEMU_RISCV64", "qemu-system-riscv64")
    bios = os.environ.get("QEMU_OPENSBI", "default")
    with tempfile.TemporaryDirectory(prefix="system-client-qemu-") as directory:
        target = Path(directory)
        triple = "riscv64gc-unknown-none-elf"
        subprocess.run(
            [
                "cargo", "build", "--locked", "--release", "--target", triple,
                "--manifest-path", guest / "Cargo.toml", "--target-dir", target,
            ],
            cwd=guest,
            check=True,
        )
        image = target / "initrd"
        image.write_bytes(initrd((target / triple / "release/system-client-guest").read_bytes()))
        for harts in ("1", "4"):
            command = [
                qemu, "-M", "virt", "-cpu", "rv64", "-m", "128M", "-smp", harts,
                "-bios", bios, "-kernel", kernel, "-initrd", image,
                "-nographic", "-monitor", "none", "-nic", "none",
            ]
            try:
                output = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60)
            except subprocess.TimeoutExpired as error:
                print((error.stdout or b"").decode(errors="replace"), flush=True)
                raise SystemExit(f"QEMU timed out with {harts} hart(s)") from error
            print(f"QEMU {harts} hart(s)\n{output.stdout.decode(errors='replace')}", flush=True)
            output.check_returncode()
            if any(marker not in output.stdout for marker in REQUIRED) or b"FAIL system-client" in output.stdout:
                raise SystemExit(f"guest assertions failed with {harts} hart(s)")


if __name__ == "__main__":
    main()
