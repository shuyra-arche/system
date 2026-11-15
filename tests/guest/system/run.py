"""Run the production Server with schema-2 archives and real ordinary User ELFs."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_stack(elf):
    """The lifetime stack must be writable, non-executable and have a load gap."""
    import struct
    table = struct.unpack_from('<Q', elf, 32)[0]
    width, count = struct.unpack_from('<HH', elf, 54)
    loads = [struct.unpack_from('<IIQQQQQQ', elf, table + index * width)
             for index in range(count)]
    loads = [header for header in loads if header[0] == 1]
    stacks = [header for header in loads if header[1] == 6 and header[5] == 0
              and header[6] >= 1024 * 1024]
    assert len(stacks) == 1, 'missing separate RW lifetime stack'
    stack = stacks[0]
    assert stack[3] % 4096 == 0 and stack[6] % 4096 == 0
    guard = stack[3] - 4096
    assert all(header[3] + header[6] <= guard or header[3] >= stack[3]
               for header in loads), 'a PT_LOAD covers the stack guard'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('kernel', type=Path)
    parser.add_argument('--profile', choices=('debug', 'release'), default='release')
    parser.add_argument('--harts', type=int, choices=(1, 2, 4), nargs='+', default=[1])
    parser.add_argument('--kernel-profile', choices=('debug', 'release'), default='release',
                        help='records the selected platform profile and its handoff execution budget')
    parser.add_argument('--handoff', action='store_true', help='independent-Team Instance/UnitRoot/Supply handoff')
    args = parser.parse_args()
    slow_handoff = args.handoff and args.kernel_profile == "debug"
    timeout = 180 if slow_handoff else 90
    handoff_budget = 120 if slow_handoff else 60
    guest = Path(__file__).resolve().parent
    root = guest.parents[2]
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location('system_capsule', guest.parent / 'client/run.py')
    capsule = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(capsule)
    triple = 'riscv64gc-unknown-none-elf'
    profile = ['--release'] if args.profile == 'release' else []
    subprocess.run(['cargo', 'build', '--locked', '-p', 'system-server', '--bin', 'system',
                    '--target', triple, *profile], cwd=root, check=True)
    system = root / 'target' / triple / args.profile / 'system'
    user = guest.parent / ('handoff' if args.handoff else 'client/image')
    subprocess.run(['cargo', 'build', '--locked', '--manifest-path', user / 'Cargo.toml',
                    '--release', '--target', triple], check=True,
                   env={**os.environ, 'SHUYRA_HANDOFF_BUDGET_SECONDS': str(handoff_budget)})
    image = user / 'target' / triple / 'release' / ('system-handoff-guest' if args.handoff else 'system-loader-image')
    pack = ['cargo', 'run', '--locked', '--release', '--target', 'x86_64-unknown-linux-gnu',
            '--manifest-path', guest / 'pack/Cargo.toml', '--']
    kernel = args.kernel.resolve(strict=True)
    firmware = os.environ.get('QEMU_OPENSBI', 'default')
    qemu = os.environ.get('QEMU_RISCV64', 'qemu-system-riscv64')
    evidence = (guest / ('target/handoff' if args.handoff else 'target/evidence') / args.profile
                / f'kernel-{digest(kernel)[:16]}-system-{digest(system)[:16]}-user-{digest(image)[:16]}')
    evidence.mkdir(parents=True, exist_ok=True)
    # Keep failed and repeated attempts; never overwrite an earlier receipt.
    output = Path(tempfile.mkdtemp(prefix='run-', dir=evidence))
    check_stack(system.read_bytes())
    boot = output / 'capsule'
    boot.write_bytes(capsule.initrd(system.read_bytes())[4096:])
    shutil.copyfile(system, output / 'system')
    shutil.copyfile(image, output / image.name)
    for source, target in [(root / 'Cargo.lock', 'system.lock'),
                           (user / 'Cargo.lock', 'user.lock'),
                           (guest.parent / 'runtime/platform/Cargo.lock', 'platform.lock')]:
        shutil.copyfile(source, output / target)
    # Checkout can be owned by the runner outside its test container. Trust
    # this exact selected checkout for these read-only provenance commands.
    git = ['git', '-c', f'safe.directory={root}']
    identity = {'system_profile': args.profile, 'user_profile': 'release',
                'kernel_profile': args.kernel_profile, 'timeout_seconds': timeout,
                'handoff_budget_seconds': handoff_budget,
                'mode': 'handoff' if args.handoff else 'application',
                'system_revision': subprocess.check_output([*git, 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
                'system_dirty': bool(subprocess.check_output([*git, 'status', '--porcelain', '--', '.', ':!.evidence'], cwd=root)),
                'system_sha256': digest(system), 'user_sha256': digest(image),
                'kernel': str(kernel), 'kernel_sha256': digest(kernel),
                'firmware': firmware,
                'firmware_sha256': digest(Path(firmware)) if Path(firmware).is_file() else None,
                'rustc': subprocess.check_output(['rustc', '-Vv'], text=True),
                'qemu': subprocess.check_output([qemu, '--version'], text=True)}
    (output / 'inputs.json').write_text(json.dumps(identity, indent=2) + '\n')
    for users in (('handoff', 'auth', 'cancel', 'death') if args.handoff else (0, 2, 1)):
        archive = output / f'initrd-{users}.bin'
        subprocess.run([*pack, boot, system, image, archive, str(users)], check=True)
        for harts in args.harts:
            for attempt in range(2):
                suffix = '' if attempt == 0 else '-retry-1'
                case = output / f'users-{users}-harts-{harts}{suffix}'
                command = [qemu, '-M', 'virt', '-cpu', 'rv64', '-m', '128M', '-smp', str(harts),
                           '-bios', firmware, '-kernel', kernel, '-initrd', archive,
                           '-nographic', '-monitor', 'none', '-nic', 'none']
                receipt = {'command': list(map(str, command)), 'users': users, 'harts': harts,
                           'archive_sha256': digest(archive), 'attempt': attempt + 1, 'passed': False}
                try:
                    # Preserve live diagnostics, including while a guest stalls.
                    with case.with_suffix('.log').open('wb') as serial_output:
                        result = subprocess.run(command, stdout=serial_output, stderr=subprocess.STDOUT, timeout=timeout)
                    receipt['returncode'] = result.returncode
                    serial = case.with_suffix('.log').read_bytes()
                except subprocess.TimeoutExpired as error:
                    serial = case.with_suffix('.log').read_bytes()
                    receipt['timeout'] = True
                    case.with_suffix('.log').write_bytes(serial)
                    case.with_suffix('.json').write_text(json.dumps(receipt, indent=2) + '\n')
                    print(serial.decode(errors='replace'), flush=True)
                    raise SystemExit(f'full System application timeout: {harts} Hart(s)') from error
                case.with_suffix('.log').write_bytes(serial)
                print(f'QEMU System {args.profile} / {harts} Hart(s) / {users} User(s)\n'
                      f'{serial.decode(errors="replace")}', flush=True)
                case.with_suffix('.json').write_text(json.dumps(receipt, indent=2) + '\n')
                result.check_returncode()
                assert b'page fault killed' not in serial and b'system: panic' not in serial
                final = b'system: supervision failed (4)' if users == 1 else b'system: complete (0)'
                assert b'all tasks exited' in serial and final in serial
                checked = subprocess.run([*pack, 'check', case.with_suffix('.log'), str(users)])
                if args.handoff and checked.returncode == 2:
                    receipt['inconclusive'] = 'kernel-console-loss'
                    case.with_suffix('.json').write_text(json.dumps(receipt, indent=2) + '\n')
                    if attempt == 0:
                        print(f'INCONCLUSIVE {case}: preserving evidence; qualifying one fresh VM', flush=True)
                        continue
                checked.check_returncode()
                receipt['passed'] = True
                case.with_suffix('.json').write_text(json.dumps(receipt, indent=2) + '\n')
                break


if __name__ == '__main__':
    main()
