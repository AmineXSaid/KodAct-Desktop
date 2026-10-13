//! Mermaid diagrams a reply writes, drawn by Forge itself: the source is
//! parsed here, laid out, and written as an SVG in Forge's colours, which the
//! chat then draws natively like any other diagram (`ui/diagram_view.rs`).
//!
//! Supported: flowcharts (`graph` / `flowchart`, every direction, the common
//! node shapes, labelled solid / dotted / thick links, chains, `&`,
//! subgraphs), state diagrams (`stateDiagram` / `-v2`) and sequence
//! diagrams (participants, actors, the arrow kinds, notes, loop / alt / opt /
//! par frames, autonumber). Anything else returns `None`, and the reply shows
//! the source as code.
//!
//! Flowcharts are laid out in layers: ranks by longest path (cycles broken
//! where a depth-first walk finds them), long links routed through dummy
//! points, crossings reduced by barycentre sweeps, then positions relaxed
//! towards their neighbours.

use std::collections::HashMap;
use std::fmt::Write as _;

/// The colours a drawing uses, as SVG colour strings (from the theme).
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub node_fill: String,
    pub node_stroke: String,
    pub text: String,
    pub muted: String,
    pub edge: String,
    pub label_bg: String,
    pub accent: String,
    pub accent_tint: String,
    pub group_fill: String,
    pub note_fill: String,
    pub note_stroke: String,
}

impl Palette {
    /// A neutral light palette, for tests.
    #[cfg(test)]
    pub fn test() -> Self {
        let s = |v: &str| v.to_string();
        Self {
            node_fill: s("#fff"),
            node_stroke: s("#ccc"),
            text: s("#111"),
            muted: s("#666"),
            edge: s("#888"),
            label_bg: s("#fafafa"),
            accent: s("#0075de"),
            accent_tint: s("#e6f3fe"),
            group_fill: s("#f5f5f5"),
            note_fill: s("#fff8e1"),
            note_stroke: s("#e0c070"),
        }
    }
}

const FONT: &str = "Hanken Grotesk, Segoe UI, Helvetica, Arial, sans-serif";
const SIZE: f32 = 13.;
/// Past this many nodes or messages, the source is shown instead.
const LIMIT: usize = 250;

