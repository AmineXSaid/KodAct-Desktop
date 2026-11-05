#!/usr/bin/env python3
"""A scripted model for checking question cards with the real KodAct CLI: it
asks two questions (AskUserQuestion) and logs the tool result it gets back,
which must read "The user answered: ...", then says ok.

    python3 scripts/e2e/ask_model.py <port> <log>

Point an endpoint at http://127.0.0.1:<port> (wire "anthropic"), as
real_kodact.sh does, ask anything, and answer the card."""
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
LOG = open(sys.argv[2], "a")
Q = {"questions": [
  {"question": "Which stack?", "header": "Stack", "multiSelect": False, "options": [
     {"label": "Tauri (Recommended)", "description": "Rust core, web UI"}, {"label": "Leptos", "description": "All Rust"}]},
  {"question": "Which targets?", "header": "Targets", "multiSelect": True, "options": [
     {"label": "Windows", "description": ""}, {"label": "Linux", "description": ""}, {"label": "macOS", "description": ""}]}]}
def sse(w, e, d): w.write(f"event: {e}\ndata: {json.dumps(d)}\n\n".encode()); w.flush()
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        o = json.dumps({"data": [{"id": "mock-model", "type": "model", "display_name": "Mock"}], "has_more": False}).encode()
        self.send_response(200); self.end_headers(); self.wfile.write(o)
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))) or b"{}")
        if self.path.endswith("count_tokens"):
            o = json.dumps({"input_tokens": 100}).encode(); self.send_response(200); self.end_headers(); self.wfile.write(o); return
        msgs = body.get("messages", []); tools = body.get("tools", [])
        results = [c for m in msgs if isinstance(m["content"], list) for c in m["content"] if c.get("type") == "tool_result"]
        if tools and results:
            r = results[-1]; cc = r.get("content")
            text = cc if isinstance(cc, str) else " ".join(x.get("text", "") for x in (cc or []) if x.get("type") == "text")
            LOG.write(json.dumps({"tool_result": text, "is_error": r.get("is_error", False)}) + "\n"); LOG.flush()
        ask = bool(tools) and not results
        msg = {"id": "m", "type": "message", "role": "assistant", "model": "mock", "content": [], "stop_reason": None, "usage": {"input_tokens": 10, "output_tokens": 1}}
        if not body.get("stream"):
            msg.update(content=[{"type": "text", "text": "Title"}], stop_reason="end_turn"); o = json.dumps(msg).encode()
            self.send_response(200); self.end_headers(); self.wfile.write(o); return
        self.send_response(200); self.send_header("content-type", "text/event-stream"); self.end_headers()
        sse(self.wfile, "message_start", {"type": "message_start", "message": msg})
        if ask:
            sse(self.wfile, "content_block_start", {"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "toolu_ask1", "name": "AskUserQuestion", "input": {}}})
            sse(self.wfile, "content_block_delta", {"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": json.dumps(Q)}})
            stop = "tool_use"
        else:
            sse(self.wfile, "content_block_start", {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}})
            sse(self.wfile, "content_block_delta", {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "ok" if tools else "Title"}})
            stop = "end_turn"
        sse(self.wfile, "content_block_stop", {"type": "content_block_stop", "index": 0})
        sse(self.wfile, "message_delta", {"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": None}, "usage": {"output_tokens": 5}})
        sse(self.wfile, "message_stop", {"type": "message_stop"})
ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
