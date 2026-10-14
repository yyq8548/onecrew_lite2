"""Offline shell integration tests: isolated HOME, mocked host/download/Mach-O header.

Run: uv run --no-project development/installers/test_installers.py
No network, real installation, Cargo, or service operation is performed.
"""
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).parent


class InstallerTests(unittest.TestCase):
    def run_case(self, *, arch='arm64', system='Darwin', translated='0',
                 bad_hash=False, bad_size=False, bad_header=False,
                 version='0.5.21', download_status='200'):
        with tempfile.TemporaryDirectory(prefix='onecrew-installer-test-') as tmp:
            root = Path(tmp)
            home = root / "home with ' quote"
            dest = home / '.local/bin'
            dest.mkdir(parents=True)
            installed = dest / 'onecrew'
            installed.write_bytes(b'existing-installation')
            payload = f'#!/bin/sh\nprintf "onecrew {version}\\n"\n'.encode()
            fixture = root / 'fixture'
            fixture.write_bytes(payload)
            mocks = root / 'mocks'
            mocks.mkdir()
            cpu = '0c000001' if arch == 'arm64' or translated == '1' else '07000001'
            header = '0000000000000000' if bad_header else 'cffaedfe' + cpu
            commands = {
                'uname': f'case "$1" in -s) echo {system};; -m) echo {arch};; esac',
                'sysctl': f'echo {translated}',
                'od': f'echo {header}',
                'stat': 'wc -c < "$3" | tr -d " "',
                'curl': r'''[ "$1" = -q ] || exit 80
case "$*" in *--max-redirs\ 0*) ;; *) exit 81;; esac
while [ "$#" -gt 0 ]; do
 if [ "$1" = --output ]; then shift; cp "$FIXTURE" "$1"; fi
 shift
done
printf '%s' "$DOWNLOAD_STATUS"
''',
            }
            for name, body in commands.items():
                command = mocks / name
                command.write_text('#!/bin/sh\n' + body + '\n')
                command.chmod(0o755)
            digest = '0' * 64 if bad_hash else hashlib.sha256(payload).hexdigest()
            size = len(payload) + int(bad_size)
            template = (ROOT / 'install.sh.txt').read_text().replace('__ONECREW_VERSION__', '0.5.21').replace('__ONECREW_BOOTSTRAP_UPDATE__', '0')
            for target in ('MAC_ARM', 'MAC_INTEL'):
                for key, value in {'URL': 'https://access.example/download?ticket=SECRET',
                                   'SHA256': digest, 'SIZE': str(size)}.items():
                    template = template.replace(f'__ONECREW_{target}_{key}__', value)
            script = root / 'install.sh'
            script.write_text(template)
            env = dict(os.environ, HOME=str(home), PATH=f'{mocks}:/usr/bin:/bin',
                       FIXTURE=str(fixture), DOWNLOAD_STATUS=download_status)
            result = subprocess.run(['/bin/sh', str(script)], env=env, capture_output=True, text=True)
            self.assertNotIn('SECRET', result.stdout + result.stderr)
            self.assertNotIn('https://', result.stdout + result.stderr)
            self.assertEqual(list(dest.glob('.onecrew-install.*')), [])
            success = (system == 'Darwin' and arch in ('arm64', 'x86_64')
                       and not (bad_hash or bad_size or bad_header)
                       and version == '0.5.21' and download_status == '200')
            self.assertEqual(result.returncode == 0, success, result.stderr)
            self.assertEqual(installed.read_bytes(), payload if success else b'existing-installation')
            if success:
                self.assertEqual(installed.stat().st_mode & 0o777, 0o755)
                self.assertEqual(len(result.stdout.splitlines()), 5)
                self.assertIn(' login --json', result.stdout)
                command = result.stdout.splitlines()[2].removeprefix('next_action="')[:-1]
                executed = subprocess.run(['/bin/sh', '-c', command], capture_output=True, text=True)
                self.assertEqual(executed.returncode, 0, command)

    def test_arm(self): self.run_case()
    def test_intel(self): self.run_case(arch='x86_64')
    def test_rosetta(self): self.run_case(arch='x86_64', translated='1')
    def test_unsupported_os(self): self.run_case(system='Linux')
    def test_unsupported_arch(self): self.run_case(arch='i386')
    def test_hash_preserves_existing(self): self.run_case(bad_hash=True)
    def test_size_preserves_existing(self): self.run_case(bad_size=True)
    def test_header_preserves_existing(self): self.run_case(bad_header=True)
    def test_version_preserves_existing(self): self.run_case(version='0.5.20')
    def test_redirect_preserves_existing(self): self.run_case(download_status='302')
    def test_error_body_preserves_existing(self): self.run_case(download_status='403')

    def test_powershell_static_contract(self):
        # Static guards only: this does not parse or execute PowerShell.
        script = (ROOT / 'install.ps1.txt').read_text()
        for marker in ('VERSION', 'WINDOWS_URL', 'WINDOWS_SHA256', 'WINDOWS_SIZE'):
            self.assertEqual(script.count(f'__ONECREW_{marker}__'), 1)
        for required in ('-MaximumRedirection 0', 'OSArchitecture',
                         '[Environment]::Is64BitOperatingSystem', '0x8664',
                         'Get-FileHash', '[IO.File]::Replace(',
                         '$reported[0] -cne "onecrew $version"'):
            self.assertIn(required, script)
        for forbidden in ('Stop-Process', 'Start-Service', 'SetEnvironmentVariable',
                          'Write-Error $_', 'Write-Output $url'):
            self.assertNotIn(forbidden, script)


if __name__ == '__main__':
    unittest.main()
