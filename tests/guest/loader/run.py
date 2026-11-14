"""Verify native Loader construction, ordinary User Tasks and explicit retirement."""
import argparse
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import hashlib
import json
import shutil
import time

MARKER = b"PASS loader-elf native=1 user=1 arguments=1 private=1 cache=1 eviction=1 cancel=1 running-stop=1 journal=1 reclaim=1"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kernel", type=Path)
    parser.add_argument("--harts", type=int, choices=(1, 2, 4), nargs="+", default=[1, 2, 4])
    parser.add_argument("--profile", choices=("debug", "release"), default="release")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    guest = Path(__file__).resolve().parent
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("system_client_runner", guest.parent / "client/run.py")
    capsule = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(capsule)
    target = (args.output or guest / "target/evidence" / args.profile).resolve()
    target.mkdir(parents=True, exist_ok=True)
    triple = "riscv64gc-unknown-none-elf"
    profile = ["--release"] if args.profile == "release" else []
    inputs = {"rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
              "qemu": subprocess.check_output([os.environ.get("QEMU_RISCV64", "qemu-system-riscv64"), "--version"], text=True),
              "profile": args.profile, "image_profile": "release", "build": []}
    # Keep the ordinary User ELF identical across Loader/kernel profiles.
    # The fixture's cancellation points and payload are the original release
    # image; debug here exercises the production Loader and Kernel themselves.
    image_elf = target / triple / "release" / "system-loader-image"
    environment = {**os.environ, "SYSTEM_LOADER_IMAGE": str(image_elf)}
    for root in [guest.parent / "client/image", guest]:
        selected = profile if root == guest else ["--release"]
        command = ["cargo", "build", "--locked", *selected, "--target", triple,
                   "--manifest-path", str(root / "Cargo.toml"), "--target-dir", str(target)]
        inputs["build"].append(command)
        subprocess.run(command, check=True, env=environment)
    image = target / "initrd.bin"
    image.write_bytes(capsule.initrd((target / triple / args.profile / "system-loader-guest").read_bytes()))
    kernel = args.kernel.resolve(strict=True)
    firmware = Path(os.environ["QEMU_OPENSBI"]).resolve(strict=True)
    for name, source in [("guest.Cargo.lock", guest / "Cargo.lock"),
                         ("image.Cargo.lock", guest.parent / "client/image/Cargo.lock"),
                         ("platform.Cargo.lock", guest.parent / "runtime/platform/Cargo.lock")]:
        shutil.copyfile(source, target / name)
    inputs["sha256"] = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in
        [kernel, firmware, image, image_elf, target / triple / args.profile / "system-loader-guest",
         target / "guest.Cargo.lock", target / "image.Cargo.lock", target / "platform.Cargo.lock"]}
    (target / "inputs.json").write_text(json.dumps(inputs, indent=2))
    failures = []
    for count in args.harts:
        command = [os.environ.get("QEMU_RISCV64", "qemu-system-riscv64"),
                   "-accel", "tcg,thread=multi", "-M", "virt", "-cpu", "rv64",
                   "-m", "128M", "-smp", str(count), "-bios", str(firmware),
                   "-kernel", str(kernel), "-initrd", str(image),
                   "-nographic", "-monitor", "none", "-nic", "none"]
        started = time.monotonic()
        serial = target / f"harts-{count}.log"
        with serial.open("wb") as log:
            try:
                status = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=120).returncode
            except subprocess.TimeoutExpired:
                status = "timeout"
        output = serial.read_bytes()
        passed = status == 0 and MARKER in output and b"FAIL" not in output and b"task: all tasks exited" in output
        (target / f"harts-{count}.json").write_text(json.dumps({"command": command,
            "status": status, "seconds": time.monotonic() - started, "passed": passed}, indent=2))
        print(f"QEMU {count} Hart(s)\n{output.decode(errors='replace')}", flush=True)
        if not passed:
            failures.append(count)
    assert not failures, f"Loader invariant failed on Harts {failures}; evidence: {target}"


if __name__ == "__main__":
    main()
