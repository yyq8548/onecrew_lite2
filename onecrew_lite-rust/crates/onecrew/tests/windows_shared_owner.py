"""Exercise two restrictive MCP Jobs against an isolated shared service.

uv run python windows_shared_owner.py EXE [--prestarted]
Without --prestarted, closing the first Job exposes the known lifecycle defect.
With --prestarted, normal CLI startup must retain the owner across both Jobs.
"""
import argparse
import ctypes as c
import ctypes.wintypes as w
import json
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time


class Basic(c.Structure):
    _fields_ = [
        ("process_time", c.c_longlong), ("job_time", c.c_longlong),
        ("flags", w.DWORD), ("min_working_set", c.c_size_t),
        ("max_working_set", c.c_size_t), ("active_processes", w.DWORD),
        ("affinity", c.c_size_t), ("priority", w.DWORD), ("scheduling", w.DWORD),
    ]


class Extended(c.Structure):
    _fields_ = [
        ("basic", Basic), ("io", c.c_ulonglong * 6),
        ("process_memory", c.c_size_t), ("job_memory", c.c_size_t),
        ("peak_process_memory", c.c_size_t), ("peak_job_memory", c.c_size_t),
    ]


kernel = c.WinDLL("kernel32", use_last_error=True)
kernel.CreateJobObjectW.argtypes = [c.c_void_p, w.LPCWSTR]
kernel.CreateJobObjectW.restype = w.HANDLE
kernel.SetInformationJobObject.argtypes = [w.HANDLE, c.c_int, c.c_void_p, w.DWORD]
kernel.AssignProcessToJobObject.argtypes = [w.HANDLE, w.HANDLE]
kernel.CloseHandle.argtypes = [w.HANDLE]
native = c.WinDLL("ntdll")
native.NtResumeProcess.argtypes = [w.HANDLE]
native.NtResumeProcess.restype = c.c_long


class Client:
    def __init__(self, exe, root, index):
        self.job = kernel.CreateJobObjectW(None, None)
        assert self.job, c.get_last_error()
        limits = Extended()
        limits.basic.flags = 0x2000  # KILL_ON_JOB_CLOSE; breakaway forbidden.
        assert kernel.SetInformationJobObject(
            self.job, 9, c.byref(limits), c.sizeof(limits)
        ), c.get_last_error()
        self.errors = (root / f"client-{index}.stderr").open("w", encoding="utf-8")
        self.process = subprocess.Popen(
            [exe, "mcp", "--stdio", "--data-dir", str(root)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.errors,
            text=True, encoding="utf-8", creationflags=0x4 | 0x08000000,
        )
        assert kernel.AssignProcessToJobObject(
            self.job, int(self.process._handle)
        ), c.get_last_error()
        assert native.NtResumeProcess(int(self.process._handle)) == 0
        self.lines = queue.Queue()
        threading.Thread(target=self.read_output, daemon=True).start()

    def read_output(self):
        for line in self.process.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def send(self, message):
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def request(self, method, request_id, params=None):
        message = {"jsonrpc": "2.0", "id": request_id, "method": method}
        if params is not None:
            message["params"] = params
        self.send(message)
        for _ in range(20):
            line = self.lines.get(timeout=30)
            assert line, f"MCP transport closed while waiting for {method}"
            response = json.loads(line)
            if response.get("id") == request_id:
                assert "result" in response, response
                return
        raise AssertionError("No matching MCP response within bounded messages")

    def initialize(self):
        self.request("initialize", 1, {
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "shared-owner-regression", "version": "1"},
        })
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        self.request("ping", 2)

    def close(self):
        if self.job:
            kernel.CloseHandle(self.job)
            self.job = None
            self.process.wait(timeout=10)
            self.errors.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("exe")
    parser.add_argument("--prestarted", action="store_true")
    parser.add_argument("--managed-host", action="store_true")
    args = parser.parse_args()
    exe = str(Path(args.exe).resolve())
    root = Path(tempfile.mkdtemp(prefix="onecrew-shared-owner-"))
    clients = []
    started = time.monotonic()
    def mark(stage):
        print(json.dumps({"stage": stage, "elapsed_seconds": round(time.monotonic()-started, 3)}), flush=True)
    try:
        if args.managed_host:
            subprocess.run([exe, "service", "host", "install", "--data-dir", str(root)],
                           check=True, timeout=90)
            mark("host_registered")
        if args.prestarted:
            # Files avoid waiting on pipe EOF from an independently running child.
            with (root / "startup.log").open("w", encoding="utf-8") as log:
                subprocess.run([exe, "web", "--no-browser", "--data-dir", str(root)],
                               stdout=log, stderr=log, check=True, timeout=40)
            assert "service_host_lifetime" not in (root / "startup.log").read_text()
        for index in range(2):
            client = Client(exe, root, index)
            clients.append(client)
            client.initialize()
            mark(f"client_{index}_initialized")
        owner_pid = json.loads((root / ".onecrew_lite-service.json").read_text())["pid"]
        clients[0].close()
        clients[1].request("ping", 3)
        clients[1].close()
        with (root / "status.json").open("w", encoding="utf-8") as output:
            subprocess.run([exe, "status", "--json", "--data-dir", str(root)],
                           stdout=output, stderr=subprocess.DEVNULL, check=True, timeout=30)
        status = json.loads((root / "status.json").read_text())
        assert status["running"] and status["pid"] == owner_pid
        print(json.dumps({"prestarted": args.prestarted,
                          "managed_host": args.managed_host,
                          "second_client_ping_after_first_job_closed": True,
                          "same_owner_after_both_jobs_closed": True}))
    finally:
        for client in clients:
            client.close()
        subprocess.run([exe, "stop", "--data-dir", str(root)],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
        if args.managed_host:
            subprocess.run([exe, "service", "host", "remove", "--data-dir", str(root)],
                           check=True, timeout=30)


if __name__ == "__main__":
    main()
