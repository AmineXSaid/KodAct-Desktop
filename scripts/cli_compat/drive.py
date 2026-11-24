"""Drive kodact as the desktop does: its flags, its messages, its answers."""
import json, subprocess, sys, os, time, uuid, threading
exe, work, out = sys.argv[1], sys.argv[2], sys.argv[3]
sid = str(uuid.uuid4())
args = [exe,"-p","--input-format","stream-json","--output-format","stream-json","--include-partial-messages",
        "--permission-prompt-tool","stdio","--model","m","--permission-mode","default",
        "--append-system-prompt","You are inside the KodAct desktop app.","--replay-user-messages","--session-id",sid]
env = dict(os.environ, KODACT_OPENAI_BASE_URL="http://127.0.0.1:8795/v1", KODACT_OPENAI_API_KEY="sk-test0123456789")
p = subprocess.Popen(args, cwd=work, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
lines = []
def send(v): p.stdin.write(json.dumps(v)+"\n"); p.stdin.flush()
send({"type":"user","message":{"role":"user","content":[{"type":"text","text":"write notes"}]},"parent_tool_use_id":None,"session_id":"","priority":"next","uuid":str(uuid.uuid4())})
results = 0; asked = []; t0=time.time()
for line in p.stdout:
    lines.append(line.rstrip("\n"))
    try: v = json.loads(line)
    except Exception: continue
    if v.get("type")=="control_request" and v["request"].get("subtype")=="can_use_tool":
        asked.append(v["request"]["tool_name"])
        send({"type":"control_response","response":{"subtype":"success","request_id":v["request_id"],"response":{"behavior":"allow","updatedInput":v["request"]["input"]}}})
    if v.get("type")=="result":
        results += 1
        if results == 1:
            # The desktop's control requests, then an immediate command.
            for i,(st,extra) in enumerate([("get_context_usage",{}),("set_model",{"model":"m"}),("set_permission_mode",{"mode":"acceptEdits"})]):
                send({"type":"control_request","request_id":f"forge-req-{i}","request":dict({"subtype":st},**extra)})
            send({"type":"user","message":{"role":"user","content":[{"type":"text","text":"/status"}]},"parent_tool_use_id":None,"session_id":""})
        elif results >= 2:
            time.sleep(1); p.stdin.close(); break
    if time.time()-t0>90: break
p.wait(timeout=30)
open(out,"w").write("\n".join(lines)+"\n")
print("asked:", asked, "results:", results, "lines:", len(lines), "exit:", p.returncode)
print("stderr:", p.stderr.read()[-800:])
