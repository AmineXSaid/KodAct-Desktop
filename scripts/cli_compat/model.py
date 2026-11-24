"""Scripted OpenAI model: TodoWrite, then Write notes.md (streamed args), then text."""
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
def chunk(delta, finish=None):
    return "data: " + json.dumps({"id":"c","object":"chat.completion.chunk","model":"m","choices":[{"index":0,"delta":delta,"finish_reason":finish}]}) + "\n\n"
def call(cid, name, args, pieces=1):
    s = json.dumps(args); n = max(1, len(s)//pieces)
    out = chunk({"role":"assistant"}) + chunk({"tool_calls":[{"index":0,"id":cid,"type":"function","function":{"name":name,"arguments":""}}]})
    for i in range(0, len(s), n):
        out += chunk({"tool_calls":[{"index":0,"function":{"arguments":s[i:i+n]}}]})
    return out + chunk({}, "tool_calls")
def say(text):
    return chunk({"role":"assistant"}) + "".join(chunk({"content":w+" "}) for w in text.split()) + chunk({}, "stop")
class H(BaseHTTPRequestHandler):
    def log_message(self,*a): pass
    def do_GET(self):
        b=json.dumps({"data":[{"id":"m"}]}).encode(); self.send_response(200); self.end_headers(); self.wfile.write(b)
    def do_POST(self):
        req=json.loads(self.rfile.read(int(self.headers.get("content-length",0))))
        done={m["tool_call_id"] for m in req["messages"] if m["role"]=="tool"}
        if "c1" not in done:
            out=call("c1","TodoWrite",{"todos":[{"content":"Write the notes","status":"in_progress","activeForm":"Writing the notes"},{"content":"Check them","status":"pending","activeForm":"Checking them"}]})
        elif "c2" not in done:
            out=call("c2","Write",{"file_path":"notes.md","content":"# Notes\n\nKodAct and the desktop app speak stream-json.\n\n- one\n- two\n"},pieces=6)
        else:
            out=say("I wrote notes.md with the two points.")
        self.send_response(200); self.send_header("content-type","text/event-stream"); self.end_headers()
        self.wfile.write((out+"data: [DONE]\n\n").encode())
ThreadingHTTPServer(("127.0.0.1",int(sys.argv[1])),H).serve_forever()
