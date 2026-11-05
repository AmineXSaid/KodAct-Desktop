//! Open Design's own front-matter reader, for the few `SKILL.md` files that
//! are not valid YAML. Adapted from Open Design (Apache-2.0),
//! `apps/daemon/src/design-systems/frontmatter.ts` (`parseYamlSubset`), and
//! checked to give the same tree as the original on all 543 `SKILL.md`
//! files in the library.
//!
//! It is not YAML: it is what Open Design's daemon runs, so upstream files
//! written for it read the same here. Its quirks are kept on purpose: inline
//! `# comments` stay in values, quotes are stripped without unescaping, only
//! `|`, `|-`, `>` and `>-` start a block (and `>` does not fold), a bare
//! `key:` is an empty map, and a few malformed shapes are an error (Open
//! Design then drops the skill; KodAct keeps the file and reports it).

use serde_json::{Map, Number, Value};

enum Slot {
    Val(Value),
    Node(usize),
}
enum Node {
    Obj(Vec<(String, Slot)>),
    Arr(Vec<Slot>),
}
struct Frame {
    indent: i64,
    node: usize,
    key: Option<String>,
}

fn lead_ws(s: &str) -> usize {
    s.chars().take_while(|c| c.is_whitespace()).count()
}
fn blank_or_comment(s: &str) -> bool {
    let t = s.trim_start();
    t.is_empty() || t.starts_with('#')
}

fn set(nodes: &mut [Node], obj: usize, key: &str, v: Slot) -> Result<(), String> {
    match &mut nodes[obj] {
        Node::Obj(kv) => {
            if let Some(e) = kv.iter_mut().find(|(k, _)| k == key) {
                e.1 = v;
            } else {
                kv.push((key.to_string(), v));
            }
            Ok(())
        }
        Node::Arr(_) => Err("frontmatter object container expected".into()),
    }
}

pub fn parse(src: &str) -> Result<Map<String, Value>, String> {
    let lines: Vec<&str> = src
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut nodes = vec![Node::Obj(vec![])];
    let mut stack = vec![Frame {
        indent: -1,
        node: 0,
        key: None,
    }];
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        if blank_or_comment(raw) {
            i += 1;
            continue;
        }
        let indent = lead_ws(raw);
        let line: String = raw.chars().skip(indent).collect();
        let is_seq = line.starts_with("- ");
        while stack.len() > 1 && (indent as i64) <= stack.last().unwrap().indent {
            let e = stack.last().unwrap();
            let keep = is_seq
                && indent as i64 == e.indent
                && e.key.is_some()
                && match &nodes[e.node] {
                    Node::Arr(_) => true,
                    Node::Obj(kv) => kv.is_empty(),
                };
            if keep {
                break;
            }
            stack.pop();
        }
        let top = stack.len() - 1;

        if is_seq {
            let value = line[2..].trim();
            let mut cont = stack[top].node;
            if !matches!(nodes[cont], Node::Arr(_)) {
                let parent = if stack.len() >= 2 {
                    Some(stack.len() - 2)
                } else {
                    None
                };
                match (parent, stack[top].key.clone()) {
                    (Some(p), Some(k)) if !k.is_empty() => {
                        let pn = stack[p].node;
                        if matches!(nodes[pn], Node::Arr(_)) {
                            return Err("invalid frontmatter array nesting".into());
                        }
                        nodes.push(Node::Arr(vec![]));
                        cont = nodes.len() - 1;
                        set(&mut nodes, pn, &k, Slot::Node(cont))?;
                        stack[top].node = cont;
                    }
                    _ => {
                        i += 1;
                        continue;
                    }
                }
            }
            if let Some(ci) = value.find(':') {
                let k = value[..ci].trim().to_string();
                let vr = value[ci + 1..].trim();
                let mut obj = vec![];
                if !vr.is_empty() {
                    obj.push((k, Slot::Val(coerce(vr))));
                }
                nodes.push(Node::Obj(obj));
                let oi = nodes.len() - 1;
                if let Node::Arr(a) = &mut nodes[cont] {
                    a.push(Slot::Node(oi));
                }
                stack.push(Frame {
                    indent: indent as i64,
                    node: oi,
                    key: None,
                });
            } else if let Node::Arr(a) = &mut nodes[cont] {
                a.push(Slot::Val(coerce(value)));
            }
            i += 1;
            continue;
        }

        // key: value  (`^([^:]+):\s*(.*)$`)
        let Some(ci) = line.find(':') else {
            i += 1;
            continue;
        };
        if ci == 0 {
            i += 1;
            continue;
        }
        let key = line[..ci].trim().to_string();
        let val = line[ci + 1..].trim_start();
        let tn = stack[top].node;

        if val.is_empty() {
            if matches!(nodes[tn], Node::Arr(_)) {
                return Err("frontmatter object container expected".into());
            }
            nodes.push(Node::Obj(vec![]));
            let ni = nodes.len() - 1;
            set(&mut nodes, tn, &key, Slot::Node(ni))?;
            stack.push(Frame {
                indent: indent as i64,
                node: ni,
                key: Some(key),
            });
            i += 1;
            continue;
        }
        if matches!(val, "|" | "|-" | ">" | ">-") {
            let mut collected: Vec<&str> = vec![];
            let mut block_indent: Option<usize> = None;
            i += 1;
            while i < lines.len() {
                let next = lines[i];
                if next.trim().is_empty() {
                    collected.push("");
                    i += 1;
                    continue;
                }
                let ni = lead_ws(next);
                if ni <= indent {
                    break;
                }
                let bi = *block_indent.get_or_insert(ni);
                if ni < bi {
                    break;
                }
                let byte = next.char_indices().nth(bi).map_or(next.len(), |(b, _)| b);
                collected.push(&next[byte..]);
                i += 1;
            }
            set(
                &mut nodes,
                tn,
                &key,
                Slot::Val(Value::String(collected.join("\n").trim_end().to_string())),
            )?;
            continue;
        }
        if val == "[]" {
            nodes.push(Node::Arr(vec![]));
            let ni = nodes.len() - 1;
            set(&mut nodes, tn, &key, Slot::Node(ni))?;
            i += 1;
            continue;
        }
        if val.starts_with('[') && val.ends_with(']') {
            let items: Vec<Slot> = split_inline(&val[1..val.len() - 1])
                .iter()
                .map(|s| coerce(s.trim()))
                .filter(|v| v != &Value::String(String::new()))
                .map(Slot::Val)
                .collect();
            nodes.push(Node::Arr(items));
            let ni = nodes.len() - 1;
            set(&mut nodes, tn, &key, Slot::Node(ni))?;
            i += 1;
            continue;
        }
        set(&mut nodes, tn, &key, Slot::Val(coerce(val)))?;
        i += 1;
    }
    match build(&nodes, 0) {
        Value::Object(m) => Ok(m),
        _ => Ok(Map::new()),
    }
}

