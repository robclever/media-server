"""Check deployment sequencing without connecting to a server."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import deploy_pi_remote


class DeploymentTests(unittest.TestCase):
    def exercise(self, fail_start=False):
        calls = []
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp) / 'app'
            directory.mkdir()
            (directory / '.env').write_text('existing configuration')
            (directory / 'compose.yaml').write_text('existing compose')
            data = Path(tmp) / 'external-data'
            data.mkdir()
            (data / 'library.sqlite3').touch()
            config = {'services': {'app': {'image': 'custom-plex:local', 'volumes': [{'source': str(data), 'target': '/data', 'type': 'bind'}]}}}

            def run(*command, **kwargs):
                calls.append(command)
                if 'config' in command:
                    return subprocess.CompletedProcess(command, 0, json.dumps(config))
                if 'inspect' in command:
                    return subprocess.CompletedProcess(command, 0, 'sha256:previous')
                if 'ps' in command:
                    return subprocess.CompletedProcess(command, 0, 'container')
                if 'up' in command and fail_start and sum('up' in c for c in calls) == 1:
                    raise subprocess.CalledProcessError(1, command)
                return subprocess.CompletedProcess(command, 0, '')

            cwd = os.getcwd()
            try:
                with patch.object(deploy_pi_remote, 'run', side_effect=run), patch('sys.argv', ['remote', str(directory), '/tmp/staging', 'custom-plex:release']), patch.object(Path, 'is_file', side_effect=PermissionError('Private database directory')):
                    if fail_start:
                        with self.assertRaises(subprocess.CalledProcessError):
                            deploy_pi_remote.main()
                    else:
                        deploy_pi_remote.main()
            finally:
                os.chdir(cwd)
            self.assertIn(('sudo', '-n', 'test', '-f', str(data / 'library.sqlite3')), calls)
            backup = next(c for c in calls if 'tar' in c)
            self.assertIn(str(data), backup)
            self.assertFalse(any('set-password' in c for c in calls))
            self.assertLess(next(i for i,c in enumerate(calls) if 'stop' in c), calls.index(backup))
        return calls

    def test_preserves_external_database_mount(self):
        calls = self.exercise()
        self.assertEqual(sum('up' in c for c in calls), 1)

    def test_attempts_previous_image_after_failed_start(self):
        calls = self.exercise(True)
        self.assertEqual(sum('up' in c for c in calls), 2)
        self.assertTrue(any('tag' in c and any(v.startswith('custom-plex:before-') for v in c) and c[-1] == 'custom-plex:local' for c in calls))


if __name__ == '__main__':
    unittest.main()
