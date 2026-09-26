"""Remote helper invoked by deploy_pi.py; do not run directly."""
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys


def run(*command, **kwargs):
    return subprocess.run(command, check=True, **kwargs)


def photo_storage(compose):
    """Honor an explicit location; otherwise require the Pi's mounted large disk."""
    environment = run(*compose, 'config', '--environment', capture_output=True, text=True).stdout
    values = dict(line.split('=', 1) for line in environment.splitlines() if '=' in line)
    configured = values.get('HOST_PHOTO_DIR', '').strip()
    if configured:
        location = str(Path(configured).resolve())
        run('sudo', '-n', 'test', '-d', location)
    else:
        run('mountpoint', '-q', '/mnt/dvd-library')
        location = '/mnt/dvd-library/Custom-Plex-Photos'
        run('sudo', '-n', 'mkdir', '-p', location)
        # exFAT uses mount-level ownership; chown is deliberately not used here.
    return location, not configured


def main():
    directory, staging, release = sys.argv[1:]
    os.chdir(directory)
    compose = ['sudo', '-n', 'docker', 'compose']
    config = json.loads(run(*compose, 'config', '--format', 'json', capture_output=True, text=True).stdout)
    service = config['services']['app']
    data = next(v['source'] for v in service['volumes'] if v['target'] == '/data' and v['type'] == 'bind')
    # The container owns this private directory; the SSH user cannot stat it.
    try:
        run('sudo', '-n', 'test', '-f', str(Path(data) / 'library.sqlite3'))
    except subprocess.CalledProcessError as error:
        raise RuntimeError('Cannot verify the existing database. Check HOST_DATA_DIR and passwordless sudo; this script only updates existing installations.') from error
    photo_root, configure_photos = photo_storage(compose)
    image = service['image']
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    backup = Path(directory).parent / 'custom-plex-backups' / stamp
    backup.mkdir(parents=True, mode=0o700)
    previous = 'custom-plex:before-' + stamp.lower()
    container = run(*compose, 'ps', '-q', 'app', capture_output=True, text=True).stdout.strip()
    old_image = run('sudo', '-n', 'docker', 'inspect', container, '--format', '{{.Image}}', capture_output=True, text=True).stdout.strip()
    run('sudo', '-n', 'docker', 'tag', old_image, previous)
    run('sudo', '-n', 'docker', 'load', '-i', staging + '/image.tar')
    # Test actual writes as the container user before stopping the running server.
    try:
        run('sudo', '-n', 'docker', 'run', '--rm', '--user', '10001:10001',
            '--entrypoint', 'sh', '--mount', f'type=bind,source={photo_root},target=/photos',
            release, '-c', 'probe=$(mktemp /photos/.write-test.XXXXXXXX) && rm "$probe"')
    except subprocess.CalledProcessError as error:
        raise RuntimeError(f'Photo storage {photo_root} is not writable by UID/GID 10001. '
                           'The running server has not been stopped. See README: Prepare the Pi photo drive. '
                           'For exFAT, configure mount ownership and masks; chmod/chown will not fix it.') from error
    # Back up private configuration separately; never overwrite it from the Mac.
    for name in ['.env', 'compose.yaml', 'compose.override.yaml', 'compose.override.yml']:
        if Path(name).exists():
            run('sudo', '-n', 'cp', '-p', name, str(backup / name))
    run(*compose, 'stop', 'app')
    try:
        run('sudo', '-n', 'tar', '-czf', str(backup / 'data.tar.gz'), '-C', data, '.')
        run('sudo', '-n', 'chmod', '600', str(backup / 'data.tar.gz'))
        run('tar', '-xzf', staging + '/source.tar.gz', '-C', directory)
        if configure_photos:
            with open('.env', 'a') as env_file:
                env_file.write('\n# Photo Album originals and browsing images on the external drive.\nHOST_PHOTO_DIR=' + photo_root + '\n')
        run('sudo', '-n', 'docker', 'tag', release, image)
        run(*compose, 'up', '-d', '--no-build', '--pull', 'never', '--wait', '--wait-timeout', '180')
    except BaseException:
        print(f'Deployment failed. Backup: {backup}; previous image: {previous}', flush=True)
        print('Restarting the previous image with its Compose configuration.', flush=True)
        run('sudo', '-n', 'docker', 'tag', previous, image)
        run('sudo', '-n', 'cp', str(backup / 'compose.yaml'), 'compose.yaml')
        run('sudo', '-n', 'cp', str(backup / '.env'), '.env')
        run(*compose, 'up', '-d', '--no-build', '--pull', 'never', '--wait', '--wait-timeout', '180')
        raise
    run(*compose, 'ps')
    print(f'Photo storage: {photo_root}/photo-albums (back up separately from the database).')
    print(f'Deployed successfully. Password and media mounts retained.\nBackup: {backup}\nPrevious image: {previous}')


if __name__ == '__main__':
    main()
