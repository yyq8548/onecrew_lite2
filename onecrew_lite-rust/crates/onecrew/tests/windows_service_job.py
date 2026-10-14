"""Native Windows regression: MCP starts under permissive and restrictive Jobs.

Usage: uv run python windows_service_job.py PATH_TO_ONECREW_EXE [--restricted]
Uses isolated temporary data, never the installed user's profile.
An allowed breakaway must outlive its client; a restrictive Job must retain it.
"""
import ctypes as c, ctypes.wintypes as w, subprocess, tempfile, json, threading, queue, time, pathlib, sys
k=c.WinDLL('kernel32',use_last_error=True)
class BASIC(c.Structure):
 _fields_=[('PerProcessUserTimeLimit',c.c_longlong),('PerJobUserTimeLimit',c.c_longlong),('LimitFlags',w.DWORD),('MinimumWorkingSetSize',c.c_size_t),('MaximumWorkingSetSize',c.c_size_t),('ActiveProcessLimit',w.DWORD),('Affinity',c.c_size_t),('PriorityClass',w.DWORD),('SchedulingClass',w.DWORD)]
class IO(c.Structure):_fields_=[(s,c.c_ulonglong) for s in ['ReadOperationCount','WriteOperationCount','OtherOperationCount','ReadTransferCount','WriteTransferCount','OtherTransferCount']]
class EXT(c.Structure):_fields_=[('BasicLimitInformation',BASIC),('IoInfo',IO),('ProcessMemoryLimit',c.c_size_t),('JobMemoryLimit',c.c_size_t),('PeakProcessMemoryUsed',c.c_size_t),('PeakJobMemoryUsed',c.c_size_t)]
k.CreateJobObjectW.restype=w.HANDLE;k.SetInformationJobObject.argtypes=[w.HANDLE,c.c_int,c.c_void_p,w.DWORD];k.AssignProcessToJobObject.argtypes=[w.HANDLE,w.HANDLE];k.CloseHandle.argtypes=[w.HANDLE];k.OpenProcess.restype=w.HANDLE;k.GetExitCodeProcess.argtypes=[w.HANDLE,c.POINTER(w.DWORD)]
restricted = '--restricted' in sys.argv[2:]
h=k.CreateJobObjectW(None,None);limits=EXT();limits.BasicLimitInformation.LimitFlags=0x2000 | (0 if restricted else 0x800)
assert k.SetInformationJobObject(h,9,c.byref(limits),c.sizeof(limits)),c.get_last_error()
root=tempfile.mkdtemp(prefix='onecrew-job-probe-');exe=str(pathlib.Path(sys.argv[1]).resolve())
p=subprocess.Popen([exe,'mcp','--stdio','--data-dir',root],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8',creationflags=0x4|0x08000000)
assert k.AssignProcessToJobObject(h,int(p._handle)),c.get_last_error()
c.WinDLL('ntdll').NtResumeProcess(w.HANDLE(int(p._handle)))
p.stdin.write(json.dumps({'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2024-11-05','capabilities':{},'clientInfo':{'name':'job-probe','version':'1'}}})+'\n');p.stdin.flush()
q=queue.Queue();threading.Thread(target=lambda:q.put(p.stdout.readline()),daemon=True).start()
line=q.get(timeout=35)
if not line:
 p.wait(timeout=5)
 k.CloseHandle(h)
 raise AssertionError('MCP closed before initialize: '+p.stderr.read())
r=json.loads(line);assert 'result' in r,r
p.stdin.write(json.dumps({'jsonrpc':'2.0','method':'notifications/initialized'})+'\n');p.stdin.flush()
endpoint=json.loads((pathlib.Path(root)/'.onecrew_lite-service.json').read_text());pid=endpoint['pid']
sh=k.OpenProcess(0x1000,False,pid);assert sh
k.CloseHandle(h);p.wait(timeout=5);time.sleep(.3)
if restricted:
 stderr=p.stderr.read()
 assert 'service_host_lifetime' in stderr and 'os error 5' in stderr, stderr
code=w.DWORD();k.GetExitCodeProcess(sh,c.byref(code));k.CloseHandle(sh)
print(json.dumps({'restricted_job':restricted,'initialize_succeeded':True,'service_pid':pid,'service_survived_client_job_close':code.value==259,'service_exit_code':code.value}))
if code.value==259:
 try:
  status=subprocess.run([exe,'status','--json','--data-dir',root],capture_output=True,text=True,encoding='utf-8',timeout=20,check=True)
  receipt=json.loads(status.stdout)
  assert receipt['running'] and receipt['pid']==pid, 'Service did not retain its original live owner'
 finally:
  subprocess.run([exe,'stop','--data-dir',root],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=10)
if restricted:
 assert code.value!=259, 'Service must respect the restrictive host Job lifetime'
else:
 assert code.value==259, 'Shared service was terminated with the temporary MCP client job'
