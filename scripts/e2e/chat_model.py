#!/usr/bin/env python3
"""A scripted model on the Anthropic Messages API (streaming), for
`scripts/e2e/chat.sh`: one real chat, driven through the real KodAct CLI.

    python3 chat_model.py PORT LOG PROJECT TOOLS_DIR

The turn it plays, each step decided from the tool results so far:
  1. says what it will do, and writes `notes.md` (Write);
  2. writes `rows.csv` (Write);
  3. makes `budget.xlsx` from it with the office-documents skill's own
     `xlsx_tool.py`, run with the Python KodAct hands every session
     (`$KODACT_PYTHON`, Bash);
  4. reads the workbook back with the same tool (Bash);
  5. ends with a line quoting what it read.
A second message in the same chat ("And the total?") is answered from the
conversation, so the test sees a follow-up reach the same session.
Every request and reply is logged as one JSON line.
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT, LOG_PATH, PROJECT, TOOLS = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
LOG = open(LOG_PATH, "a")
NOTES = "# Budget\n\nRent and food, for the end-to-end test.\n"
ROWS = "Item,Amount\nRent,700\nFood,250\n"
DONE = "Done: budget.xlsx has Rent 700 and Food 250."
TOTAL = "The total is 950."


def text_of(content):
    if isinstance(content, str):
        return content
    return " ".join(c.get("text", "") for c in content or [] if c.get("type") == "text")


def results(msgs):
    """The tool results so far, in order: (tool name, text, is_error)."""
    names, out = {}, []
    for m in msgs:
        for c in m["content"] if isinstance(m["content"], list) else []:
            if c.get("type") == "tool_use":
                names[c["id"]] = c["name"]
            if c.get("type") == "tool_result":
                out.append((names.get(c["tool_use_id"], "?"), text_of(c.get("content")), bool(c.get("is_error"))))
    return out


def asked(msgs):
    """What the person typed, in order (tool results aside)."""
    out = []
    for m in msgs:
        if m["role"] != "user":
            continue
        c = m["content"]
        if isinstance(c, str):
            out.append(c)
        elif not any(x.get("type") == "tool_result" for x in c):
            out.append(text_of(c))
    return out


def decide(body):
    tools = [t["name"] for t in body.get("tools", [])]
    # Side requests (a title, a summary) carry no tools: answer plainly.
    if "Write" not in tools:
        return [("text", "Budget workbook")]
    msgs = body["messages"]
    res = results(msgs)
    if any("total" in q.lower() for q in asked(msgs)[1:]):
        return [("text", TOTAL)]
    n = len(res)
    py = '"$KODACT_PYTHON"'
    if n == 0:
        return [("text", "I'll note the plan, then build the workbook."),
                ("tool", "Write", {"file_path": f"{PROJECT}/notes.md", "content": NOTES})]
    if n == 1:
        return [("tool", "Write", {"file_path": f"{PROJECT}/rows.csv", "content": ROWS})]
    if n == 2:
        return [("tool", "Bash", {"command": f'{py} "{TOOLS}/xlsx_tool.py" create "{PROJECT}/budget.xlsx" --csv "{PROJECT}/rows.csv" --sheet Budget',
                                  "description": "Make the workbook"})]
    if n == 3:
        return [("tool", "Bash", {"command": f'{py} "{TOOLS}/xlsx_tool.py" read "{PROJECT}/budget.xlsx"',
                                  "description": "Read the workbook back"})]
    read = res[3][1]
    ok = "Rent" in read and "700" in read and "Food" in read and "250" in read
    return [("text", DONE if ok else "The workbook did not read back: " + read[:200])]


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
            "asked": asked(msgs),
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
