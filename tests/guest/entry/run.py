"""Boot the production entry with its real ledger/DTB/heap and a contract fixture."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import shutil
import sys
import tempfile
MARKER = b"PASS system-entry words=4 ledger=11 catalog=2 dtb=1 heap=1"
SERVICES = b"PASS entry-services heap=production roles=2 root=1 wired=1 retire=1"
DATA = b"PASS boot-data native=1 readonly=1 content=1 padding=1 retire=1"

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kernel", type=Path)
    parser.add_argument("--harts", type=int, choices=(1, 4), nargs="+", default=[1, 4])
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--abort", action="store_true", help="force root failure with two live workers")
    mode.add_argument("--stall", action="store_true", help="freeze a startup worker and require its deadline")
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
                        "--manifest-path", guest / "Cargo.toml", "--target-dir", output,
                        *(["--features", "abort"] if args.abort else
                          ["--features", "stall"] if args.stall else [])], check=True)
        image = output / triple / "release/system"
        boot = output / "capsule"
        boot.write_bytes(capsule.initrd(image.read_bytes())[4096:])
        archive = output / "initrd"
        subprocess.run(["cargo", "run", "--locked", "--release", "--target", "x86_64-unknown-linux-gnu",
                        "--manifest-path", guest / "pack/Cargo.toml", "--", boot, image, archive], check=True)
        kernel = args.kernel.resolve(strict=True)
        mode = "abort" if args.abort else "stall" if args.stall else "normal"
        evidence = (guest / "target/evidence" / mode
                    / f"kernel-{digest(kernel)[:16]}-system-{digest(image)[:16]}")
        evidence.mkdir(parents=True, exist_ok=True)
        for source in (image, archive, guest / "Cargo.lock"):
            shutil.copyfile(source, evidence / source.name)
        firmware = os.environ.get("QEMU_OPENSBI", "default")
        inputs = {"mode": mode, "system_profile": "release", "kernel_sha256": digest(kernel),
                  "system_sha256": digest(image), "rustflags": os.environ.get("RUSTFLAGS"),
                  "qemu": subprocess.check_output([os.environ.get("QEMU_RISCV64", "qemu-system-riscv64"), "--version"], text=True),
                  "archive_sha256": digest(archive), "kernel": str(kernel), "firmware": firmware,
                  "firmware_sha256": digest(Path(firmware)) if Path(firmware).is_file() else None,
                  "rustc": subprocess.check_output(["rustc", "-Vv"], text=True)}
        (evidence / "inputs.json").write_text(json.dumps(inputs, indent=2) + "\n")
        for count in args.harts:
            harts = str(count)
            command = [os.environ.get("QEMU_RISCV64", "qemu-system-riscv64"), "-M", "virt", "-cpu", "rv64",
                       "-m", "128M", "-smp", harts, "-bios", os.environ.get("QEMU_OPENSBI", "default"),
                       "-kernel", kernel, "-initrd", archive,
                       "-nographic", "-monitor", "none", "-nic", "none"]
            case = evidence / f"harts-{harts}"
            receipt = {"command": list(map(str, command)), "passed": False, "harts": count}
            case.with_suffix(".json").write_text(json.dumps(receipt, indent=2) + "\n")
            try:
                result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=90)
            except subprocess.TimeoutExpired as error:
                case.with_suffix(".log").write_bytes(error.stdout or b"")
                print((error.stdout or b"").decode(errors="replace"), flush=True)
                raise SystemExit(f"entry timeout: {harts} Hart(s)") from error
            print(f"QEMU {harts} Hart(s)\n{result.stdout.decode(errors='replace')}", flush=True)
            case.with_suffix(".log").write_bytes(result.stdout)
            result.check_returncode()
            markers = ((b"EXPECTED entry-abort workers=2 backing=live",
                        b"exit tid=1 reason=0x2a note: system: injected entry failure") if args.abort
                       else (b"EXPECTED entry-stall timed-out=1",
                             b"exit tid=1 reason=0x2b note: system: stalled entry bootstrap") if args.stall
                       else (MARKER, SERVICES, DATA, b"exit tid=1 reason=0x0 note: system: complete"))
            if (any(marker not in result.stdout for marker in markers)
                    or any(marker in result.stdout for marker in
                           (b"panicked", b"page fault killed", b"reason=4294967295"))
                    or b"all tasks exited" not in result.stdout):
                raise SystemExit(f"entry contract failed: {harts} Hart(s)")
            if args.abort:
                print(f"PASS entry-abort harts={harts} root=1 workers=2 reclaimed=1")
            if args.stall:
                print(f"PASS entry-stall harts={harts} timeout=1 root=1")
            receipt.update(passed=True, returncode=result.returncode)
            case.with_suffix(".json").write_text(json.dumps(receipt, indent=2) + "\n")

if __name__ == "__main__":
    main()