/// The SVG for a Mermaid source, or `None` when it isn't one Forge draws.
pub fn render(source: &str, p: &Palette) -> Option<String> {
    let lines: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("%%"))
        .collect();
    // A front-matter block (`---` … `---`) is skipped.
    let lines: Vec<&str> = if lines.first() == Some(&"---") {
        let end = lines.iter().skip(1).position(|l| *l == "---")?;
        lines[end + 2..].to_vec()
    } else {
        lines
    };
    let (head, body) = lines.split_first()?;
    let kind = head.split_whitespace().next()?.to_ascii_lowercase();
    match kind.as_str() {
        "graph" | "flowchart" => {
            let dir = Dir::parse(head.split_whitespace().nth(1).unwrap_or("TD"));
            let g = parse_flow(body)?;
            Some(draw_graph(&g, dir, p))
        }
        "statediagram" | "statediagram-v2" => {
            let (g, dir) = parse_state(body)?;
            Some(draw_graph(&g, dir, p))
        }
        "sequencediagram" => {
            let s = parse_sequence(body)?;
            Some(draw_sequence(&s, p))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------- text

/// About how wide `text` is at `size` px in the sans face.
pub fn text_width(text: &str, size: f32) -> f32 {
    text.chars()
        .map(|c| match c {
            'i' | 'l' | 'j' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' | 'I' | ' ' => 0.28,
            'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '-' | '/' => 0.38,
            'm' | 'w' | 'M' | 'W' | '@' => 0.86,
            c if c.is_ascii_uppercase() => 0.66,
            c if c.is_ascii_digit() => 0.56,
            c if c.is_ascii() => 0.53,
            // CJK and other wide scripts.
            c if (c as u32) >= 0x2E80 => 1.0,
            _ => 0.6,
        })
        .sum::<f32>()
        * size
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A label's lines: `<br>` breaks, quotes and entities undone, long lines wrapped.
fn label_lines(raw: &str) -> Vec<String> {
    let text = raw
        .trim()
        .trim_matches('"')
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
        .replace("\\n", "\n")
        .replace("#quot;", "\"")
        .replace("&amp;", "&");
    let mut out = vec![];
    for line in text.lines() {
        let mut cur = String::new();
        for word in line.split_whitespace() {
            if !cur.is_empty() && cur.chars().count() + word.chars().count() > 30 {
                out.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn text_block(out: &mut String, lines: &[String], cx: f32, cy: f32, color: &str, weight: u32) {
    let lh = SIZE * 1.3;
    let top = cy - lh * (lines.len() as f32 - 1.) / 2.;
    for (i, l) in lines.iter().enumerate() {
        let _ = write!(
            out,
            r#"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" dominant-baseline="central" font-family="{FONT}" font-size="{SIZE}" font-weight="{weight}" fill="{color}">{}</text>"#,
            top + lh * i as f32,
            esc(l)
        );
    }
}

fn lines_size(lines: &[String]) -> (f32, f32) {
    let w = lines.iter().map(|l| text_width(l, SIZE)).fold(0., f32::max);
    (w, lines.len() as f32 * SIZE * 1.3)
}

// ---------------------------------------------------------------- graph model

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dir {
    Down,
    Up,
    Right,
    Left,
}

impl Dir {
    fn parse(s: &str) -> Self {
        match s.to_ascii_uppercase().as_str() {
            "LR" => Dir::Right,
            "RL" => Dir::Left,
            "BT" => Dir::Up,
            _ => Dir::Down,
        }
    }
    fn horizontal(self) -> bool {
        matches!(self, Dir::Right | Dir::Left)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Shape {
    Rect,
    Round,
    Stadium,
    Diamond,
    Circle,
    Hexagon,
    Cylinder,
    Start,
    End,
}

#[derive(Clone, Debug)]
struct Node {
    id: String,
    label: Vec<String>,
    shape: Shape,
    group: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Line {
    Solid,
    Dotted,
    Thick,
}

#[derive(Clone, Debug)]
struct Edge {
    from: usize,
    to: usize,
    label: Option<Vec<String>>,
    line: Line,
    arrow: bool,
    both: bool,
}

#[derive(Default, Debug)]
struct Graph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    groups: Vec<String>,
}

impl Graph {
    fn node(&mut self, id: &str, label: Option<(String, Shape)>, group: Option<usize>) -> usize {
        if let Some(i) = self.nodes.iter().position(|n| n.id == id) {
            if let Some((l, s)) = label {
                self.nodes[i].label = label_lines(&l);
                self.nodes[i].shape = s;
            }
            return i;
        }
        let (l, shape) = label.unwrap_or_else(|| (id.to_string(), Shape::Rect));
        self.nodes.push(Node {
            id: id.to_string(),
            label: label_lines(&l),
            shape,
            group,
        });
        self.nodes.len() - 1
    }
}

// ---------------------------------------------------------------- flowchart parsing

/// The shapes, longest opener first so `([` wins over `(`.
const SHAPES: &[(&str, &str, Shape)] = &[
    ("([", "])", Shape::Stadium),
    ("[(", ")]", Shape::Cylinder),
    ("((", "))", Shape::Circle),
    ("{{", "}}", Shape::Hexagon),
    ("[[", "]]", Shape::Rect),
    ("[/", "/]", Shape::Rect),
    ("[\\", "\\]", Shape::Rect),
    ("[/", "\\]", Shape::Rect),
    ("[", "]", Shape::Rect),
    ("(", ")", Shape::Round),
    ("{", "}", Shape::Diamond),
    (">", "]", Shape::Rect),
];

/// Ids are letters, digits and `_` (a `-` would start a link).
fn is_id_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A node at `s`: its id, its label and shape if given, and what follows.
fn parse_node(s: &str) -> Option<(String, Option<(String, Shape)>, &str)> {
    let s = s.trim_start();
    let end = s
        .char_indices()
        .find(|(_, c)| !is_id_char(*c))
        .map_or(s.len(), |(i, _)| i);
    if end == 0 {
        return None;
    }
    let (id, mut rest) = s.split_at(end);
    let mut label = None;
    for (open, close, shape) in SHAPES {
        if let Some(r) = rest.strip_prefix(open) {
            // A quoted label may hold the closing characters.
            let (text, after) = if let Some(q) = r.strip_prefix('"') {
                let qend = q.find('"')?;
                let after = q[qend + 1..].trim_start().strip_prefix(close)?;
                (format!("\"{}\"", &q[..qend]), after)
            } else {
                let cend = r.find(close)?;
                (r[..cend].to_string(), &r[cend + close.len()..])
            };
            label = Some((text, *shape));
            rest = after;
            break;
        }
    }
    // A class on the node (`A:::warn`) is dropped.
    if let Some(r) = rest.strip_prefix(":::") {
        let cend = r
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(r.len());
        rest = &r[cend..];
    }
    Some((id.to_string(), label, rest))
}

/// A link at `s`: its style and label, and what follows.
fn parse_link(s: &str) -> Option<(Line, bool, bool, Option<String>, &str)> {
    let s = s.trim_start();
    let both = s.starts_with('<');
    let body = if both { &s[1..] } else { s };
    let run = body
        .find(|c: char| !matches!(c, '-' | '=' | '.'))
        .unwrap_or(body.len());
    if run < 2 {
        return None;
    }
    let ops = &body[..run];
    let mut rest = &body[run..];
    let mut arrow = false;
    let mut label = None;
    let line = |ops: &str| {
        if ops.contains('=') {
            Line::Thick
        } else if ops.contains('.') {
            Line::Dotted
        } else {
            Line::Solid
        }
    };
    let mut style = line(ops);
    if let Some(r) = rest.strip_prefix(['>', 'o', 'x']) {
        arrow = rest.starts_with('>');
        rest = r;
    } else if matches!(ops, "--" | "==" | "-.") && rest.starts_with(char::is_whitespace) {
        // `A -- text --> B`: the label runs to the link's end.
        let ends = ["-->", "---", "==>", "===", ".->", "-.-", "--o", "--x"];
        let (at, end) = ends
            .iter()
            .filter_map(|e| rest.find(e).map(|i| (i, *e)))
            .min_by_key(|(i, _)| *i)?;
        label = Some(rest[..at].trim().to_string());
        let mut after = &rest[at + end.len()..];
        // A longer run (`---->`) goes on.
        after = after.trim_start_matches(['-', '=', '.']);
        if let Some(r) = after.strip_prefix('>') {
            after = r;
        }
        arrow = end.ends_with('>');
        style = line(end);
        rest = after;
    }
    // `-->|text|`
    let trimmed = rest.trim_start();
    if let Some(r) = trimmed.strip_prefix('|') {
        let end = r.find('|')?;
        label = Some(r[..end].to_string());
        rest = &r[end + 1..];
    }
    Some((style, arrow, both && arrow, label, rest))
}

fn parse_flow(body: &[&str]) -> Option<Graph> {
    let mut g = Graph::default();
    let mut stack: Vec<usize> = vec![];
    let statements = body
        .iter()
        .flat_map(|l| l.split(';'))
        .map(str::trim)
        .filter(|l| !l.is_empty());
    for st in statements {
        let word = st.split_whitespace().next().unwrap_or_default();
        match word {
            "subgraph" => {
                let rest = st["subgraph".len()..].trim();
                let title = match rest.find('[') {
                    Some(i) => rest[i + 1..]
                        .trim_end_matches(']')
                        .trim_matches('"')
                        .to_string(),
                    None => rest.trim_matches('"').to_string(),
                };
                g.groups.push(title);
                stack.push(g.groups.len() - 1);
                continue;
            }
            "end" => {
                stack.pop();
                continue;
            }
            "classDef" | "class" | "style" | "linkStyle" | "click" | "direction" | "accTitle"
            | "accDescr" => continue,
            _ => {}
        }
        let group = stack.last().copied();
        let mut rest = st;
        let mut prev: Vec<usize> = vec![];
        let mut pending: Option<(Line, bool, bool, Option<String>)> = None;
        loop {
            // A group of nodes: `A & B`.
            let mut here = vec![];
            loop {
                let (id, label, r) = parse_node(rest)?;
                here.push(g.node(&id, label, group));
                rest = r.trim_start();
                match rest.strip_prefix('&') {
                    Some(r) => rest = r,
                    None => break,
                }
            }
            if let Some((line, arrow, both, label)) = pending.take() {
                for &a in &prev {
                    for &b in &here {
                        g.edges.push(Edge {
                            from: a,
                            to: b,
                            label: label.as_deref().map(label_lines),
                            line,
                            arrow,
                            both,
                        });
                    }
                }
            }
            if rest.is_empty() {
                break;
            }
            let (line, arrow, both, label, r) = parse_link(rest)?;
            pending = Some((line, arrow, both, label));
            rest = r;
            prev = here;
        }
        if g.nodes.len() > LIMIT {
            return None;
        }
    }
    (!g.nodes.is_empty()).then_some(g)
}

// ---------------------------------------------------------------- state parsing

fn parse_state(body: &[&str]) -> Option<(Graph, Dir)> {
    let mut g = Graph::default();
    let mut dir = Dir::Down;
    let mut names: HashMap<String, String> = HashMap::new();
    let state_node =
        |g: &mut Graph, names: &HashMap<String, String>, raw: &str, as_source: bool| -> usize {
            let raw = raw.trim();
            if raw == "[*]" {
                return if as_source {
                    g.node("[*]start", Some((String::new(), Shape::Start)), None)
                } else {
                    g.node("[*]end", Some((String::new(), Shape::End)), None)
                };
            }
            let id = raw.trim_end_matches(":::").to_string();
            let label = names.get(&id).cloned().unwrap_or_else(|| id.clone());
            let exists = g.nodes.iter().any(|n| n.id == id);
            g.node(&id, (!exists).then(|| (label, Shape::Round)), None)
        };
    for line in body {
        let l = line.trim();
        if l == "}"
            || l.starts_with("note")
            || l.starts_with("end note")
            || l.starts_with("classDef")
            || l.starts_with("class ")
            || l == "--"
        {
            continue;
        }
        if let Some(d) = l.strip_prefix("direction") {
            dir = Dir::parse(d.trim());
            continue;
        }
        if let Some(rest) = l.strip_prefix("state ") {
            // `state "Long name" as Id`, or `state Id {`.
            let rest = rest.trim_end_matches('{').trim();
            if let Some((name, id)) = rest.split_once(" as ") {
                let id = id.trim().to_string();
                names.insert(id.clone(), name.trim().trim_matches('"').to_string());
                if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) {
                    n.label = label_lines(name);
                } else {
                    g.node(
                        &id,
                        Some((name.trim().trim_matches('"').to_string(), Shape::Round)),
                        None,
                    );
                }
            } else if !rest.is_empty() {
                state_node(
                    &mut g,
                    &names,
                    rest.split_whitespace().next().unwrap_or(rest),
                    true,
                );
            }
            continue;
        }
        if let Some((a, b)) = l.split_once("-->") {
            let (b, label) = match b.split_once(':') {
                Some((b, lab)) => (b, Some(label_lines(lab))),
                None => (b, None),
            };
            let from = state_node(&mut g, &names, a, true);
            let to = state_node(&mut g, &names, b, false);
            g.edges.push(Edge {
                from,
                to,
                label,
                line: Line::Solid,
                arrow: true,
                both: false,
            });
        } else if let Some((id, desc)) = l.split_once(':') {
            // `Id : description` names the state.
            let i = state_node(&mut g, &names, id, true);
            g.nodes[i].label = label_lines(desc);
        } else {
            state_node(&mut g, &names, l, true);
        }
        if g.nodes.len() > LIMIT {
            return None;
        }
    }
    (!g.nodes.is_empty()).then_some((g, dir))
}

// ---------------------------------------------------------------- graph layout

const PAD_X: f32 = 14.;
const PAD_Y: f32 = 9.;
const NODE_GAP: f32 = 28.;
const RANK_GAP: f32 = 54.;
const MARGIN: f32 = 12.;

struct Placed {
    /// Centre, in the drawing's frame.
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn node_size(n: &Node) -> (f32, f32) {
    let (tw, th) = lines_size(&n.label);
    let (w, h) = (tw + 2. * PAD_X, th + 2. * PAD_Y);
    match n.shape {
        Shape::Start | Shape::End => (16., 16.),
        Shape::Diamond => (w * 1.35 + 8., h * 1.45),
        Shape::Circle => {
            let d = tw.max(th) + 2. * PAD_Y + 6.;
            (d, d)
        }
        Shape::Hexagon => (w + 24., h),
        Shape::Cylinder => (w, h + 12.),
        _ => (w.max(48.), h),
    }
}

/// Every edge as a run of layout points (rank, slot index), with the
/// dummies a long edge passes through. Returns the ranks' contents too.
struct Layered {
    /// Each rank's members: a real node (`Some`) or a dummy (`None`) with its edge.
    ranks: Vec<Vec<Slot>>,
    /// Per edge: the slots it passes through, source to target, and whether it was reversed.
    paths: Vec<(Vec<(usize, usize)>, bool)>,
}

#[derive(Clone, Copy, Debug)]
enum Slot {
    Node(usize),
    Dummy(usize),
}

fn layer(g: &Graph) -> Layered {
    let n = g.nodes.len();
    // Break cycles: edges a depth-first walk meets going back are reversed.
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    for (i, e) in g.edges.iter().enumerate() {
        if e.from != e.to {
            adj[e.from].push(i);
        }
    }
    let mut state = vec![0u8; n];
    let mut reversed = vec![false; g.edges.len()];
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        let mut stack = vec![(root, 0usize)];
        state[root] = 1;
        while let Some((v, k)) = stack.pop() {
            if k < adj[v].len() {
                stack.push((v, k + 1));
                let e = adj[v][k];
                let w = g.edges[e].to;
                match state[w] {
                    0 => {
                        state[w] = 1;
                        stack.push((w, 0));
                    }
                    1 => reversed[e] = true,
                    _ => {}
                }
            } else {
                state[v] = 2;
            }
        }
    }
    let dag = |i: usize| {
        let e = &g.edges[i];
        if reversed[i] {
            (e.to, e.from)
        } else {
            (e.from, e.to)
        }
    };
    // Longest-path ranks.
    let mut rank = vec![0usize; n];
    let mut indeg = vec![0usize; n];
    let mut out: Vec<Vec<usize>> = vec![vec![]; n];
    for i in 0..g.edges.len() {
        let (a, b) = dag(i);
        if a != b {
            indeg[b] += 1;
            out[a].push(b);
        }
    }
    let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|v| indeg[*v] == 0).collect();
    while let Some(v) = queue.pop_front() {
        for &w in &out[v] {
            rank[w] = rank[w].max(rank[v] + 1);
            indeg[w] -= 1;
            if indeg[w] == 0 {
                queue.push_back(w);
            }
        }
    }
    let ranks_n = rank.iter().max().map_or(1, |m| m + 1);
    let mut ranks: Vec<Vec<Slot>> = vec![vec![]; ranks_n];
    for v in 0..n {
        ranks[rank[v]].push(Slot::Node(v));
    }
    // Long edges pass through a dummy on every rank between.
    let mut paths = vec![];
    for i in 0..g.edges.len() {
        let (a, b) = dag(i);
        let mut path = vec![(
            rank[a],
            ranks[rank[a]]
                .iter()
                .position(|s| matches!(s, Slot::Node(x) if *x == a))
                .unwrap_or(0),
        )];
        if a != b {
            for r in rank[a] + 1..rank[b] {
                ranks[r].push(Slot::Dummy(i));
                path.push((r, ranks[r].len() - 1));
            }
        }
        path.push((
            rank[b],
            ranks[rank[b]]
                .iter()
                .position(|s| matches!(s, Slot::Node(x) if *x == b))
                .unwrap_or(0),
        ));
        paths.push((path, reversed[i]));
    }
    Layered { ranks, paths }
}

/// Order each rank to cut crossings: barycentre sweeps down and up, a
/// group's members kept together.
fn order(g: &Graph, l: &mut Layered) {
    // Neighbours of each slot in the ranks above and below, by (rank, index).
    let nr = l.ranks.len();
    let key = |s: &Slot| match s {
        Slot::Node(v) => (0usize, *v),
        Slot::Dummy(e) => (1usize, *e),
    };
    // Edges between adjacent ranks, as slot keys.
    let mut links: Vec<((usize, (usize, usize)), (usize, (usize, usize)))> = vec![];
    for (path, _) in &l.paths {
        for w in path.windows(2) {
            let a = (w[0].0, key(&l.ranks[w[0].0][w[0].1]));
            let b = (w[1].0, key(&l.ranks[w[1].0][w[1].1]));
            links.push((a, b));
        }
    }
    for sweep in 0..8 {
        let down = sweep % 2 == 0;
        let rs: Vec<usize> = if down {
            (1..nr).collect()
        } else {
            (0..nr.saturating_sub(1)).rev().collect()
        };
        for r in rs {
            let other = if down { r - 1 } else { r + 1 };
            let pos: HashMap<(usize, usize), usize> = l.ranks[other]
                .iter()
                .enumerate()
                .map(|(i, s)| (key(s), i))
                .collect();
            let mut bary: Vec<(f32, usize, Slot)> = l.ranks[r]
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let k = key(s);
                    let ns: Vec<f32> = links
                        .iter()
                        .filter_map(|(a, b)| {
                            if (a.0, a.1) == (r, k) && b.0 == other {
                                pos.get(&b.1).map(|p| *p as f32)
                            } else if (b.0, b.1) == (r, k) && a.0 == other {
                                pos.get(&a.1).map(|p| *p as f32)
                            } else {
                                None
                            }
                        })
                        .collect();
                    let b = if ns.is_empty() {
                        i as f32
                    } else {
                        ns.iter().sum::<f32>() / ns.len() as f32
                    };
                    (b, i, *s)
                })
                .collect();
            // A subgraph's members sort as one block, at their mean.
            let mut group_mean: HashMap<usize, (f32, f32)> = HashMap::new();
            for (b, _, s) in &bary {
                if let Slot::Node(v) = s {
                    if let Some(gr) = g.nodes[*v].group {
                        let e = group_mean.entry(gr).or_default();
                        e.0 += b;
                        e.1 += 1.;
                    }
                }
            }
            for item in bary.iter_mut() {
                if let Slot::Node(v) = item.2 {
                    if let Some((sum, cnt)) = g.nodes[v].group.and_then(|gr| group_mean.get(&gr)) {
                        item.0 = sum / cnt + item.0 * 0.001;
                    }
                }
            }
            bary.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1.cmp(&b.1))
            });
            l.ranks[r] = bary.into_iter().map(|(_, _, s)| s).collect();
        }
    }
    // The paths point at slots by index: point their dummies at the new
    // order (`place` re-points the ends, which are nodes).
    for (i, (path, _)) in l.paths.iter_mut().enumerate() {
        let last = path.len() - 1;
        for step in path.iter_mut().take(last).skip(1) {
            step.1 = l.ranks[step.0]
                .iter()
                .position(|s| matches!(s, Slot::Dummy(e) if *e == i))
                .unwrap_or(0);
        }
    }
}

struct Laid {
    nodes: Vec<Placed>,
    /// Per edge, its points from source to target (after any reversal is undone).
    edges: Vec<Vec<(f32, f32)>>,
}

/// Positions in a top-down frame ("across" is x, "along" is y); turned for
/// other directions afterwards.
fn place(g: &Graph, dir: Dir) -> Laid {
    let mut l = layer(g);
    order(g, &mut l);
    // Fix reversed edges' endpoints: a reversed edge's path runs to → from.
    for (i, (path, rev)) in l.paths.iter_mut().enumerate() {
        let (a, b) = if *rev {
            (g.edges[i].to, g.edges[i].from)
        } else {
            (g.edges[i].from, g.edges[i].to)
        };
        let first = path[0].0;
        let last_ix = path.len() - 1;
        let lr = path[last_ix].0;
        path[0].1 = l.ranks[first]
            .iter()
            .position(|s| matches!(s, Slot::Node(x) if *x == a))
            .unwrap_or(0);
        path[last_ix].1 = l.ranks[lr]
            .iter()
            .position(|s| matches!(s, Slot::Node(x) if *x == b))
            .unwrap_or(0);
    }
    let horizontal = dir.horizontal();
    let sizes: Vec<(f32, f32)> = g.nodes.iter().map(node_size).collect();
    // (across, along) extents.
    let ext = |s: &Slot| -> (f32, f32) {
        match s {
            Slot::Node(v) => {
                let (w, h) = sizes[*v];
                if horizontal {
                    (h, w)
                } else {
                    (w, h)
                }
            }
            Slot::Dummy(_) => (10., 0.),
        }
    };
    // Along: each rank as deep as its deepest member.
    let mut along = vec![0f32; l.ranks.len()];
    let mut y = MARGIN;
    for (r, rank) in l.ranks.iter().enumerate() {
        let depth = rank.iter().map(|s| ext(s).1).fold(0., f32::max);
        along[r] = y + depth / 2.;
        // Edge labels need room between ranks.
        let gap = RANK_GAP + if horizontal { 40. } else { 0. };
        y += depth + gap;
    }
    // Across: packed, then pulled towards neighbours a few times.
    let mut across: Vec<Vec<f32>> = l
        .ranks
        .iter()
        .map(|rank| {
            let mut x = 0.;
            rank.iter()
                .map(|s| {
                    let w = ext(s).0;
                    let c = x + w / 2.;
                    x += w + NODE_GAP;
                    c
                })
                .collect()
        })
        .collect();
    // Centre every rank on the widest.
    let width_of = |r: usize, across: &Vec<Vec<f32>>| -> (f32, f32) {
        let rank = &l.ranks[r];
        if rank.is_empty() {
            return (0., 0.);
        }
        (
            across[r][0] - ext(&rank[0]).0 / 2.,
            across[r][rank.len() - 1] + ext(&rank[rank.len() - 1]).0 / 2.,
        )
    };
    let neighbours = |r: usize, i: usize| -> Vec<(usize, usize)> {
        let mut out = vec![];
        for (path, _) in &l.paths {
            for w in path.windows(2) {
                if w[0] == (r, i) {
                    out.push(w[1]);
                }
                if w[1] == (r, i) {
                    out.push(w[0]);
                }
            }
        }
        out
    };
    let nbrs: Vec<Vec<Vec<(usize, usize)>>> = (0..l.ranks.len())
        .map(|r| (0..l.ranks[r].len()).map(|i| neighbours(r, i)).collect())
        .collect();
    for _ in 0..12 {
        for r in 0..l.ranks.len() {
            let n = l.ranks[r].len();
            if n == 0 {
                continue;
            }
            let want: Vec<f32> = (0..n)
                .map(|i| {
                    let ns = &nbrs[r][i];
                    if ns.is_empty() {
                        across[r][i]
                    } else {
                        ns.iter().map(|(rr, ii)| across[*rr][*ii]).sum::<f32>() / ns.len() as f32
                    }
                })
                .collect();
            let mut xs = want.clone();
            // Keep order and spacing: a pass each way, then split the difference.
            let min_gap = |a: usize, b: usize| {
                (ext(&l.ranks[r][a]).0 + ext(&l.ranks[r][b]).0) / 2. + NODE_GAP
            };
            let mut left = xs.clone();
            for i in 1..n {
                left[i] = left[i].max(left[i - 1] + min_gap(i - 1, i));
            }
            let mut right = xs.clone();
            for i in (0..n - 1).rev() {
                right[i] = right[i].min(right[i + 1] - min_gap(i, i + 1));
            }
            for i in 0..n {
                xs[i] = (left[i] + right[i]) / 2.;
            }
            for i in 1..n {
                xs[i] = xs[i].max(xs[i - 1] + min_gap(i - 1, i));
            }
            across[r] = xs;
        }
    }
    // Shift everything to start at the margin.
    let min_x = (0..l.ranks.len())
        .map(|r| width_of(r, &across).0)
        .fold(f32::MAX, f32::min);
    for r in across.iter_mut() {
        for x in r.iter_mut() {
            *x += MARGIN - min_x;
        }
    }
    let span_along = y - RANK_GAP - if horizontal { 40. } else { 0. } + MARGIN;

    // Into the drawing's frame.
    let to_frame = |a: f32, b: f32| -> (f32, f32) {
        match dir {
            Dir::Down => (a, b),
            Dir::Up => (a, span_along - b),
            Dir::Right => (b, a),
            Dir::Left => (span_along - b, a),
        }
    };
    let mut nodes: Vec<Placed> = g
        .nodes
        .iter()
        .map(|_| Placed {
            x: 0.,
            y: 0.,
            w: 0.,
            h: 0.,
        })
        .collect();
    for (r, rank) in l.ranks.iter().enumerate() {
        for (i, s) in rank.iter().enumerate() {
            if let Slot::Node(v) = s {
                let (x, y) = to_frame(across[r][i], along[r]);
                nodes[*v] = Placed {
                    x,
                    y,
                    w: sizes[*v].0,
                    h: sizes[*v].1,
                };
            }
        }
    }
    let edges = l
        .paths
        .iter()
        .map(|(path, rev)| {
            let mut pts: Vec<(f32, f32)> = path
                .iter()
                .map(|(r, i)| to_frame(across[*r][*i], along[*r]))
                .collect();
            if *rev {
                pts.reverse();
            }
            pts
        })
        .collect();
    Laid { nodes, edges }
}

/// Where a link leaves or meets a node, heading towards `to`.
fn boundary(n: &Placed, shape: Shape, to: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (to.0 - n.x, to.1 - n.y);
    if dx == 0. && dy == 0. {
        return (n.x, n.y);
    }
    let (hw, hh) = (n.w / 2., n.h / 2.);
    match shape {
        Shape::Circle | Shape::Start | Shape::End => {
            let r = hw;
            let d = (dx * dx + dy * dy).sqrt();
            (n.x + dx / d * r, n.y + dy / d * r)
        }
        Shape::Diamond => {
            let t = 1. / (dx.abs() / hw + dy.abs() / hh);
            (n.x + dx * t, n.y + dy * t)
        }
        _ => {
            let t = (hw / dx.abs().max(1e-3)).min(hh / dy.abs().max(1e-3));
            (n.x + dx * t, n.y + dy * t)
        }
    }
}

fn draw_shape(out: &mut String, n: &Placed, shape: Shape, p: &Palette) {
    let (x, y, w, h) = (n.x - n.w / 2., n.y - n.h / 2., n.w, n.h);
    let style = format!(
        r#"fill="{}" stroke="{}" stroke-width="1.2""#,
        p.node_fill, p.node_stroke
    );
    let _ = match shape {
        Shape::Rect => write!(
            out,
            r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="4" {style}/>"#
        ),
        Shape::Round => write!(
            out,
            r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="10" {style}/>"#
        ),
        Shape::Stadium => write!(
            out,
            r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="{:.1}" {style}/>"#,
            h / 2.
        ),
        Shape::Diamond => write!(
            out,
            r#"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" {style}/>"#,
            n.x,
            y,
            x + w,
            n.y,
            n.x,
            y + h,
            x,
            n.y
        ),
        Shape::Circle => write!(
            out,
            r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" {style}/>"#,
            n.x,
            n.y,
            w / 2.
        ),
        Shape::Hexagon => write!(
            out,
            r#"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" {style}/>"#,
            x + 12.,
            y,
            x + w - 12.,
            y,
            x + w,
            n.y,
            x + w - 12.,
            y + h,
            x + 12.,
            y + h,
            x,
            n.y
        ),
        Shape::Cylinder => write!(
            out,
            r#"<path d="M{x:.1},{:.1} A{:.1},6 0 0 1 {:.1},{:.1} V{:.1} A{:.1},6 0 0 1 {x:.1},{:.1} Z" {style}/><path d="M{x:.1},{:.1} A{:.1},6 0 0 0 {:.1},{:.1}" fill="none" stroke="{}" stroke-width="1.2"/>"#,
            y + 6.,
            w / 2.,
            x + w,
            y + 6.,
            y + h - 6.,
            w / 2.,
            y + h - 6.,
            y + 6.,
            w / 2.,
            x + w,
            y + 6.,
            p.node_stroke
        ),
        Shape::Start => write!(
            out,
            r#"<circle cx="{:.1}" cy="{:.1}" r="7" fill="{}"/>"#,
            n.x, n.y, p.text
        ),
        Shape::End => write!(
            out,
            r#"<circle cx="{:.1}" cy="{:.1}" r="8" fill="none" stroke="{}" stroke-width="1.5"/><circle cx="{:.1}" cy="{:.1}" r="4.5" fill="{}"/>"#,
            n.x, n.y, p.text, n.x, n.y, p.text
        ),
    };
}

fn arrowhead(out: &mut String, tip: (f32, f32), from: (f32, f32), color: &str) {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let d = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (ux, uy) = (dx / d, dy / d);
    let (len, half) = (8., 4.);
    let base = (tip.0 - ux * len, tip.1 - uy * len);
    let _ = write!(
        out,
        r#"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{color}"/>"#,
        tip.0,
        tip.1,
        base.0 - uy * half,
        base.1 + ux * half,
        base.0 + uy * half,
        base.1 - ux * half
    );
}

fn draw_graph(g: &Graph, dir: Dir, p: &Palette) -> String {
    let laid = place(g, dir);
    let horizontal = dir.horizontal();
    // Everything drawn is measured, so nothing is cut off at the edge.
    let mut bounds = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut grow = |x0: f32, y0: f32, x1: f32, y1: f32| {
        bounds = (
            bounds.0.min(x0),
            bounds.1.min(y0),
            bounds.2.max(x1),
            bounds.3.max(y1),
        );
    };
    for n in &laid.nodes {
        grow(
            n.x - n.w / 2.,
            n.y - n.h / 2.,
            n.x + n.w / 2.,
            n.y + n.h / 2.,
        );
    }
    let mut body = String::new();
    // Subgraphs first, behind everything.
    for (gi, title) in g.groups.iter().enumerate() {
        let members: Vec<&Placed> = g
            .nodes
            .iter()
            .zip(&laid.nodes)
            .filter(|(n, _)| n.group == Some(gi))
            .map(|(_, pl)| pl)
            .collect();
        if members.is_empty() {
            continue;
        }
        let x0 = members
            .iter()
            .map(|m| m.x - m.w / 2.)
            .fold(f32::MAX, f32::min)
            - 12.;
        let x1 = members
            .iter()
            .map(|m| m.x + m.w / 2.)
            .fold(f32::MIN, f32::max)
            .max(x0 + text_width(title, 12.) + 8.)
            + 12.;
        let y0 = members
            .iter()
            .map(|m| m.y - m.h / 2.)
            .fold(f32::MAX, f32::min)
            - 30.;
        let y1 = members
            .iter()
            .map(|m| m.y + m.h / 2.)
            .fold(f32::MIN, f32::max)
            + 12.;
        grow(x0, y0, x1, y1);
        let _ = write!(
            body,
            r#"<rect x="{x0:.1}" y="{y0:.1}" width="{:.1}" height="{:.1}" rx="8" fill="{}" stroke="{}" stroke-width="1" stroke-dasharray="4 3"/>"#,
            x1 - x0,
            y1 - y0,
            p.group_fill,
            p.node_stroke
        );
        let _ = write!(
            body,
            r#"<text x="{:.1}" y="{:.1}" font-family="{FONT}" font-size="12" font-weight="600" fill="{}">{}</text>"#,
            x0 + 10.,
            y0 + 18.,
            p.muted,
            esc(title)
        );
    }
    // Links between the same two nodes are fanned apart, so neither hides the other.
    let mut pairs: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (i, e) in g.edges.iter().enumerate() {
        if e.from != e.to {
            pairs
                .entry((e.from.min(e.to), e.from.max(e.to)))
                .or_default()
                .push(i);
        }
    }
    let mut fan = vec![0f32; g.edges.len()];
    for list in pairs.values().filter(|l| l.len() > 1) {
        for (k, &i) in list.iter().enumerate() {
            fan[i] = (k as f32 - (list.len() - 1) as f32 / 2.) * 26.;
        }
    }
    let mut labels = String::new();
    let mut taken: Vec<(f32, f32, f32, f32)> = laid
        .nodes
        .iter()
        .map(|n| {
            (
                n.x - n.w / 2.,
                n.y - n.h / 2.,
                n.x + n.w / 2.,
                n.y + n.h / 2.,
            )
        })
        .collect();
    for (i, (e, pts)) in g.edges.iter().zip(&laid.edges).enumerate() {
        let (a, b) = (&laid.nodes[e.from], &laid.nodes[e.to]);
        let color = &p.edge;
        let width = if e.line == Line::Thick { 2.6 } else { 1.4 };
        let dash = if e.line == Line::Dotted {
            r#" stroke-dasharray="4 4""#
        } else {
            ""
        };
        if e.from == e.to {
            // A loop on the node's side.
            let (x, y) = (a.x + a.w / 2., a.y);
            grow(x, y - 26., x + 40., y + 26.);
            let _ = write!(
                body,
                r#"<path d="M{:.1},{:.1} C{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="none" stroke="{color}" stroke-width="{width}"{dash}/>"#,
                x,
                y - 6.,
                x + 36.,
                y - 26.,
                x + 36.,
                y + 26.,
                x + 1.,
                y + 7.
            );
            if e.arrow {
                arrowhead(&mut body, (x + 1., y + 7.), (x + 20., y + 16.), color);
            }
            if let Some(lines) = &e.label {
                let (tw, _) = lines_size(lines);
                grow(x + 40., y - 10., x + 48. + tw, y + 10.);
                let mut t = String::new();
                text_block(&mut t, lines, x + 44. + tw / 2., y, &p.muted, 400);
                labels.push_str(&t);
            }
            continue;
        }
        let mut pts = pts.clone();
        if fan[i] != 0. {
            // Across the flow: x for top-down, y for left-right.
            let o = fan[i];
            let shift = |q: (f32, f32)| {
                if horizontal {
                    (q.0, q.1 + o)
                } else {
                    (q.0 + o, q.1)
                }
            };
            if pts.len() == 2 {
                let mid = ((pts[0].0 + pts[1].0) / 2., (pts[0].1 + pts[1].1) / 2.);
                pts.insert(1, shift(mid));
            } else {
                let last = pts.len() - 1;
                for q in pts.iter_mut().take(last).skip(1) {
                    *q = shift(*q);
                }
            }
        }
        let next = pts.get(1).copied().unwrap_or((b.x, b.y));
        pts[0] = boundary(a, g.nodes[e.from].shape, next);
        let last = pts.len() - 1;
        let prev = pts[last - 1];
        pts[last] = boundary(b, g.nodes[e.to].shape, prev);
        for q in &pts {
            grow(q.0, q.1, q.0, q.1);
        }
        // Smooth: each segment a curve whose ends run along the flow.
        let mut d = format!("M{:.1},{:.1}", pts[0].0, pts[0].1);
        for w in pts.windows(2) {
            let (p0, p1) = (w[0], w[1]);
            if horizontal {
                let mx = (p0.0 + p1.0) / 2.;
                let _ = write!(
                    d,
                    " C{mx:.1},{:.1} {mx:.1},{:.1} {:.1},{:.1}",
                    p0.1, p1.1, p1.0, p1.1
                );
            } else {
                let my = (p0.1 + p1.1) / 2.;
                let _ = write!(
                    d,
                    " C{:.1},{my:.1} {:.1},{my:.1} {:.1},{:.1}",
                    p0.0, p1.0, p1.0, p1.1
                );
            }
        }
        let _ = write!(
            body,
            r#"<path d="{d}" fill="none" stroke="{color}" stroke-width="{width}"{dash}/>"#
        );
        // The arrow follows the curve's last tangent.
        let tail = |p0: (f32, f32), p1: (f32, f32)| {
            if horizontal {
                ((p0.0 + p1.0) / 2., p1.1)
            } else {
                (p1.0, (p0.1 + p1.1) / 2.)
            }
        };
        if e.arrow {
            arrowhead(&mut body, pts[last], tail(pts[last - 1], pts[last]), color);
        }
        if e.both {
            let head = if horizontal {
                ((pts[0].0 + pts[1].0) / 2., pts[0].1)
            } else {
                (pts[0].0, (pts[0].1 + pts[1].1) / 2.)
            };
            arrowhead(&mut body, pts[0], head, color);
        }
        if let Some(lines) = &e.label {
            // At the middle of the path, moved along it off anything already there.
            let mut mid = if pts.len() > 2 {
                pts[pts.len() / 2]
            } else {
                ((pts[0].0 + pts[1].0) / 2., (pts[0].1 + pts[1].1) / 2.)
            };
            let (tw, th) = lines_size(lines);
            let (hw, hh) = (tw / 2. + 5., th / 2. + 2.);
            // A fanned link's label sits outside its own curve.
            if fan[i] != 0. {
                let side = fan[i].signum();
                mid = if horizontal {
                    (mid.0, mid.1 + side * (hh + 3.))
                } else {
                    (mid.0 + side * (hw + 3.), mid.1)
                };
            }
            let hits = |m: (f32, f32), taken: &Vec<(f32, f32, f32, f32)>| {
                taken
                    .iter()
                    .any(|r| m.0 - hw < r.2 && m.0 + hw > r.0 && m.1 - hh < r.3 && m.1 + hh > r.1)
            };
            for step in [0., 1., -1., 2., -2., 3.] {
                let m = if horizontal {
                    (mid.0, mid.1 + step * (hh * 2. + 2.))
                } else {
                    (mid.0 + step * (hw + 8.), mid.1)
                };
                if !hits(m, &taken) {
                    mid = m;
                    break;
                }
            }
            taken.push((mid.0 - hw, mid.1 - hh, mid.0 + hw, mid.1 + hh));
            grow(mid.0 - hw, mid.1 - hh, mid.0 + hw, mid.1 + hh);
            let _ = write!(
                labels,
                r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="4" fill="{}"/>"#,
                mid.0 - hw,
                mid.1 - hh,
                hw * 2.,
                hh * 2.,
                p.label_bg
            );
            let mut t = String::new();
            text_block(&mut t, lines, mid.0, mid.1, &p.muted, 400);
            labels.push_str(&t);
        }
    }
    for (n, pl) in g.nodes.iter().zip(&laid.nodes) {
        draw_shape(&mut body, pl, n.shape, p);
        if !matches!(n.shape, Shape::Start | Shape::End) {
            text_block(&mut body, &n.label, pl.x, pl.y, &p.text, 500);
        }
    }
    body.push_str(&labels);
    let (x0, y0, x1, y1) = bounds;
    let (dx, dy) = (MARGIN - x0, MARGIN - y0);
    let body = format!(r#"<g transform="translate({dx:.1},{dy:.1})">{body}</g>"#);
    svg(
        (x1 - x0 + 2. * MARGIN).max(40.),
        (y1 - y0 + 2. * MARGIN).max(40.),
        &body,
    )
}

fn svg(w: f32, h: f32, body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.1} {h:.1}">{body}</svg>"#
    )
}

// ---------------------------------------------------------------- sequence diagrams

#[derive(Debug)]
struct Seq {
    actors: Vec<(String, String, bool)>,
    steps: Vec<Step>,
    numbered: bool,
}

#[derive(Debug)]
enum Step {
    Message {
        from: usize,
        to: usize,
        text: String,
        dashed: bool,
        arrow: Arrow,
    },
    Note {
        from: usize,
        to: usize,
        text: String,
        side: NoteSide,
    },
    Open {
        kind: String,
        text: String,
    },
    Else {
        text: String,
    },
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Arrow {
    Filled,
    Open,
    Cross,
    Async,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NoteSide {
    Left,
    Right,
    Over,
}

fn parse_sequence(body: &[&str]) -> Option<Seq> {
    let mut s = Seq {
        actors: vec![],
        steps: vec![],
        numbered: false,
    };
    fn actor(s: &mut Seq, name: &str) -> usize {
        let name = name.trim();
        if let Some(i) = s.actors.iter().position(|a| a.0 == name) {
            return i;
        }
        s.actors.push((name.to_string(), name.to_string(), false));
        s.actors.len() - 1
    }
    // Longest first, so `-->>` isn't read as `->>`.
    const ARROWS: &[(&str, bool, Arrow)] = &[
        ("-->>", true, Arrow::Filled),
        ("->>", false, Arrow::Filled),
        ("--x", true, Arrow::Cross),
        ("-x", false, Arrow::Cross),
        ("--)", true, Arrow::Async),
        ("-)", false, Arrow::Async),
        ("-->", true, Arrow::Open),
        ("->", false, Arrow::Open),
    ];
    for line in body {
        let l = line.trim();
        let word = l.split_whitespace().next().unwrap_or_default();
        let rest = l[word.len()..].trim();
        match word {
            "autonumber" => s.numbered = true,
            "participant" | "actor" => {
                let (id, alias) = match rest.split_once(" as ") {
                    Some((id, a)) => (id.trim(), a.trim()),
                    None => (rest, rest),
                };
                let i = actor(&mut s, id);
                s.actors[i].1 = alias.to_string();
                s.actors[i].2 = word == "actor";
            }
            "activate" | "deactivate" | "title" | "create" | "destroy" | "box" => {}
            "loop" | "alt" | "opt" | "par" | "critical" | "break" | "rect" => {
                s.steps.push(Step::Open {
                    kind: word.to_string(),
                    text: if word == "rect" {
                        String::new()
                    } else {
                        rest.to_string()
                    },
                })
            }
            "else" | "and" | "option" => s.steps.push(Step::Else {
                text: rest.to_string(),
            }),
            "end" => s.steps.push(Step::Close),
            w if w.eq_ignore_ascii_case("note") => {
                let (head, text) = rest.split_once(':')?;
                let head = head.trim();
                let (side, who) = if let Some(w) = head.strip_prefix("right of") {
                    (NoteSide::Right, w)
                } else if let Some(w) = head.strip_prefix("left of") {
                    (NoteSide::Left, w)
                } else {
                    (NoteSide::Over, head.strip_prefix("over").unwrap_or(head))
                };
                let mut names = who.split(',');
                let a = actor(&mut s, names.next()?);
                let b = names.next().map(|n| actor(&mut s, n)).unwrap_or(a);
                s.steps.push(Step::Note {
                    from: a,
                    to: b,
                    text: text.trim().to_string(),
                    side,
                });
            }
            _ => {
                let (head, text) = l.split_once(':').unwrap_or((l, ""));
                let (at, op, dashed, arrow) = ARROWS
                    .iter()
                    .filter_map(|(op, d, a)| head.find(op).map(|i| (i, *op, *d, *a)))
                    .min_by_key(|(i, op, ..)| (*i, std::cmp::Reverse(op.len())))?;
                let from = actor(&mut s, &head[..at]);
                let to_name = head[at + op.len()..].trim().trim_start_matches(['+', '-']);
                let to = actor(&mut s, to_name);
                s.steps.push(Step::Message {
                    from,
                    to,
                    text: text.trim().to_string(),
                    dashed,
                    arrow,
                });
            }
        }
        if s.steps.len() > LIMIT || s.actors.len() > 40 {
            return None;
        }
    }
    (!s.actors.is_empty()).then_some(s)
}

fn draw_sequence(s: &Seq, p: &Palette) -> String {
    let head_h = 34.;
    let col_gap = 36.;
    let row = 38.;
    // Each actor's box width, and the room messages between neighbours need.
    let widths: Vec<f32> = s
        .actors
        .iter()
        .map(|a| (text_width(&a.1, SIZE) + 28.).max(90.))
        .collect();
    let n = s.actors.len();
    let mut gaps = vec![col_gap; n.saturating_sub(1)];
    for st in &s.steps {
        if let Step::Message { from, to, text, .. } = st {
            let (a, b) = ((*from).min(*to), (*from).max(*to));
            if a != b {
                let need = text_width(text, 12.) + 30.;
                let have: f32 = (a..b)
                    .map(|i| widths[i] / 2. + widths[i + 1] / 2.)
                    .sum::<f32>()
                    + gaps[a..b].iter().sum::<f32>();
                if need > have {
                    let extra = (need - have) / (b - a) as f32;
                    for g in &mut gaps[a..b] {
                        *g += extra;
                    }
                }
            } else if a + 1 < n {
                gaps[a] =
                    gaps[a].max(text_width(text, 12.) + 50. - widths[a] / 2. - widths[a + 1] / 2.);
            }
        }
    }
    let mut xs = vec![];
    let mut x = MARGIN + 20.;
    for i in 0..n {
        xs.push(x + widths[i] / 2.);
        x += widths[i] + gaps.get(i).copied().unwrap_or(0.);
    }
    let total_w =
        x + MARGIN
            + 20.
            + if s.steps.iter().any(
                |st| matches!(st, Step::Message { from, to, .. } if from == to && *from == n - 1),
            ) {
                80.
            } else {
                0.
            };
    let mut y = MARGIN + head_h + 22.;
    let mut body = String::new();
    let mut frames: Vec<(f32, String, String, Vec<(f32, String)>)> = vec![];
    let mut drawn_frames = String::new();
    let mut number = 0;
    for st in &s.steps {
        match st {
            Step::Message {
                from,
                to,
                text,
                dashed,
                arrow,
            } => {
                number += 1;
                let label = if s.numbered {
                    format!("{number}. {text}")
                } else {
                    text.clone()
                };
                let (x1, x2) = (xs[*from], xs[*to]);
                let dash = if *dashed {
                    r#" stroke-dasharray="5 4""#
                } else {
                    ""
                };
                if from == to {
                    let _ = write!(
                        body,
                        r#"<path d="M{x1:.1},{:.1} h34 v18 h-34" fill="none" stroke="{}" stroke-width="1.3"{dash}/>"#,
                        y - 6.,
                        p.edge
                    );
                    arrowhead(&mut body, (x1 + 1., y + 12.), (x1 + 20., y + 12.), &p.edge);
                    let _ = write!(
                        body,
                        r#"<text x="{:.1}" y="{:.1}" font-family="{FONT}" font-size="12" fill="{}">{}</text>"#,
                        x1 + 40.,
                        y + 7.,
                        p.text,
                        esc(&label)
                    );
                    y += row + 10.;
                } else {
                    let dirn = if x2 > x1 { 1. } else { -1. };
                    let end = x2 - dirn * 1.;
                    let _ = write!(
                        body,
                        r#"<line x1="{x1:.1}" y1="{y:.1}" x2="{end:.1}" y2="{y:.1}" stroke="{}" stroke-width="1.3"{dash}/>"#,
                        p.edge
                    );
                    match arrow {
                        Arrow::Filled | Arrow::Async => {
                            arrowhead(&mut body, (end, y), (end - dirn * 10., y), &p.edge)
                        }
                        Arrow::Open => {}
                        Arrow::Cross => {
                            let _ = write!(
                                body,
                                r#"<path d="M{:.1},{:.1} l8,8 M{:.1},{:.1} l-8,8" stroke="{}" stroke-width="1.5"/>"#,
                                end - 4.,
                                y - 4.,
                                end + 4.,
                                y - 4.,
                                p.edge
                            );
                        }
                    }
                    let _ = write!(
                        body,
                        r#"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-family="{FONT}" font-size="12" fill="{}">{}</text>"#,
                        (x1 + x2) / 2.,
                        y - 7.,
                        p.text,
                        esc(&label)
                    );
                    y += row;
                }
            }
            Step::Note {
                from,
                to,
                text,
                side,
            } => {
                let lines = label_lines(text);
                let (tw, th) = lines_size(&lines);
                let w = tw + 20.;
                let (l, r) = (xs[*from].min(xs[*to]), xs[*from].max(xs[*to]));
                let x0 = match side {
                    NoteSide::Right => l + 10.,
                    NoteSide::Left => l - 10. - w,
                    NoteSide::Over => (l + r) / 2. - (w.max(r - l + 40.)) / 2.,
                };
                let w = if *side == NoteSide::Over {
                    w.max(r - l + 40.)
                } else {
                    w
                };
                let _ = write!(
                    body,
                    r#"<rect x="{x0:.1}" y="{:.1}" width="{w:.1}" height="{:.1}" rx="3" fill="{}" stroke="{}" stroke-width="1"/>"#,
                    y - 12.,
                    th + 12.,
                    p.note_fill,
                    p.note_stroke
                );
                text_block(
                    &mut body,
                    &lines,
                    x0 + w / 2.,
                    y - 6. + th / 2.,
                    &p.text,
                    400,
                );
                y += th + 24.;
            }
            Step::Open { kind, text } => {
                frames.push((y - 14., kind.clone(), text.clone(), vec![]));
                y += 16.;
            }
            Step::Else { text } => {
                if let Some(f) = frames.last_mut() {
                    f.3.push((y - 10., text.clone()));
                }
                y += 16.;
            }
            Step::Close => {
                if let Some((top, kind, text, elses)) = frames.pop() {
                    let depth = frames.len() as f32 * 8.;
                    let (x0, x1) = (MARGIN + 6. + depth, total_w - MARGIN - 6. - depth);
                    let bottom = y - 6.;
                    let _ = write!(
                        drawn_frames,
                        r#"<rect x="{x0:.1}" y="{top:.1}" width="{:.1}" height="{:.1}" rx="4" fill="{}" stroke="{}" stroke-width="1"/>"#,
                        x1 - x0,
                        bottom - top,
                        if kind == "rect" {
                            p.group_fill.as_str()
                        } else {
                            "none"
                        },
                        p.node_stroke
                    );
                    if kind != "rect" {
                        let tag_w = text_width(&kind, 11.) + 14.;
                        let _ = write!(
                            drawn_frames,
                            r#"<path d="M{x0:.1},{top:.1} h{tag_w:.1} v12 l-6,6 h{:.1} Z" fill="{}"/>"#,
                            -(tag_w - 6.),
                            p.accent_tint
                        );
                        let _ = write!(
                            drawn_frames,
                            r#"<text x="{:.1}" y="{:.1}" font-family="{FONT}" font-size="11" font-weight="600" fill="{}">{}</text>"#,
                            x0 + 6.,
                            top + 13.,
                            p.accent,
                            esc(&kind)
                        );
                        if !text.is_empty() {
                            let _ = write!(
                                drawn_frames,
                                r#"<text x="{:.1}" y="{:.1}" font-family="{FONT}" font-size="11" fill="{}">[{}]</text>"#,
                                x0 + tag_w + 8.,
                                top + 13.,
                                p.muted,
                                esc(&text)
                            );
                        }
                    }
                    for (ey, et) in elses {
                        let _ = write!(
                            drawn_frames,
                            r#"<line x1="{x0:.1}" y1="{ey:.1}" x2="{x1:.1}" y2="{ey:.1}" stroke="{}" stroke-dasharray="4 3"/>"#,
                            p.node_stroke
                        );
                        if !et.is_empty() {
                            let _ = write!(
                                drawn_frames,
                                r#"<text x="{:.1}" y="{:.1}" font-family="{FONT}" font-size="11" fill="{}">[{}]</text>"#,
                                x0 + 8.,
                                ey + 13.,
                                p.muted,
                                esc(&et)
                            );
                        }
                    }
                    y += 10.;
                }
            }
        }
    }
    let bottom_top = y + 4.;
    let total_h = bottom_top + head_h + MARGIN;
    let mut heads = String::new();
    for (i, a) in s.actors.iter().enumerate() {
        let (x, w) = (xs[i], widths[i]);
        let _ = write!(
            heads,
            r#"<line x1="{x:.1}" y1="{:.1}" x2="{x:.1}" y2="{bottom_top:.1}" stroke="{}" stroke-width="1" stroke-dasharray="3 4"/>"#,
            MARGIN + head_h,
            p.node_stroke
        );
        for top in [MARGIN, bottom_top] {
            let _ = write!(
                heads,
                r#"<rect x="{:.1}" y="{top:.1}" width="{w:.1}" height="{head_h:.1}" rx="{}" fill="{}" stroke="{}" stroke-width="1.2"/>"#,
                x - w / 2.,
                if a.2 { 17. } else { 6. },
                p.node_fill,
                p.node_stroke
            );
            text_block(
                &mut heads,
                &[a.1.clone()],
                x,
                top + head_h / 2.,
                &p.text,
                600,
            );
        }
    }
    svg(total_w, total_h, &format!("{drawn_frames}{heads}{body}"))
}

// ---------------------------------------------------------------- fences

/// The SVG a fenced block draws, if it is a diagram: Mermaid, an SVG, or a
/// page holding one. `None` leaves it as code.
pub fn fence_svg(lang: &str, code: &str, p: &Palette) -> Option<String> {
    match lang.to_ascii_lowercase().as_str() {
        "mermaid" | "mmd" => render(code, p),
        "svg" | "xml" | "html" | "htm" => crate::diagram::extract_svg(code)
            .filter(|s| crate::diagram::view_size(s).is_some())
            .map(str::to_string),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(src: &str) -> Graph {
        let lines: Vec<&str> = src
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        parse_flow(&lines).expect("parses")
    }

    #[test]
    fn flowchart_nodes_shapes_and_links() {
        let g = flow("A[Start] --> B{Is it ok?}\nB -->|yes| C([Done])\nB -- no --> D((Retry))\nD -.-> A\nC ==> E[(Store)] & F");
        let ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["A", "B", "C", "D", "E", "F"]);
        assert_eq!(g.nodes[1].shape, Shape::Diamond);
        assert_eq!(g.nodes[2].shape, Shape::Stadium);
        assert_eq!(g.nodes[3].shape, Shape::Circle);
        assert_eq!(g.nodes[4].shape, Shape::Cylinder);
        assert_eq!(g.nodes[0].label, vec!["Start"]);
        assert_eq!(g.edges.len(), 6);
        assert_eq!(g.edges[1].label.as_ref().unwrap(), &vec!["yes".to_string()]);
        assert_eq!(g.edges[2].label.as_ref().unwrap(), &vec!["no".to_string()]);
        assert_eq!(g.edges[3].line, Line::Dotted);
        assert_eq!(g.edges[4].line, Line::Thick);
        assert_eq!((g.edges[4].from, g.edges[4].to, g.edges[5].to), (2, 4, 5));
    }

    #[test]
    fn quoted_labels_subgraphs_and_classes() {
        let g = flow("subgraph api [The API]\nA[\"Calls (v2)\"]:::hot --> B\nend\nB --> C;C --> D");
        assert_eq!(g.groups, vec!["The API"]);
        assert_eq!(g.nodes[0].label, vec!["Calls (v2)"]);
        assert_eq!(g.nodes[0].group, Some(0));
        assert_eq!(g.nodes[2].group, None);
        assert_eq!(g.edges.len(), 3);
    }

    #[test]
    fn every_kind_draws_a_well_formed_svg() {
        let p = Palette::test();
        for src in [
            "graph TD\nA --> B --> C\nC --> A\nA --> C",
            "flowchart LR\n  user((User)) -->|types| ui[Forge desktop]\n  ui --> cli[ForgeCLI] --> api{{Model API}}\n  cli -.-> disk[(Sessions)]",
            "graph BT\nA-->B",
            "stateDiagram-v2\n[*] --> Idle\nIdle --> Working : send\nWorking --> Idle : done\nWorking --> [*]",
            "sequenceDiagram\nautonumber\nparticipant U as You\nactor F as Forge\nU->>F: Ask a question\nloop each tool\nF->>F: Run it\nend\nalt ok\nF-->>U: Answer\nelse failed\nF--xU: Error\nend\nNote over U,F: done",
        ] {
            let svg = render(src, &p).unwrap_or_else(|| panic!("{src}"));
            assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"), "{svg}");
            assert!(crate::diagram::view_size(&svg).is_some(), "{svg}");
            assert!(!svg.contains("NaN") && !svg.contains("inf"), "{src}\n{svg}");
            // Every element closes: as many opening tags as closings/self-closings.
            let opens = svg.matches('<').count();
            assert!(opens > 3);
        }
    }

    #[test]
    fn labels_are_escaped() {
        let svg = render("graph TD\nA[a < b & c] --> B", &Palette::test()).unwrap();
        assert!(svg.contains("a &lt; b &amp; c"), "{svg}");
    }

    #[test]
    fn unknown_or_broken_sources_are_left_as_code() {
        let p = Palette::test();
        assert!(render("pie title Pets\n\"Dogs\" : 3", &p).is_none());
        assert!(render("", &p).is_none());
        assert!(render("graph TD\n-->", &p).is_none());
        assert!(fence_svg("rust", "fn main() {}", &p).is_none());
        assert!(fence_svg("svg", "<svg viewBox=\"0 0 10 10\"><rect/></svg>", &p).is_some());
        assert!(fence_svg("html", "<p>no drawing</p>", &p).is_none());
    }

    #[test]
    fn layout_keeps_ranks_apart_and_nodes_from_overlapping() {
        let g = flow("A --> B\nA --> C\nA --> D\nB --> E\nC --> E\nD --> E\nA --> E");
        let laid = place(&g, Dir::Down);
        let y = |i: usize| laid.nodes[i].y;
        assert!(y(0) < y(1) && y(1) < y(4), "ranks go down");
        for i in 0..laid.nodes.len() {
            for j in i + 1..laid.nodes.len() {
                let (a, b) = (&laid.nodes[i], &laid.nodes[j]);
                let apart = (a.x - b.x).abs() >= (a.w + b.w) / 2. - 0.5
                    || (a.y - b.y).abs() >= (a.h + b.h) / 2. - 0.5;
                assert!(apart, "{i} and {j} overlap");
            }
        }
        // The long edge A → E passes through a dummy point.
        assert_eq!(laid.edges[6].len(), 3);
    }

    #[test]
    fn a_cycle_still_lays_out() {
        let g = flow("A --> B\nB --> C\nC --> A");
        let laid = place(&g, Dir::Right);
        assert!(laid.nodes[0].x < laid.nodes[1].x && laid.nodes[1].x < laid.nodes[2].x);
        // The back edge runs from C to A.
        let back = &laid.edges[2];
        assert!(back.first().unwrap().0 > back.last().unwrap().0);
    }
}

/// `MERMAID_DUMP=dir cargo test dump_samples -- --ignored` writes sample
/// drawings to look at.
#[cfg(test)]
#[test]
#[ignore]
fn dump_samples() {
    let Some(dir) = std::env::var_os("MERMAID_DUMP") else {
        return;
    };
    let samples = [
        ("flow-td", "flowchart TD\n  A[You type a message] --> B{Endpoint set up?}\n  B -->|no| C[Open Settings › Models]\n  B -->|yes| D[ForgeCLI process]\n  C --> D\n  D --> E[(Session log)]\n  D -.->|stream-json| F([Reply in the chat])\n  F --> G((Done))\n  D --> F"),
        ("flow-lr", "graph LR\n  subgraph Desktop\n    ui[Forge UI] --> st[AppState]\n  end\n  st --> cli[ForgeCLI]\n  cli ==> api{{Model API}}\n  cli --> tools[Tools]\n  tools --> cli"),
        ("state", "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Working : send\n  Working --> NeedsYou : permission\n  NeedsYou --> Working : allow\n  Working --> Idle : done\n  Working --> Error : failed\n  Error --> [*]"),
        ("sequence", "sequenceDiagram\n  autonumber\n  participant U as You\n  participant D as Forge Desktop\n  participant C as ForgeCLI\n  U->>D: Send a message\n  D->>C: stream-json user turn\n  loop every tool call\n    C->>C: Run the tool\n    C-->>D: tool_use / tool_result\n  end\n  alt needs permission\n    C->>D: can_use_tool\n    D->>U: Ask\n    U-->>D: Allow\n  else allowed\n    Note over C: runs at once\n  end\n  C-->>D: result\n  D-->>U: Reply"),
    ];
    let p = Palette::test();
    for (name, src) in samples {
        std::fs::write(
            std::path::Path::new(&dir).join(format!("{name}.svg")),
            render(src, &p).unwrap(),
        )
        .unwrap();
    }
}
