"""Run the unmodified production Server with schema-2 archives and User ELFs."""
import argparse
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('kernel', type=Path)
    parser.add_argument('--profile', choices=('debug', 'release'), default='release')
    args = parser.parse_args()
    guest = Path(__file__).resolve().parent
    root = guest.parents[2]
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location('system_capsule', guest.parent / 'client/run.py')
    capsule = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(capsule)
    triple = 'riscv64gc-unknown-none-elf'
    profile = ['--release'] if args.profile == 'release' else []
    subprocess.run(['cargo', 'build', '--locked', '-p', 'system-server', '--bin', 'system', '--target', triple, *profile], cwd=root, check=True)
    system = root / 'target' / triple / args.profile / 'system'
    subprocess.run(['cargo', 'build', '--locked', '--manifest-path', guest.parent / 'client/image/Cargo.toml', '--release', '--target', triple], check=True)
    image = guest.parent / 'client/image/target' / triple / 'release/system-loader-image'
    pack = ['cargo', 'run', '--locked', '--release', '--target', 'x86_64-unknown-linux-gnu', '--manifest-path', guest / 'pack/Cargo.toml', '--']
    with tempfile.TemporaryDirectory(prefix='system-app-') as directory:
        output = Path(directory)
        boot = output / 'capsule'
        boot.write_bytes(capsule.initrd(system.read_bytes())[4096:])
        for users in (0, 2, 1):
            archive = output / f'initrd-{users}'
            subprocess.run([*pack, boot, system, image, archive, str(users)], check=True)
            command = [os.environ.get('QEMU_RISCV64', 'qemu-system-riscv64'), '-M', 'virt', '-cpu', 'rv64', '-m', '128M', '-smp', '1', '-bios', os.environ.get('QEMU_OPENSBI', 'default'), '-kernel', args.kernel.resolve(strict=True), '-initrd', archive, '-nographic', '-monitor', 'none', '-nic', 'none']
            try:
                result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=90)
            except subprocess.TimeoutExpired as error:
                print((error.stdout or b'').decode(errors='replace'), flush=True)
                raise SystemExit('full System application timeout') from error
            print(result.stdout.decode(errors='replace'), flush=True)
            result.check_returncode()
            assert b'page fault killed' not in result.stdout and b'system: panic' not in result.stdout
            final = b'system: supervision failed (4)' if users == 1 else b'system: complete (0)'
            assert b'all tasks exited' in result.stdout and final in result.stdout
            log = output / 'serial.log'
            log.write_bytes(result.stdout)
            subprocess.run([*pack, 'check', log, str(users)], check=True)

if __name__ == '__main__':
    main()
