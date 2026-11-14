"""Verify native Loader construction, ordinary User Tasks and explicit retirement."""
import argparse
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile

MARKER = b"PASS loader-elf native=1 user=1 arguments=1 private=1 cache=1 eviction=1 cancel=1 running-stop=1 journal=1 reclaim=1"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kernel", type=Path)
    parser.add_argument("--harts", type=int, choices=(1, 4), nargs="+", default=[1, 4])
    args = parser.parse_args()
    guest = Path(__file__).resolve().parent
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("system_client_runner", guest.parent / "client/run.py")
    capsule = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(capsule)
    with tempfile.TemporaryDirectory(prefix="system-loader-") as temporary:
        target = Path(temporary)
        triple = "riscv64gc-unknown-none-elf"
        subprocess.run(["cargo", "build", "--locked", "--release", "--target", triple,
                        "--manifest-path", guest.parent / "client/image/Cargo.toml", "--target-dir", target], check=True)
        environment = {**os.environ, "SYSTEM_LOADER_IMAGE": str(target / triple / "release/system-loader-image")}
        subprocess.run(["cargo", "build", "--locked", "--release", "--target", triple,
                        "--manifest-path", guest / "Cargo.toml", "--target-dir", target], check=True, env=environment)
        image = target / "initrd"
        image.write_bytes(capsule.initrd((target / triple / "release/system-loader-guest").read_bytes()))
        for count in args.harts:
            harts = str(count)
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
            if MARKER not in result.stdout or b"FAIL system-loader" in result.stdout or b"task: all tasks exited" not in result.stdout:
                raise SystemExit(f"Loader invariant failed: {harts} Hart(s)")


if __name__ == "__main__":
    main()
