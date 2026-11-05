#!/usr/bin/env python3
"""A scripted model on the Anthropic Messages API (streaming), for driving
the real KodAct CLI end to end. The main agent starts a sub-agent (Task) that
tests a form with KodAct's browser tools, then opens and switches tabs and
runs JavaScript itself. Each step is decided from the conversation so far."""
import json, sys, re
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOG = open(sys.argv[2] if len(sys.argv) > 2 else "/tmp/mock.log", "a")
URL = "http://127.0.0.1:8765/index.html"
B = "mcp__kodact-browser__"

def results(msgs):
    """The tool results so far, in order: (tool_use name, text)."""
    names, out = {}, []
    for m in msgs:
        for c in (m["content"] if isinstance(m["content"], list) else []):
            if c.get("type") == "tool_use":
                names[c["id"]] = c["name"]
            if c.get("type") == "tool_result":
                cont = c.get("content")
                t = cont if isinstance(cont, str) else " ".join(x.get("text", "") for x in (cont or []) if x.get("type") == "text")
                out.append((names.get(c["tool_use_id"], "?"), t))
    return out

def decide(body):
    tools = [t["name"] for t in body.get("tools", [])]
    msgs = body["messages"]
    res = results(msgs)
    is_sub = "Task" not in tools
    if is_sub:
        n = len(res)
        if n == 0: return ("tool", B + "navigate", {"url": URL})
        if n == 1: return ("tool", B + "state", {})
        if n == 2:
            st = json.loads(res[1][1]) if res[1][1].strip().startswith("{") else {"elements": []}
            idx = next((e["index"] for e in st["elements"] if e["tag"] == "input"), 1)
            return ("tool", B + "type", {"index": idx, "text": "KodAct", "clear": True})
        if n == 3:
            st = json.loads(res[1][1]) if res[1][1].strip().startswith("{") else {"elements": []}
            idx = next((e["index"] for e in st["elements"] if e["tag"] == "button"), 2)
            return ("tool", B + "click", {"index": idx})
        if n == 4: return ("tool", B + "text", {"selector": "#out"})
        if n == 5: return ("tool", B + "screenshot", {})
        return ("text", "The form works: after typing KodAct and pressing Greet, the page says \"" + res[4][1].strip() + "\".", None)
    # The main agent.
    names = [r[0] for r in res]
    if not names:
        return ("tool", "Task", {"subagent_type": "general-purpose", "description": "Test the greeting form",
                                 "prompt": "Open " + URL + ", type KodAct into the name field, press Greet, and report what the page says."})
    if names[-1] == "Task": return ("tool", B + "tabs", {})
    if names[-1] == B + "tabs" and B + "new_tab" not in names:
        return ("tool", B + "new_tab", {"url": "http://127.0.0.1:8765/page2.html"})
    if names[-1] == B + "new_tab":
        tabs = json.loads(res[names.index(B + "tabs")][1])
        first = next(t["id"] for t in tabs if t["current"])
        return ("tool", B + "switch_tab", {"id": first})
    if names[-1] == B + "switch_tab": return ("tool", B + "eval", {"expression": "document.getElementById('out').textContent"})
    return ("text", "Checked: the sub-agent's test passed (the page greets KodAct), a second tab is open, and the first tab still says \"" + res[-1][1].strip() + "\".", None)

def sse(w, event, data):
    w.write(f"event: {event}\ndata: {json.dumps(data)}\n\n".encode()); w.flush()

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        out = json.dumps({"data": [{"id": "mock-model", "type": "model", "display_name": "Mock"}], "has_more": False}).encode()
        self.send_response(200); self.send_header("content-type", "application/json"); self.end_headers(); self.wfile.write(out)
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))) or b"{}")
        if self.path.endswith("count_tokens"):
            out = json.dumps({"input_tokens": 1000}).encode()
            self.send_response(200); self.send_header("content-type", "application/json"); self.end_headers(); self.wfile.write(out); return
        kind, a, b = decide(body) if body.get("messages") else ("text", "ok", None)
        LOG.write(json.dumps({"sub": "Task" not in [t["name"] for t in body.get("tools", [])], "tools": len(body.get("tools", [])), "reply": [kind, a, b]})[:400] + "\n"); LOG.flush()
        msg = {"id": "msg_1", "type": "message", "role": "assistant", "model": body.get("model", "mock"), "content": [], "stop_reason": None, "usage": {"input_tokens": 100, "output_tokens": 1}}
        if kind == "tool":
            block = {"type": "tool_use", "id": f"toolu_{abs(hash(json.dumps(body['messages'])))%10**12}", "name": a, "input": {}}
            delta = {"type": "input_json_delta", "partial_json": json.dumps(b)}
            stop = "tool_use"
        else:
            block = {"type": "text", "text": ""}
            delta = {"type": "text_delta", "text": a}
            stop = "end_turn"
        if not body.get("stream"):
            full = dict(block); 
            if kind == "tool": full["input"] = b
            else: full["text"] = a
            msg.update(content=[full], stop_reason=stop)
            out = json.dumps(msg).encode()
            self.send_response(200); self.send_header("content-type", "application/json"); self.end_headers(); self.wfile.write(out); return
        self.send_response(200); self.send_header("content-type", "text/event-stream"); self.end_headers()
        sse(self.wfile, "message_start", {"type": "message_start", "message": msg})
        sse(self.wfile, "content_block_start", {"type": "content_block_start", "index": 0, "content_block": block})
        sse(self.wfile, "content_block_delta", {"type": "content_block_delta", "index": 0, "delta": delta})
        sse(self.wfile, "content_block_stop", {"type": "content_block_stop", "index": 0})
        sse(self.wfile, "message_delta", {"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": None}, "usage": {"output_tokens": 20}})
        sse(self.wfile, "message_stop", {"type": "message_stop"})

ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
