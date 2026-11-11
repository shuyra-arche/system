"""Boot the production entry with its real ledger/DTB/heap and a contract fixture."""
import argparse
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
MARKER = b"PASS system-entry words=4 ledger=9 catalog=2 dtb=1 heap=1"
SERVICES = b"PASS entry-services heap=production roles=2 root=1 wired=1 retire=1"

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kernel", type=Path)
    args = parser.parse_args()
    guest = Path(__file__).resolve().parent
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("system_capsule", guest.parent / "client/run.py")
    capsule = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(capsule)
    with tempfile.TemporaryDirectory(prefix="system-entry-") as directory:
        output = Path(directory)
        triple = "riscv64gc-unknown-none-elf"
        subprocess.run(["cargo", "build", "--locked", "--release", "--target", triple,
                        "--manifest-path", guest / "Cargo.toml", "--target-dir", output], check=True)
        image = output / triple / "release/system"
        boot = output / "capsule"
        boot.write_bytes(capsule.initrd(image.read_bytes())[4096:])
        archive = output / "initrd"
        subprocess.run(["cargo", "run", "--locked", "--release", "--target", "x86_64-unknown-linux-gnu",
                        "--manifest-path", guest / "pack/Cargo.toml", "--", boot, image, archive], check=True)
        for harts in ("1", "4"):
            command = [os.environ.get("QEMU_RISCV64", "qemu-system-riscv64"), "-M", "virt", "-cpu", "rv64",
                       "-m", "128M", "-smp", harts, "-bios", os.environ.get("QEMU_OPENSBI", "default"),
                       "-kernel", args.kernel.resolve(strict=True), "-initrd", archive,
                       "-nographic", "-monitor", "none", "-nic", "none"]
            try:
                result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=90)
            except subprocess.TimeoutExpired as error:
                print((error.stdout or b"").decode(errors="replace"), flush=True)
                raise SystemExit(f"entry timeout: {harts} Hart(s)") from error
            print(f"QEMU {harts} Hart(s)\n{result.stdout.decode(errors='replace')}", flush=True)
            result.check_returncode()
            if MARKER not in result.stdout or SERVICES not in result.stdout or b"panicked" in result.stdout:
                raise SystemExit(f"entry contract failed: {harts} Hart(s)")

if __name__ == "__main__":
    main()
