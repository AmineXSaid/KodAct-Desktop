"""A small Markdown reader shared by the document tools.

It knows what a document needs and no more: headings (#, ##, ###),
paragraphs, bullet and numbered lists, tables, fenced code, and inline
**bold**, *italic* and `code`. Each block comes back as a tuple the tools
turn into Word paragraphs, slides or PDF flowables.
"""

import re

INLINE = re.compile(r"(\*\*[^*]+\*\*|\*[^*]+\*|`[^`]+`)")


def inline(text):
    """[(text, bold, italic, code)] for one line of Markdown."""
    out = []
    for part in INLINE.split(text):
        if not part:
            continue
        if part.startswith("**") and part.endswith("**") and len(part) > 4:
            out.append((part[2:-2], True, False, False))
        elif part.startswith("`") and part.endswith("`") and len(part) > 2:
            out.append((part[1:-1], False, False, True))
        elif part.startswith("*") and part.endswith("*") and len(part) > 2:
            out.append((part[1:-1], False, True, False))
        else:
            out.append((part, False, False, False))
    return out


def plain(text):
    return "".join(t for t, *_ in inline(text))


def _cells(line):
    return [c.strip() for c in line.strip().strip("|").split("|")]


def blocks(markdown):
    """Yield ("heading", level, text), ("para", text), ("bullet", text),
    ("number", text), ("table", [header], [[row]]) and ("code", text)."""
    lines = markdown.splitlines()
    i, para = 0, []

    def flush():
        if para:
            yield ("para", " ".join(s.strip() for s in para))
            para.clear()

    while i < len(lines):
        line = lines[i]
        s = line.strip()
        if s.startswith("```"):
            yield from flush()
            code = []
            i += 1
            while i < len(lines) and not lines[i].strip().startswith("```"):
                code.append(lines[i])
                i += 1
            yield ("code", "\n".join(code))
        elif not s:
            yield from flush()
        elif m := re.match(r"^(#{1,3})\s+(.*)$", s):
            yield from flush()
            yield ("heading", len(m.group(1)), m.group(2).strip())
        elif s.startswith("|") and i + 1 < len(lines) and re.match(r"^\|?\s*:?-{2,}", lines[i + 1].strip()):
            yield from flush()
            header = _cells(s)
            rows = []
            i += 2
            while i < len(lines) and lines[i].strip().startswith("|"):
                rows.append(_cells(lines[i]))
                i += 1
            yield ("table", header, rows)
            continue
        elif m := re.match(r"^[-*]\s+(.*)$", s):
            yield from flush()
            yield ("bullet", m.group(1))
        elif m := re.match(r"^\d+[.)]\s+(.*)$", s):
            yield from flush()
            yield ("number", m.group(1))
        else:
            para.append(s)
        i += 1
    yield from flush()
