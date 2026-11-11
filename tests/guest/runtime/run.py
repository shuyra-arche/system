"""Verify the production System heap against real Kernel9 on one/four Harts."""
import argparse
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile

MARKER = b"PASS system-runtime heap=1 growing=1 shared=1 owner=1 worker-exit=1 reclaim=1 contention=1"
SUPPLY = b"PASS supply-native rights=1 scope=1 replay=1 cancel=1 reorder=1 withdrawal=1 cleanup=1"
WINDOW = b"PASS supply-window relative=1 narrowed=1 once=1 cleanup=1"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kernel", type=Path)
    args = parser.parse_args()
    guest = Path(__file__).resolve().parent
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("system_client_runner", guest.parent / "client/run.py")
    capsule = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(capsule)
    with tempfile.TemporaryDirectory(prefix="system-runtime-") as temporary:
        target = Path(temporary)
        triple = "riscv64gc-unknown-none-elf"
        subprocess.run(["cargo", "build", "--locked", "--release", "--target", triple,
                        "--manifest-path", guest / "Cargo.toml", "--target-dir", target], check=True)
        image = target / "initrd"
        image.write_bytes(capsule.initrd((target / triple / "release/system-runtime-guest").read_bytes()))
        for harts in ("1", "4"):
            command = [os.environ.get("QEMU_RISCV64", "qemu-system-riscv64"),
                       "-M", "virt", "-cpu", "rv64", "-m", "128M", "-smp", harts,
                       "-bios", os.environ.get("QEMU_OPENSBI", "default"),
                       "-kernel", args.kernel.resolve(strict=True), "-initrd", image,
                       "-nographic", "-monitor", "none", "-nic", "none"]
            try:
                result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
            except subprocess.TimeoutExpired as error:
                print((error.stdout or b"").decode(errors="replace"), flush=True)
                raise SystemExit(f"QEMU timed out: {harts} Hart(s)") from error
            print(f"QEMU {harts} Hart(s)\n{result.stdout.decode(errors='replace')}", flush=True)
            result.check_returncode()
            if any(marker not in result.stdout for marker in (MARKER, SUPPLY, WINDOW)) or b"FAIL system-runtime" in result.stdout:
                raise SystemExit(f"runtime invariant failed: {harts} Hart(s)")


if __name__ == "__main__":
    main()
