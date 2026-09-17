#!/usr/bin/env python3
"""Build and deploy the server to an existing Pi installation over SSH."""
import argparse
from pathlib import Path
import shlex
import subprocess
import tarfile
import tempfile
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', default='rob@192.168.0.73')
    parser.add_argument('--key', type=Path, default=Path.home() / '.ssh/custom_plex_pi')
    parser.add_argument('--directory', default='/home/rob/custom-plex')
    args = parser.parse_args()
    if args.host.startswith('-') or not args.directory.startswith('/'):
        parser.error('Use an SSH host and an absolute deployment directory.')
    root = Path(__file__).resolve().parent.parent
    release = 'custom-plex:deploy-' + uuid.uuid4().hex[:12]
    ssh = ['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=15', '-i', str(args.key), args.host]

    def remote(command, **kwargs):
        return subprocess.run(ssh + [shlex.join(command)], check=True, **kwargs)

    remote(['bash', '-c', 'test -f "$1/.env" && sudo -n docker info >/dev/null && test "$(uname -m)" = aarch64', 'deploy', args.directory])
    subprocess.run(['docker', 'build', '--platform', 'linux/arm64', '-t', release, '.'], cwd=root, check=True)
    staging = remote(['mktemp', '-d', '/tmp/custom-plex-deploy.XXXXXXXX'], capture_output=True, text=True).stdout.strip()
    try:
        with tempfile.TemporaryDirectory(prefix='custom-plex-deploy-') as temporary:
            source = Path(temporary) / 'source.tar.gz'
            with tarfile.open(source, 'w:gz') as archive:
                for name in ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'Dockerfile', '.dockerignore', 'compose.yaml', 'compose.storage.example.yaml', '.env.example', 'README.md', 'src', 'web', 'scripts', 'tests', '.github']:
                    archive.add(root / name, arcname=name, filter=lambda info: None if '__pycache__' in info.name else info)
            image = Path(temporary) / 'image.tar'
            subprocess.run(['docker', 'save', '-o', str(image), release], check=True)
            # Stream through SSH so paths never depend on scp's remote-shell parsing.
            for path in [source, image]:
                with path.open('rb') as stream:
                    remote(['bash', '-c', 'cat > "$1"', 'upload', staging + '/' + path.name], stdin=stream)
        code = (root / 'scripts/deploy_pi_remote.py').read_text()
        remote(['python3', '-', args.directory, staging, release], input=code, text=True)
    finally:
        remote(['rm', '-rf', '--', staging])


if __name__ == '__main__':
    main()
