#!/usr/bin/env python3
"""A scripted model for `scripts/e2e/office_live.sh`: a deck built the way
the office-documents skill now says to, through the real KodAct CLI.

    python3 office_model.py PORT LOG PROJECT TOOLS_DIR

The turn it plays, each step decided from the tool results so far:
  1. `pptx_tool.py init deck.pptx --audience "kids aged 8"`: the real file
     first (Bash);
  2. pauses PAUSE seconds (so the test can see the card and the panel while
     the turn is still running), then adds a slide;
  3. adds two more slides in one call;
  4. renders the deck to `pages/` with `check_tool.py render` (no tool
     hunting: the app names the renderer);
  5. removes `pages/` with `rm -rf` (a removal: Auto asks for this one);
  6. ends with a line quoting the slide count it read back.
Every request and reply is logged as one JSON line.
"""
import json
import os
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT, LOG_PATH, PROJECT, TOOLS = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
PAUSE = float(os.environ.get("PAUSE", "8"))
LOG = open(LOG_PATH, "a")
PY = '"$KODACT_PYTHON"'
DECK = f"{PROJECT}/deck.pptx"
SLIDES = [
    {"layout": "cards", "title": "Your Body Is Made of Cells",
     "items": ["Billions of tiny parts", "Each has a job", "They work as a team"], "icon": "cell"},
    {"layout": "faces", "title": "How You Might Feel",
     "items": [{"label": "Worried", "mood": "worried"}, {"label": "Sad", "mood": "sad"}], "lines": ["All okay."]},
    {"layout": "closing", "title": "You Are Loved", "items": ["Ask anything."]},
]


def text_of(content):
    if isinstance(content, str):
        return content
    return " ".join(c.get("text", "") for c in content or [] if c.get("type") == "text")


def results(msgs):
    names, out = {}, []
    for m in msgs:
        for c in m["content"] if isinstance(m["content"], list) else []:
            if c.get("type") == "tool_use":
                names[c["id"]] = c["name"]
            if c.get("type") == "tool_result":
                out.append((names.get(c["tool_use_id"], "?"), text_of(c.get("content")), bool(c.get("is_error"))))
    return out


def bash(command, description):
    return ("tool", "Bash", {"command": command, "description": description})


def js(obj):
    # One shell word, in single quotes: the JSON has no single quotes.
    return "'" + json.dumps(obj) + "'"


def decide(body):
    tools = [t["name"] for t in body.get("tools", [])]
    if "Bash" not in tools:
        return [("text", "Cells deck")]
    res = results(body["messages"])
    n = len(res)
    if n == 0:
        return [("text", "I'll make the deck first, then fill it slide by slide."),
                bash(f'{PY} "{TOOLS}/pptx_tool.py" init "{DECK}" --audience "kids aged 8" --title "What Is Cancer?"',
                     "Make the deck")]
    if n == 1:
        time.sleep(PAUSE)
        return [bash(f'{PY} "{TOOLS}/pptx_tool.py" add "{DECK}" --json {js(SLIDES[0])}', "Add the cells slide")]
    if n == 2:
        return [bash(f'{PY} "{TOOLS}/pptx_tool.py" add "{DECK}" --json {js(SLIDES[1:])}', "Add two slides")]
    if n == 3:
        return [bash(f'{PY} "{TOOLS}/check_tool.py" render "{DECK}" "{PROJECT}/pages" --dpi 40',
                     "Look at the slides")]
    if n == 4:
        return [bash(f'rm -rf "{PROJECT}/pages"', "Remove the page images")]
    read = res[2][1]
    return [("text", "Done: the deck has 4 slides." if "up to 4" in read else "The deck did not grow: " + read[:200])]


def sse(w, event, data):
    w.write(f"event: {event}\ndata: {json.dumps(data)}\n\n".encode())
    w.flush()


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def send_json(self, obj):
        out = json.dumps(obj).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.end_headers()
        self.wfile.write(out)

    def do_GET(self):
        self.send_json({"data": [{"id": "mock-model", "type": "model", "display_name": "Mock"}], "has_more": False})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))) or b"{}")
        if self.path.endswith("count_tokens"):
            return self.send_json({"input_tokens": 1000})
        blocks = decide(body) if body.get("messages") else [("text", "ok")]
        msgs = body.get("messages", [])
        LOG.write(json.dumps({
            "results": [[n, t[:300], e] for n, t, e in results(msgs)],
            "reply": [list(b) for b in blocks],
        }) + "\n")
        LOG.flush()
        stop = "tool_use" if any(b[0] == "tool" for b in blocks) else "end_turn"
        msg = {"id": "msg_1", "type": "message", "role": "assistant", "model": body.get("model", "mock"),
               "content": [], "stop_reason": None, "usage": {"input_tokens": 100, "output_tokens": 1}}
        full = []
        for i, b in enumerate(blocks):
            if b[0] == "tool":
                full.append({"type": "tool_use", "id": f"toolu_{len(msgs):04d}{i}", "name": b[1], "input": b[2]})
            else:
                full.append({"type": "text", "text": b[1]})
        if not body.get("stream"):
            msg.update(content=full, stop_reason=stop)
            return self.send_json(msg)
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        sse(self.wfile, "message_start", {"type": "message_start", "message": msg})
        for i, block in enumerate(full):
            if block["type"] == "tool_use":
                start = dict(block, input={})
                delta = {"type": "input_json_delta", "partial_json": json.dumps(block["input"])}
            else:
                start = {"type": "text", "text": ""}
                delta = {"type": "text_delta", "text": block["text"]}
            sse(self.wfile, "content_block_start", {"type": "content_block_start", "index": i, "content_block": start})
            sse(self.wfile, "content_block_delta", {"type": "content_block_delta", "index": i, "delta": delta})
            sse(self.wfile, "content_block_stop", {"type": "content_block_stop", "index": i})
        sse(self.wfile, "message_delta", {"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": None},
                                          "usage": {"output_tokens": 20}})
        sse(self.wfile, "message_stop", {"type": "message_stop"})


ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