fn build(nodes: &[Node], at: usize) -> Value {
    let slot = |s: &Slot| match s {
        Slot::Val(v) => v.clone(),
        Slot::Node(n) => build(nodes, *n),
    };
    match &nodes[at] {
        Node::Obj(kv) => Value::Object(kv.iter().map(|(k, s)| (k.clone(), slot(s))).collect()),
        Node::Arr(a) => Value::Array(a.iter().map(slot).collect()),
    }
}

/// Split on top-level commas; a quote opens a quoted item only as the first
/// non-blank character of the item.
fn split_inline(inner: &str) -> Vec<String> {
    let (mut items, mut buf, mut quote) = (vec![], String::new(), None::<char>);
    for ch in inner.chars() {
        if let Some(q) = quote {
            buf.push(ch);
            if ch == q {
                quote = None;
            }
        } else if (ch == '"' || ch == '\'') && buf.trim().is_empty() {
            quote = Some(ch);
            buf.push(ch);
        } else if ch == ',' {
            items.push(std::mem::take(&mut buf));
        } else {
            buf.push(ch);
        }
    }
    items.push(buf);
    items
}

fn coerce(raw: &str) -> Value {
    let v = raw.trim();
    let b = v.as_bytes();
    if b.len() >= 2
        && ((b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\''))
    {
        return Value::String(v[1..v.len() - 1].to_string());
    }
    match v {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        "null" | "~" => return Value::Null,
        _ => {}
    }
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
    let body = v.strip_prefix('-').unwrap_or(v);
    if digits(body) {
        return v.parse::<i64>().map(Value::from).unwrap_or_else(|_| num(v));
    }
    // `^-?\d*\.\d+$`
    if let Some((a, f)) = body.split_once('.') {
        if (a.is_empty() || digits(a)) && digits(f) {
            return num(v);
        }
    }
    Value::String(v.to_string())
}

fn num(v: &str) -> Value {
    v.parse::<f64>()
        .ok()
        .and_then(Number::from_f64)
        .map_or_else(|| Value::String(v.to_string()), Value::Number)
}
