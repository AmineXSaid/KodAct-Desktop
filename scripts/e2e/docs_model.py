#!/usr/bin/env python3
"""A scripted model for `scripts/e2e/office_docs.sh`: a Word report, then an
Excel workbook, each made the way the office-documents skill says (`init`
first, then `add`), through the real KodAct CLI.

    python3 docs_model.py PORT LOG PROJECT TOOLS_DIR
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT, LOG_PATH, PROJECT, TOOLS = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
LOG = open(LOG_PATH, "a")
PY = '"$KODACT_PYTHON"'
DOCX, XLSX = f"{PROJECT}/report.docx", f"{PROJECT}/budget.xlsx"
WORD = [
    {"block": "heading", "text": "Summary"},
    {"block": "text", "text": "Revenue grew **23%** on last year.", "lead": True},
    {"block": "kpis", "items": [{"value": "185k", "label": "Revenue", "delta": "+23%"},
                                {"value": "2.1%", "label": "Churn", "delta": "-0.4%", "better": "down"}]},
    {"block": "callout", "kind": "warning", "title": "Risk", "text": "Licence costs rise 12% in January."},
    {"block": "cards", "items": [{"icon": "rocket", "head": "Launch", "text": "Windows app in December"},
                                 {"icon": "users", "head": "Hiring", "text": "Two engineers"}]},
    {"block": "chart", "chart": {"type": "column", "categories": ["Q1", "Q2", "Q3"], "series": {"Revenue": [140, 160, 185]}}},
]
EXCEL = [
    {"block": "table", "heading": "Costs", "name": "Costs", "columns": ["Item", "Q2", "Q3", "Change"],
     "rows": [["Servers", 1200, 1350], ["Licences", 800, 640], ["Travel", 400, 520]],
     "formulas": {"Change": "=[@Q3]-[@Q2]"}, "formats": {"Q2": "eur", "Q3": "eur", "Change": "eur"},
     "totals": {"Q2": "sum", "Q3": "sum", "Change": "sum"}, "bars": ["Q3"], "icons": ["Change"]},
    {"block": "kpis", "items": [{"label": "Q3 total", "formula": "=SUM(D8:D10)", "format": "eur"}]},
    {"block": "chart", "type": "column", "data": "C7:D10", "categories": "B8:B10", "title": "Q2 vs Q3", "at": "G6"},
]


def text_of(content):
    if isinstance(content, str):
        return content
    return " ".join(c.get("text", "") for c in content or [] if c.get("type") == "text")


def turn(msgs):
    """The person's latest message, and the tool results since it."""
    last, since = "", []
    names = {}
    for m in msgs:
        c = m["content"]
        if m["role"] == "user" and (isinstance(c, str) or not any(x.get("type") == "tool_result" for x in c)):
            last, since = text_of(c), []
        for x in c if isinstance(c, list) else []:
            if x.get("type") == "tool_use":
                names[x["id"]] = x["name"]
            if x.get("type") == "tool_result":
                since.append((names.get(x["tool_use_id"], "?"), text_of(x.get("content")), bool(x.get("is_error"))))
    return last, since


def results(msgs):
    return turn(msgs)[1]


def bash(command, description):
    return ("tool", "Bash", {"command": command, "description": description})


def js(obj):
    return "'" + json.dumps(obj) + "'"


def decide(body):
    tools = [t["name"] for t in body.get("tools", [])]
    if "Bash" not in tools:
        return [("text", "Q3 report")]
    asked, res = turn(body["messages"])
    n = len(res)
    if "word" in asked.lower():
        if n == 0:
            return [("text", "The report first, then its parts."),
                    bash(f'{PY} "{TOOLS}/docx_tool.py" init "{DOCX}" --audience "the board" --title "Q3 Review" --subtitle "Results and the plan"', "Make the report")]
        if n == 1:
            return [bash(f'{PY} "{TOOLS}/docx_tool.py" add "{DOCX}" --json {js(WORD)}', "Add its blocks")]
        return [("text", "Done: report.docx is ready." if not res[-1][2] else "Failed: " + res[-1][1][:200])]
    if "excel" in asked.lower():
        if n == 0:
            return [("text", "The workbook first, then its parts."),
                    bash(f'{PY} "{TOOLS}/xlsx_tool.py" init "{XLSX}" --audience "finance" --title "Q3 Budget" --subtitle "Costs by item"', "Make the workbook")]
        if n == 1:
            return [bash(f'{PY} "{TOOLS}/xlsx_tool.py" add "{XLSX}" --json {js(EXCEL)}', "Add the table, tile and chart")]
        if n == 2:
            return [bash(f'{PY} "{TOOLS}/xlsx_tool.py" recalc "{XLSX}" "{XLSX}"', "Compute the formulas")]
        return [("text", "Done: budget.xlsx is ready." if not res[-1][2] else "Failed: " + res[-1][1][:200])]
    return [("text", "ok")]

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
