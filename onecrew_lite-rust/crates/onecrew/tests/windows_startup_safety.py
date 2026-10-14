"""Isolated native checks: diagnosis cannot spawn, restricted Jobs fail before startup."""
import ctypes as c
from pathlib import Path
import subprocess
import sys
import tempfile

from windows_shared_owner import kernel, native, Extended


exe = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="onecrew-startup-safety-") as directory:
    root = Path(directory)
    data = root / "absent-data"
    status = subprocess.run([exe, "status", "--data-dir", str(data)],
                            capture_output=True, text=True, timeout=15)
    assert status.returncode != 0 and "service_not_running" in status.stderr, status.stderr
    assert not data.exists(), "status created application data or started an owner"
    for args in [["__service-preflight"], ["web", "--no-browser", "--data-dir", str(data)]]:
        job = kernel.CreateJobObjectW(None, None)
        assert job
        limits = Extended()
        limits.basic.flags = 0x2000
        assert kernel.SetInformationJobObject(job, 9, c.byref(limits), c.sizeof(limits))
        child = None
        try:
            child = subprocess.Popen([exe, *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     text=True, creationflags=0x4 | 0x08000000)
            assert kernel.AssignProcessToJobObject(job, int(child._handle))
            assert native.NtResumeProcess(int(child._handle)) == 0
            out, err = child.communicate(timeout=90)
            if args == ["__service-preflight"]:
                assert child.returncode == 0, (out, err)
            else:
                assert child.returncode != 0 and "service_host_start_failed" in err, (out, err)
            assert not (data / ".onecrew_lite-service.json").exists(), "unsafe owner was published"
        finally:
            kernel.CloseHandle(job)
            if child is not None:
                child.wait(timeout=10)
print("PASS: read-only status; restricted host preflight; missing registration never starts an unsafe owner")
