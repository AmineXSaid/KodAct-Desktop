//! The Mermaid kinds past flowcharts, states and sequences: class and ER
//! diagrams and mindmaps (laid out by the flowchart engine), and the charts
//! drawn on their own axes: pie, gantt, timeline, user journey, xychart,
//! quadrant chart and git graph. Each returns `None` when its source can't
//! be read, and the reply shows the source as code.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::{
    esc, label_lines, svg, text_width, Dir, Edge, Graph, Line, Palette, Placed, Shape, FONT, LIMIT,
    PAD_X, PAD_Y, SIZE,
};

const M: f32 = 16.;
const LH: f32 = SIZE * 1.3;
const TITLE: f32 = 15.;
const SMALL: f32 = 11.5;

#[allow(clippy::too_many_arguments)]
fn text(
    out: &mut String,
    x: f32,
    y: f32,
    anchor: &str,
    size: f32,
    weight: u32,
    color: &str,
    s: &str,
) {
    let _ = write!(
        out,
        r#"<text x="{x:.1}" y="{y:.1}" text-anchor="{anchor}" dominant-baseline="central" font-family="{FONT}" font-size="{size}" font-weight="{weight}" fill="{color}">{}</text>"#,
        esc(s)
    );
}

fn rect(
    out: &mut String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    fill: &str,
    stroke: Option<&str>,
) {
    let stroke = stroke
        .map(|s| format!(r#" stroke="{s}" stroke-width="1.2""#))
        .unwrap_or_default();
    let _ = write!(
        out,
        r#"<rect x="{x:.1}" y="{y:.1}" width="{:.1}" height="{:.1}" rx="{rx:.1}" fill="{fill}"{stroke}/>"#,
        w.max(0.),
        h.max(0.)
    );
}

fn line(out: &mut String, a: (f32, f32), b: (f32, f32), color: &str, width: f32, dash: bool) {
    let dash = if dash {
        r#" stroke-dasharray="4 3""#
    } else {
        ""
    };
    let _ = write!(
        out,
        r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{color}" stroke-width="{width}"{dash}/>"#,
        a.0, a.1, b.0, b.1
    );
}

fn color(p: &Palette, i: usize) -> &str {
    if p.series.is_empty() {
        &p.accent
    } else {
        &p.series[i % p.series.len()]
    }
}

/// A title line (`title …` or the header's own `title …`), unquoted.
fn title_of(l: &str) -> Option<String> {
    l.strip_prefix("title")
        .filter(|r| r.is_empty() || r.starts_with(char::is_whitespace))
        .map(|r| r.trim().trim_matches('"').to_string())
}

/// The title drawn at the top; returns the height it takes.
fn draw_title(out: &mut String, title: &Option<String>, w: f32, p: &Palette) -> f32 {
    match title {
        Some(t) if !t.is_empty() => {
            text(
                out,
                w / 2.,
                M + TITLE / 2.,
                "middle",
                TITLE,
                600,
                &p.text,
                t,
            );
            TITLE + 12.
        }
        _ => 0.,
    }
}

/// Round numbers for an axis from `lo` to `hi`: (start, end, step).
fn nice(lo: f32, hi: f32, target: f32) -> (f32, f32, f32) {
    let span = (hi - lo).abs().max(1e-6);
    let raw = span / target;
    let mag = 10f32.powf(raw.log10().floor());
    let step = [1., 2., 2.5, 5., 10.]
        .iter()
        .map(|m| m * mag)
        .find(|s| *s >= raw)
        .unwrap_or(10. * mag);
    ((lo / step).floor() * step, (hi / step).ceil() * step, step)
}

fn num(v: f32) -> String {
    if (v - v.round()).abs() < 1e-4 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ---------------------------------------------------------------- table nodes

/// A class or entity box: the name on a tinted band, then its members.
pub(super) fn table_size(lines: &[String]) -> (f32, f32) {
    let w = lines
        .iter()
        .enumerate()
        .map(|(i, l)| text_width(l, SIZE) * if i == 0 { 1.08 } else { 1. })
        .fold(0., f32::max);
    let members = lines.len().saturating_sub(1) as f32;
    let body = if members > 0. {
        members * LH + PAD_Y * 1.6
    } else {
        0.
    };
    ((w + 2. * PAD_X).max(90.), LH + 2. * PAD_Y + body)
}

pub(super) fn table_text(out: &mut String, n: &Placed, lines: &[String], p: &Palette) {
    let (x, y) = (n.x - n.w / 2., n.y - n.h / 2.);
    let head = LH + 2. * PAD_Y;
    let _ = write!(
        out,
        r#"<path d="M{:.1},{:.1} V{:.1} A5.4,5.4 0 0 1 {:.1},{:.1} H{:.1} A5.4,5.4 0 0 1 {:.1},{:.1} V{:.1} Z" fill="{}"/>"#,
        x + 0.6,
        y + head,
        y + 6.,
        x + 6.,
        y + 0.6,
        x + n.w - 6.,
        x + n.w - 0.6,
        y + 6.,
        y + head,
        p.accent_tint
    );
    let title = lines.first().map(String::as_str).unwrap_or_default();
    text(out, n.x, y + head / 2., "middle", SIZE, 600, &p.text, title);
    if lines.len() > 1 {
        line(
            out,
            (x, y + head),
            (x + n.w, y + head),
            &p.node_stroke,
            1.,
            false,
        );
        for (i, l) in lines[1..].iter().enumerate() {
            let ly = y + head + PAD_Y * 0.8 + LH * (i as f32 + 0.5);
            text(out, x + PAD_X, ly, "start", SIZE, 400, &p.text, l);
        }
    }
}

// ---------------------------------------------------------------- class diagrams

/// Relationship operators, longest first.
const CLASS_OPS: &[&str] = &[
    "<|--", "--|>", "<|..", "..|>", "*--", "--*", "o--", "--o", "-->", "<--", "..>", "<..", "--",
    "..",
];

fn class_name(s: &str) -> String {
    let s = s.trim().trim_matches('`');
    let s = s.split(":::").next().unwrap_or(s);
    s.split(['[', '~', '{'])
        .next()
        .unwrap_or(s)
        .trim()
        .to_string()
}

fn generics(s: &str) -> String {
    // `List~int~` reads as `List<int>`.
    let mut out = String::new();
    let mut open = true;
    for c in s.chars() {
        if c == '~' {
            out.push(if open { '<' } else { '>' });
            open = !open;
        } else {
            out.push(c);
        }
    }
    out
}

/// Where a class relationship's operator sits: (start, op).
fn find_op(l: &str) -> Option<(usize, &'static str)> {
    let mut best: Option<(usize, &'static str)> = None;
    for op in CLASS_OPS {
        let mut from = 0;
        while let Some(i) = l[from..].find(op).map(|i| i + from) {
            let before = l[..i].chars().last();
            let after = l[i + op.len()..].chars().next();
            let ok_before = !op.starts_with('o') || matches!(before, None | Some(' ' | '"'));
            let ok_after = !op.ends_with('o') || matches!(after, None | Some(' ' | '"'));
            if ok_before && ok_after {
                if best.is_none_or(|(b, bop)| i < b || (i == b && op.len() > bop.len())) {
                    best = Some((i, op));
                }
                break;
            }
            from = i + 1;
        }
    }
    best
}

/// A quoted cardinality at the end (`left`) or start of a side.
fn split_card(s: &str, at_end: bool) -> (String, Option<String>) {
    let s = s.trim();
    if at_end && s.ends_with('"') {
        if let Some(q) = s[..s.len() - 1].rfind('"') {
            return (
                s[..q].trim().to_string(),
                Some(s[q + 1..s.len() - 1].to_string()),
            );
        }
    }
    if !at_end && s.starts_with('"') {
        if let Some(q) = s[1..].find('"') {
            return (s[q + 2..].trim().to_string(), Some(s[1..q + 1].to_string()));
        }
    }
    (s.to_string(), None)
}

pub(super) fn class(body: &[&str]) -> Option<(Graph, Dir)> {
    let mut g = Graph::default();
    let mut dir = Dir::Down;
    let mut members: HashMap<usize, Vec<String>> = HashMap::new();
    let mut open: Option<usize> = None;
    let add = |g: &mut Graph, name: &str| -> usize {
        let id = class_name(name);
        let exists = g.nodes.iter().any(|n| n.id == id);
        g.node(&id, (!exists).then(|| (id.clone(), Shape::Table)), None)
    };
    for l in body {
        let l = l.trim();
        if let Some(i) = open {
            if l == "}" {
                open = None;
            } else if !l.starts_with("<<") {
                members.entry(i).or_default().push(generics(l));
            } else {
                members.entry(i).or_default().insert(0, l.to_string());
            }
            continue;
        }
        if let Some(d) = l.strip_prefix("direction") {
            dir = Dir::parse(d.trim());
            continue;
        }
        if [
            "note",
            "classDef",
            "style",
            "cssClass",
            "link",
            "click",
            "callback",
            "namespace",
            "}",
        ]
        .iter()
        .any(|k| l.starts_with(k))
        {
            continue;
        }
        if let Some(rest) = l.strip_prefix("class ") {
            let i = add(&mut g, rest);
            // `class A["Label"]`.
            if let Some(lab) = rest.split_once('[').and_then(|(_, r)| r.split_once(']')) {
                g.nodes[i].label = vec![lab.0.trim_matches('"').to_string()];
            } else {
                g.nodes[i].label = vec![generics(rest.trim_end_matches('{').trim())];
            }
            if rest.trim_end().ends_with('{') {
                open = Some(i);
            }
            continue;
        }
        if l.starts_with("<<") {
            // `<<interface>> Shape`.
            if let Some((ann, name)) = l.split_once(">>") {
                let i = add(&mut g, name);
                members.entry(i).or_default().insert(0, format!("{ann}>>"));
            }
            continue;
        }
        if let Some((at, op)) = find_op(l) {
            let (left, rest) = (&l[..at], &l[at + op.len()..]);
            let (rest, label) = match rest.split_once(':') {
                Some((r, lab)) => (r, Some(lab.trim().to_string())),
                None => (rest, None),
            };
            let (a, ca) = split_card(left, true);
            let (b, cb) = split_card(rest, false);
            if a.is_empty() || b.is_empty() {
                continue;
            }
            let (ia, ib) = (add(&mut g, &a), add(&mut g, &b));
            let points_left = op.starts_with('<') || op.starts_with('*') || op.starts_with('o');
            let points_right = op.ends_with('>') || op.ends_with('*') || op.ends_with('o');
            let (from, to) = if points_left { (ib, ia) } else { (ia, ib) };
            let mut text = vec![];
            if ca.is_some() || cb.is_some() {
                text.push(format!(
                    "{} : {}",
                    ca.as_deref().unwrap_or("·"),
                    cb.as_deref().unwrap_or("·")
                ));
            }
            text.extend(label.filter(|s| !s.is_empty()));
            g.edges.push(Edge {
                from,
                to,
                label: (!text.is_empty()).then(|| label_lines(&text.join("\n"))),
                line: if op.contains("..") {
                    Line::Dotted
                } else {
                    Line::Solid
                },
                arrow: points_left || points_right,
                both: false,
            });
        } else if let Some((name, member)) = l.split_once(':') {
            let i = add(&mut g, name);
            members.entry(i).or_default().push(generics(member.trim()));
        } else if !l.is_empty() {
            add(&mut g, l);
        }
        if g.nodes.len() > LIMIT {
            return None;
        }
    }
    for (i, m) in members {
        g.nodes[i].label.extend(m);
    }
    (!g.nodes.is_empty()).then_some((g, dir))
}

// ---------------------------------------------------------------- ER diagrams

/// A crow's-foot end as a cardinality: `||` one, `|o` / `o|` zero or one,
/// `}|` / `|{` one or more, `}o` / `o{` zero or more.
fn er_side(m: &str) -> &'static str {
    match m {
        "||" => "1",
        "|o" | "o|" => "0..1",
        "}|" | "|{" => "1..N",
        "}o" | "o{" => "0..N",
        _ => "",
    }
}

pub(super) fn er(body: &[&str]) -> Option<(Graph, Dir)> {
    let mut g = Graph::default();
    let mut dir = Dir::Down;
    let mut open: Option<usize> = None;
    let add = |g: &mut Graph, raw: &str| -> usize {
        let raw = raw.trim();
        let (id, label) = match raw.split_once('[') {
            Some((id, r)) => (
                id.trim(),
                r.trim_end_matches(']').trim_matches('"').to_string(),
            ),
            None => (raw, raw.trim_matches('"').to_string()),
        };
        let exists = g.nodes.iter().any(|n| n.id == id);
        g.node(id, (!exists).then_some((label, Shape::Table)), None)
    };
    for l in body {
        let l = l.trim();
        if let Some(i) = open {
            if l == "}" {
                open = None;
            } else {
                // `type name PK, FK "comment"`.
                let words: Vec<&str> = l.split_whitespace().collect();
                if let [ty, name, rest @ ..] = words.as_slice() {
                    let keys: Vec<&str> = rest
                        .iter()
                        .take_while(|w| !w.starts_with('"'))
                        .map(|w| w.trim_end_matches(','))
                        .collect();
                    let keys = if keys.is_empty() {
                        String::new()
                    } else {
                        format!("  {}", keys.join(", "))
                    };
                    g.nodes[i].label.push(format!("{name}: {ty}{keys}"));
                }
            }
            continue;
        }
        if let Some(d) = l.strip_prefix("direction") {
            dir = Dir::parse(d.trim());
            continue;
        }
        if l.ends_with('{') {
            open = Some(add(&mut g, l.trim_end_matches('{')));
            continue;
        }
        let (rel, label) = match l.split_once(':') {
            Some((r, lab)) => (r, Some(lab.trim().trim_matches('"').to_string())),
            None => (l, None),
        };
        let words: Vec<&str> = rel.split_whitespace().collect();
        if let [a, op, b] = words.as_slice() {
            let dotted = op.contains("..");
            let Some((lm, rm)) = op.split_once(if dotted { ".." } else { "--" }) else {
                continue;
            };
            let (ia, ib) = (add(&mut g, a), add(&mut g, b));
            let card = format!("{} : {}", er_side(lm), er_side(rm));
            let mut text = vec![];
            text.extend(label.filter(|s| !s.is_empty()));
            if card != " : " {
                text.push(card);
            }
            g.edges.push(Edge {
                from: ia,
                to: ib,
                label: (!text.is_empty()).then(|| label_lines(&text.join("\n"))),
                line: if dotted { Line::Dotted } else { Line::Solid },
                arrow: false,
                both: false,
            });
        } else if let [a] = words.as_slice() {
            add(&mut g, a);
        }
        if g.nodes.len() > LIMIT {
            return None;
        }
    }
    (!g.nodes.is_empty()).then_some((g, dir))
}

// ---------------------------------------------------------------- mindmaps

fn mind_node(s: &str) -> (String, Shape) {
    let s = s.split(":::").next().unwrap_or(s).trim();
    for (open, close, shape) in [
        ("((", "))", Shape::Circle),
        ("))", "((", Shape::Hexagon),
        ("{{", "}}", Shape::Hexagon),
        ("(", ")", Shape::Round),
        (")", "(", Shape::Round),
        ("[", "]", Shape::Rect),
    ] {
        if let Some(i) = s.find(open) {
            if let Some(inner) = s[i + open.len()..].strip_suffix(close) {
                return (inner.trim().trim_matches('"').to_string(), shape);
            }
        }
    }
    (s.trim_matches('"').to_string(), Shape::Stadium)
}

/// A mindmap, from the whole source: its tree is in the indentation.
pub(super) fn mindmap(source: &str) -> Option<(Graph, Dir)> {
    let mut g = Graph::default();
    let mut stack: Vec<(usize, usize)> = vec![];
    let mut seen_head = false;
    for raw in source.lines() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with("%%") || t.starts_with("::") {
            continue;
        }
        if !seen_head {
            seen_head = t.eq_ignore_ascii_case("mindmap");
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let (label, shape) = mind_node(t);
        let id = format!("m{}", g.nodes.len());
        let i = g.node(&id, Some((label, shape)), None);
        while stack.last().is_some_and(|(ind, _)| *ind >= indent) {
            stack.pop();
        }
        if let Some((_, parent)) = stack.last() {
            g.edges.push(Edge {
                from: *parent,
                to: i,
                label: None,
                line: Line::Solid,
                arrow: false,
                both: false,
            });
        } else if i != 0 {
            // A second root: hang it off the first.
            g.edges.push(Edge {
                from: 0,
                to: i,
                label: None,
                line: Line::Solid,
                arrow: false,
                both: false,
            });
        }
        stack.push((indent, i));
        if g.nodes.len() > LIMIT {
            return None;
        }
    }
    if let Some(root) = g.nodes.first_mut() {
        if root.shape == Shape::Stadium {
            root.shape = Shape::Round;
        }
    }
    (!g.nodes.is_empty()).then_some((g, Dir::Right))
}

// ---------------------------------------------------------------- pie

pub(super) fn pie(head: &str, body: &[&str], p: &Palette) -> Option<String> {
    let mut title = head
        .split_once("title")
        .map(|(_, t)| t.trim().trim_matches('"').to_string());
    let mut show = head.contains("showData");
    let mut slices: Vec<(String, f32)> = vec![];
    for l in body {
        if let Some(t) = title_of(l) {
            title = Some(t);
        } else if l.trim() == "showData" {
            show = true;
        } else if let Some((name, v)) = l.rsplit_once(':') {
            let v: f32 = v.trim().parse().ok()?;
            if v > 0. {
                slices.push((name.trim().trim_matches('"').to_string(), v));
            }
        }
    }
    let total: f32 = slices.iter().map(|s| s.1).sum();
    if slices.is_empty() || slices.len() > LIMIT || total <= 0. {
        return None;
    }
    let r = 100.;
    let legend: Vec<String> = slices
        .iter()
        .map(|(n, v)| {
            let pct = v / total * 100.;
            if show {
                format!("{n}  {} ({pct:.1}%)", num(*v))
            } else {
                format!("{n}  {pct:.1}%")
            }
        })
        .collect();
    let lw = legend
        .iter()
        .map(|l| text_width(l, SIZE))
        .fold(0., f32::max)
        + 22.;
    let w = (M * 2. + r * 2. + 28. + lw).max(
        title
            .as_deref()
            .map_or(0., |t| text_width(t, TITLE) + 2. * M),
    );
    let mut out = String::new();
    let top = M + draw_title(&mut out, &title, w, p);
    let lh = 22.;
    let h = top + (r * 2.).max(lh * slices.len() as f32) + M;
    let (cx, cy) = (M + r, top + r);
    let mut a = -std::f32::consts::FRAC_PI_2;
    for (i, (_, v)) in slices.iter().enumerate() {
        let sweep = v / total * std::f32::consts::TAU;
        let c = color(p, i);
        if slices.len() == 1 {
            let _ = write!(
                out,
                r#"<circle cx="{cx:.1}" cy="{cy:.1}" r="{r}" fill="{c}"/>"#
            );
        } else {
            let (x0, y0) = (cx + r * a.cos(), cy + r * a.sin());
            let (x1, y1) = (cx + r * (a + sweep).cos(), cy + r * (a + sweep).sin());
            let large = u8::from(sweep > std::f32::consts::PI);
            let _ = write!(
                out,
                r#"<path d="M{cx:.1},{cy:.1} L{x0:.1},{y0:.1} A{r},{r} 0 {large} 1 {x1:.1},{y1:.1} Z" fill="{c}" stroke="{}" stroke-width="1.5"/>"#,
                p.node_fill
            );
        }
        a += sweep;
    }
    let lx = M + 2. * r + 28.;
    let ly = cy - lh * slices.len() as f32 / 2. + lh / 2.;
    for (i, l) in legend.iter().enumerate() {
        let y = ly + lh * i as f32;
        rect(&mut out, lx, y - 6., 12., 12., 3., color(p, i), None);
        text(&mut out, lx + 20., y, "start", SIZE, 400, &p.text, l);
    }
    Some(svg(w, h, &out))
}

// ---------------------------------------------------------------- gantt

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

/// `YYYY-MM-DD` (or with `/`), with an optional `HH:mm`, as days.
fn date(s: &str) -> Option<f64> {
    let s = s.trim();
    let (d, time) = s.split_once([' ', 'T']).unwrap_or((s, ""));
    let parts: Vec<i64> = d
        .split(['-', '/'])
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let [y, m, dd] = parts.as_slice() else {
        return None;
    };
    if *y < 1000 || !(1..=12).contains(m) || !(1..=31).contains(dd) {
        return None;
    }
    let mut days = days_from_civil(*y, *m, *dd) as f64;
    if let Some((h, mi)) = time.split_once(':') {
        days +=
            (h.parse::<f64>().ok()? + mi.get(..2).unwrap_or(mi).parse::<f64>().ok()? / 60.) / 24.;
    }
    Some(days)
}

/// `3d`, `2w`, `12h`, as days.
fn duration(s: &str) -> Option<f64> {
    let s = s.trim();
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let (n, unit) = s.split_at(split);
    let n: f64 = n.parse().ok()?;
    let per = match unit {
        "ms" => 1. / 86_400_000.,
        "s" => 1. / 86_400.,
        "m" | "min" => 1. / 1440.,
        "h" => 1. / 24.,
        "d" => 1.,
        "w" => 7.,
        "M" => 30.,
        "y" => 365.,
        _ => return None,
    };
    Some(n * per)
}

struct Task {
    name: String,
    section: usize,
    start: f64,
    end: f64,
    done: bool,
    active: bool,
    crit: bool,
    milestone: bool,
}

pub(super) fn gantt(body: &[&str], p: &Palette) -> Option<String> {
    let mut title = None;
    let mut sections: Vec<String> = vec![];
    let mut tasks: Vec<Task> = vec![];
    let mut ids: HashMap<String, (f64, f64)> = HashMap::new();
    for l in body {
        if let Some(t) = title_of(l) {
            title = Some(t);
            continue;
        }
        if let Some(s) = l.strip_prefix("section") {
            sections.push(s.trim().to_string());
            continue;
        }
        if [
            "dateFormat",
            "axisFormat",
            "excludes",
            "includes",
            "todayMarker",
            "tickInterval",
            "weekday",
            "displayMode",
            "inclusiveEndDates",
            "topAxis",
            "accTitle",
            "accDescr",
            "click",
        ]
        .iter()
        .any(|k| l.starts_with(k))
        {
            continue;
        }
        let Some((name, meta)) = l.split_once(':') else {
            continue;
        };
        let mut parts: Vec<&str> = meta
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let mut flag = |f: &str| {
            let had = parts.contains(&f);
            parts.retain(|p| *p != f);
            had
        };
        let (done, active, crit, milestone) = (
            flag("done"),
            flag("active"),
            flag("crit"),
            flag("milestone"),
        );
        let prev = tasks.last().map(|t| t.end);
        let start_of = |s: &str| -> Option<f64> {
            if let Some(after) = s.strip_prefix("after ") {
                after
                    .split_whitespace()
                    .filter_map(|id| ids.get(id).map(|t| t.1))
                    .reduce(f64::max)
            } else {
                date(s)
            }
        };
        let (id, start, end) = match parts.as_slice() {
            [id, s, e] => (Some(*id), start_of(s), *e),
            [s, e] if start_of(s).is_some() => (None, start_of(s), *e),
            [id, e] => (Some(*id), prev, *e),
            [e] => (None, prev, *e),
            _ => continue,
        };
        let start = start?;
        let end = if let Some(until) = end.strip_prefix("until ") {
            ids.get(until.trim()).map(|t| t.0)?
        } else {
            date(end).or_else(|| duration(end).map(|d| start + d))?
        };
        if let Some(id) = id {
            ids.insert(id.to_string(), (start, end));
        }
        tasks.push(Task {
            name: name.trim().to_string(),
            section: sections.len().saturating_sub(1),
            start,
            end: end.max(start),
            done,
            active,
            crit,
            milestone,
        });
        if tasks.len() > LIMIT {
            return None;
        }
    }
    if tasks.is_empty() {
        return None;
    }
    let t0 = tasks
        .iter()
        .map(|t| t.start)
        .fold(f64::MAX, f64::min)
        .floor();
    let t1 = tasks
        .iter()
        .map(|t| t.end)
        .fold(f64::MIN, f64::max)
        .ceil()
        .max(t0 + 1.);
    let names_w = tasks
        .iter()
        .map(|t| text_width(&t.name, SIZE))
        .fold(0., f32::max)
        + 16.;
    let sec_w = sections
        .iter()
        .map(|s| text_width(s, SIZE))
        .fold(0., f32::max);
    let label_w = names_w.max(sec_w + 12.).min(260.);
    let plot_w = 560.;
    let row = 26.;
    let w = M * 2. + label_w + plot_w;
    let mut out = String::new();
    let top = M + draw_title(&mut out, &title, w, p);
    let axis_h = 22.;
    let mut y = top + axis_h;
    let x0 = M + label_w;
    let scale = plot_w as f64 / (t1 - t0);
    let xs = |d: f64| x0 + ((d - t0) * scale) as f32;
    // Rows, a section heading before each section's first task.
    let mut rows: Vec<(f32, Option<&str>, Option<&Task>)> = vec![];
    let mut last_section = usize::MAX;
    for t in &tasks {
        if t.section != last_section && !sections.is_empty() {
            rows.push((y, sections.get(t.section).map(String::as_str), None));
            y += row;
            last_section = t.section;
        }
        rows.push((y, None, Some(t)));
        y += row;
    }
    let h = y + M;
    // The ticks, at a round step of days.
    let span = t1 - t0;
    let step = [1., 2., 7., 14., 30., 61., 91., 182., 365.]
        .into_iter()
        .find(|s| span / s <= 9.)
        .unwrap_or(730.);
    let mut d = t0;
    while d <= t1 + 1e-6 {
        let x = xs(d);
        line(
            &mut out,
            (x, top + axis_h - 4.),
            (x, y),
            &p.node_stroke,
            0.6,
            true,
        );
        let (yy, mm, dd) = civil_from_days(d as i64);
        let label = if step >= 30. {
            format!("{yy}-{mm:02}")
        } else {
            format!("{mm:02}-{dd:02}")
        };
        text(
            &mut out,
            x,
            top + axis_h / 2. - 2.,
            "middle",
            SMALL,
            400,
            &p.muted,
            &label,
        );
        d += step;
    }
    for (ry, section, task) in rows {
        if let Some(s) = section {
            rect(
                &mut out,
                M,
                ry + 2.,
                w - 2. * M,
                row - 4.,
                4.,
                &p.group_fill,
                None,
            );
            text(
                &mut out,
                M + 8.,
                ry + row / 2.,
                "start",
                SIZE,
                600,
                &p.text,
                s,
            );
            continue;
        }
        let Some(t) = task else { continue };
        text(
            &mut out,
            M + 8.,
            ry + row / 2.,
            "start",
            SIZE,
            400,
            &p.text,
            &t.name,
        );
        let c = if t.done {
            p.node_stroke.as_str()
        } else {
            color(p, t.section)
        };
        let (bx, bw) = (xs(t.start), (xs(t.end) - xs(t.start)).max(3.));
        let cy = ry + row / 2.;
        if t.milestone {
            let _ = write!(
                out,
                r#"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{c}"/>"#,
                bx,
                cy - 7.,
                bx + 7.,
                cy,
                bx,
                cy + 7.,
                bx - 7.,
                cy
            );
        } else {
            let stroke = (t.crit || t.active).then_some(p.text.as_str());
            rect(&mut out, bx, ry + 5., bw, row - 10., 4., c, stroke);
        }
    }
    Some(svg(w, h, &out))
}

// ---------------------------------------------------------------- timeline

pub(super) fn timeline(body: &[&str], p: &Palette) -> Option<String> {
    let mut title = None;
    let mut sections: Vec<String> = vec![];
    // (section, period, events)
    let mut periods: Vec<(Option<usize>, String, Vec<String>)> = vec![];
    for l in body {
        if let Some(t) = title_of(l) {
            title = Some(t);
        } else if let Some(s) = l.strip_prefix("section") {
            sections.push(s.trim().to_string());
        } else if let Some(ev) = l.strip_prefix(':') {
            let last = periods.last_mut()?;
            last.2.extend(
                ev.split(':')
                    .map(|e| e.trim().to_string())
                    .filter(|e| !e.is_empty()),
            );
        } else {
            let mut parts = l.split(':').map(str::trim);
            let period = parts.next()?.to_string();
            let events = parts.filter(|e| !e.is_empty()).map(String::from).collect();
            periods.push((sections.len().checked_sub(1), period, events));
        }
        if periods.len() > LIMIT {
            return None;
        }
    }
    if periods.is_empty() {
        return None;
    }
    let col_w = periods
        .iter()
        .flat_map(|(_, p, e)| std::iter::once(p).chain(e))
        .map(|s| text_width(s, SIZE).min(200.))
        .fold(110., f32::max)
        + 20.;
    let gap = 12.;
    let w = M * 2. + periods.len() as f32 * (col_w + gap) - gap;
    let mut out = String::new();
    let mut top = M + draw_title(&mut out, &title, w, p);
    if !sections.is_empty() {
        top += 28.;
    }
    let ph = 32.;
    let axis = top + ph + 18.;
    let max_events = periods.iter().map(|p| p.2.len()).max().unwrap_or(0) as f32;
    let ev_h = 30.;
    let h = axis + 18. + max_events * (ev_h + 8.) + M;
    line(
        &mut out,
        (M, axis),
        (w - M, axis),
        &p.node_stroke,
        1.5,
        false,
    );
    // Section bands over their periods.
    for (si, s) in sections.iter().enumerate() {
        let cols: Vec<usize> = periods
            .iter()
            .enumerate()
            .filter(|(_, x)| x.0 == Some(si))
            .map(|(i, _)| i)
            .collect();
        let (Some(a), Some(b)) = (cols.first(), cols.last()) else {
            continue;
        };
        let x = M + *a as f32 * (col_w + gap);
        let bw = (*b - *a + 1) as f32 * (col_w + gap) - gap;
        rect(&mut out, x, top - 26., bw, 20., 4., &p.group_fill, None);
        text(
            &mut out,
            x + bw / 2.,
            top - 16.,
            "middle",
            SMALL,
            600,
            &p.text,
            s,
        );
    }
    for (i, (sec, period, events)) in periods.iter().enumerate() {
        let x = M + i as f32 * (col_w + gap);
        let cx = x + col_w / 2.;
        let c = color(p, sec.unwrap_or(i));
        rect(&mut out, x, top, col_w, ph, 6., c, None);
        text(
            &mut out,
            cx,
            top + ph / 2.,
            "middle",
            SIZE,
            600,
            &p.node_fill,
            period,
        );
        line(
            &mut out,
            (cx, top + ph),
            (cx, axis + 10. + events.len() as f32 * (ev_h + 8.) - 8.),
            c,
            1.2,
            true,
        );
        let _ = write!(
            out,
            r#"<circle cx="{cx:.1}" cy="{axis:.1}" r="4.5" fill="{c}"/>"#
        );
        for (j, e) in events.iter().enumerate() {
            let ey = axis + 14. + j as f32 * (ev_h + 8.);
            rect(
                &mut out,
                x + 4.,
                ey,
                col_w - 8.,
                ev_h,
                6.,
                &p.node_fill,
                Some(&p.node_stroke),
            );
            let shown = if text_width(e, SIZE) > col_w - 16. {
                let mut s: String = e
                    .chars()
                    .take(((col_w - 24.) / (SIZE * 0.55)) as usize)
                    .collect();
                s.push('…');
                s
            } else {
                e.clone()
            };
            text(
                &mut out,
                cx,
                ey + ev_h / 2.,
                "middle",
                SIZE,
                400,
                &p.text,
                &shown,
            );
        }
    }
    Some(svg(w, h, &out))
}

// ---------------------------------------------------------------- user journey

pub(super) fn journey(body: &[&str], p: &Palette) -> Option<String> {
    let mut title = None;
    let mut sections: Vec<String> = vec![];
    // (section, task, score, actors)
    let mut tasks: Vec<(usize, String, f32, String)> = vec![];
    for l in body {
        if let Some(t) = title_of(l) {
            title = Some(t);
        } else if let Some(s) = l.strip_prefix("section") {
            sections.push(s.trim().to_string());
        } else {
            let mut parts = l.split(':').map(str::trim);
            let (Some(name), Some(score)) = (parts.next(), parts.next()) else {
                continue;
            };
            let score: f32 = score.parse().ok()?;
            tasks.push((
                sections.len().saturating_sub(1),
                name.to_string(),
                score.clamp(0., 5.),
                parts.next().unwrap_or("").to_string(),
            ));
        }
        if tasks.len() > LIMIT {
            return None;
        }
    }
    if tasks.is_empty() {
        return None;
    }
    let col = tasks
        .iter()
        .map(|t| text_width(&t.1, SMALL).min(150.))
        .fold(90., f32::max)
        + 16.;
    let axis_w = 26.;
    let w = M * 2. + axis_w + col * tasks.len() as f32;
    let mut out = String::new();
    let mut top = M + draw_title(&mut out, &title, w, p);
    if !sections.is_empty() {
        top += 26.;
    }
    let plot_h = 150.;
    let x0 = M + axis_w;
    let ys = |s: f32| top + plot_h - (s - 1.).max(0.) / 4. * (plot_h - 20.) - 10.;
    for s in 1..=5 {
        let y = ys(s as f32);
        line(&mut out, (x0, y), (w - M, y), &p.node_stroke, 0.6, true);
        text(
            &mut out,
            x0 - 8.,
            y,
            "end",
            SMALL,
            400,
            &p.muted,
            &s.to_string(),
        );
    }
    for (si, s) in sections.iter().enumerate() {
        let cols: Vec<usize> = tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.0 == si)
            .map(|(i, _)| i)
            .collect();
        let (Some(a), Some(b)) = (cols.first(), cols.last()) else {
            continue;
        };
        let x = x0 + *a as f32 * col + 2.;
        let bw = (*b - *a + 1) as f32 * col - 4.;
        rect(&mut out, x, top - 24., bw, 20., 4., color(p, si), None);
        text(
            &mut out,
            x + bw / 2.,
            top - 14.,
            "middle",
            SMALL,
            600,
            &p.node_fill,
            s,
        );
    }
    let mut prev: Option<(f32, f32)> = None;
    let mut dots = String::new();
    for (i, (si, name, score, actors)) in tasks.iter().enumerate() {
        let cx = x0 + (i as f32 + 0.5) * col;
        let cy = ys(*score);
        if let Some(a) = prev {
            line(&mut out, a, (cx, cy), &p.edge, 1.5, false);
        }
        prev = Some((cx, cy));
        let _ = write!(
            dots,
            r#"<circle cx="{cx:.1}" cy="{cy:.1}" r="11" fill="{}" stroke="{}" stroke-width="2"/>"#,
            p.node_fill,
            color(p, *si)
        );
        text(
            &mut dots,
            cx,
            cy,
            "middle",
            SMALL,
            600,
            &p.text,
            &num(*score),
        );
        for (j, l) in label_lines(name).iter().take(2).enumerate() {
            text(
                &mut out,
                cx,
                top + plot_h + 14. + j as f32 * 14.,
                "middle",
                SMALL,
                500,
                &p.text,
                l,
            );
        }
        if !actors.is_empty() {
            text(
                &mut out,
                cx,
                top + plot_h + 44.,
                "middle",
                SMALL,
                400,
                &p.muted,
                actors,
            );
        }
    }
    out.push_str(&dots);
    let h = top + plot_h + 56. + M;
    Some(svg(w, h, &out))
}

// ---------------------------------------------------------------- xychart

/// `[a, "b c", 3]` as strings.
fn list(s: &str) -> Option<Vec<String>> {
    let inner = s.trim().strip_prefix('[')?.strip_suffix(']')?;
    Some(
        inner
            .split(',')
            .map(|v| v.trim().trim_matches('"').to_string())
            .filter(|v| !v.is_empty())
            .collect(),
    )
}

/// An axis line: its title, then `[categories]` or `min --> max`.
fn axis(rest: &str) -> (Option<String>, Option<Vec<String>>, Option<(f32, f32)>) {
    let rest = rest.trim();
    let (title, rest) = if let Some(r) = rest.strip_prefix('"') {
        match r.split_once('"') {
            Some((t, r)) => (Some(t.to_string()), r.trim()),
            None => (None, rest),
        }
    } else if !rest.starts_with('[') && !rest.contains("-->") {
        (Some(rest.to_string()), "")
    } else if let Some(i) = rest.find('[') {
        let t = rest[..i].trim();
        ((!t.is_empty()).then(|| t.to_string()), &rest[i..])
    } else {
        (None, rest)
    };
    if let Some(cats) = list(rest) {
        return (title, Some(cats), None);
    }
    if let Some((a, b)) = rest.split_once("-->") {
        let a = a.split_whitespace().last().and_then(|v| v.parse().ok());
        let b = b.trim().parse().ok();
        if let (Some(a), Some(b)) = (a, b) {
            return (title, None, Some((a, b)));
        }
    }
    (title, None, None)
}

pub(super) fn xychart(body: &[&str], p: &Palette) -> Option<String> {
    let mut title = None;
    let (mut x_title, mut cats, mut y_title, mut y_range) = (None, None, None, None);
    let mut series: Vec<(bool, Vec<f32>)> = vec![];
    for l in body {
        if let Some(t) = title_of(l) {
            title = Some(t);
        } else if let Some(r) = l.strip_prefix("x-axis") {
            let (t, c, _) = axis(r);
            x_title = t;
            cats = c;
        } else if let Some(r) = l.strip_prefix("y-axis") {
            let (t, _, range) = axis(r);
            y_title = t;
            y_range = range;
        } else if let Some(r) = l.strip_prefix("bar").or_else(|| l.strip_prefix("line")) {
            let is_bar = l.starts_with("bar");
            let r = r.find('[').map_or(r, |i| &r[i..]);
            let vals: Vec<f32> = list(r)?
                .iter()
                .map(|v| v.parse().ok())
                .collect::<Option<_>>()?;
            series.push((is_bar, vals));
        }
    }
    let n = series.iter().map(|s| s.1.len()).max()?;
    if n == 0 || n > LIMIT {
        return None;
    }
    let cats = cats.unwrap_or_else(|| (1..=n).map(|i| i.to_string()).collect());
    let all = series.iter().flat_map(|s| s.1.iter().copied());
    let (lo, hi) = all.fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(v), b.max(v)));
    let (lo, hi) = y_range.unwrap_or((lo.min(0.), hi));
    let (lo, hi, step) = nice(lo, hi.max(lo + 1e-3), 5.);
    let plot_h = 240.;
    let slot = cats
        .iter()
        .map(|c| text_width(c, SMALL) + 12.)
        .fold(36., f32::max)
        .max(560. / n as f32);
    let y_label_w = [lo, hi]
        .iter()
        .map(|v| text_width(&num(*v), SMALL))
        .fold(0., f32::max)
        + 10.;
    let x0 = M + y_title.as_ref().map_or(0., |_| 18.) + y_label_w;
    let w = x0 + slot * n as f32 + M;
    let mut out = String::new();
    let top = M + draw_title(&mut out, &title, w, p) + 6.;
    let ys = |v: f32| top + plot_h - (v - lo) / (hi - lo) * plot_h;
    let mut v = lo;
    while v <= hi + step * 0.01 {
        let y = ys(v);
        line(&mut out, (x0, y), (w - M, y), &p.node_stroke, 0.6, v != lo);
        text(&mut out, x0 - 6., y, "end", SMALL, 400, &p.muted, &num(v));
        v += step;
    }
    let bars: Vec<&Vec<f32>> = series.iter().filter(|s| s.0).map(|s| &s.1).collect();
    let bw = (slot * 0.7 / bars.len().max(1) as f32).min(48.);
    for (bi, vals) in bars.iter().enumerate() {
        for (i, v) in vals.iter().enumerate() {
            let x = x0 + slot * (i as f32 + 0.5) - bw * bars.len() as f32 / 2. + bw * bi as f32;
            let (y, base) = (ys(*v), ys(lo.max(0.).min(hi)));
            rect(
                &mut out,
                x + 1.,
                y.min(base),
                bw - 2.,
                (base - y).abs(),
                3.,
                color(p, bi),
                None,
            );
        }
    }
    for (li, (_, vals)) in series.iter().filter(|s| !s.0).enumerate() {
        let c = color(p, bars.len() + li);
        let pts: Vec<String> = vals
            .iter()
            .enumerate()
            .map(|(i, v)| format!("{:.1},{:.1}", x0 + slot * (i as f32 + 0.5), ys(*v)))
            .collect();
        let _ = write!(
            out,
            r#"<polyline points="{}" fill="none" stroke="{c}" stroke-width="2.2" stroke-linejoin="round"/>"#,
            pts.join(" ")
        );
        for (i, v) in vals.iter().enumerate() {
            let _ = write!(
                out,
                r#"<circle cx="{:.1}" cy="{:.1}" r="3.5" fill="{c}"/>"#,
                x0 + slot * (i as f32 + 0.5),
                ys(*v)
            );
        }
    }
    for (i, c) in cats.iter().enumerate() {
        text(
            &mut out,
            x0 + slot * (i as f32 + 0.5),
            top + plot_h + 12.,
            "middle",
            SMALL,
            400,
            &p.text,
            c,
        );
    }
    let mut h = top + plot_h + 26.;
    if let Some(t) = x_title {
        text(
            &mut out,
            x0 + slot * n as f32 / 2.,
            h + 4.,
            "middle",
            SMALL,
            600,
            &p.muted,
            &t,
        );
        h += 18.;
    }
    if let Some(t) = y_title {
        let (x, y) = (M + 6., top + plot_h / 2.);
        let _ = write!(
            out,
            r#"<text x="{x:.1}" y="{y:.1}" transform="rotate(-90 {x:.1} {y:.1})" text-anchor="middle" dominant-baseline="central" font-family="{FONT}" font-size="{SMALL}" font-weight="600" fill="{}">{}</text>"#,
            p.muted,
            esc(&t)
        );
    }
    Some(svg(w, h + M, &out))
}

// ---------------------------------------------------------------- quadrant chart

pub(super) fn quadrant(body: &[&str], p: &Palette) -> Option<String> {
    let mut title = None;
    let mut x_axis = (String::new(), String::new());
    let mut y_axis = (String::new(), String::new());
    let mut quads = [String::new(), String::new(), String::new(), String::new()];
    let mut points: Vec<(String, f32, f32)> = vec![];
    let ends = |r: &str| -> (String, String) {
        let (a, b) = r.split_once("-->").unwrap_or((r, ""));
        (
            a.trim().trim_matches('"').to_string(),
            b.trim().trim_matches('"').to_string(),
        )
    };
    for l in body {
        if let Some(t) = title_of(l) {
            title = Some(t);
        } else if let Some(r) = l.strip_prefix("x-axis") {
            x_axis = ends(r);
        } else if let Some(r) = l.strip_prefix("y-axis") {
            y_axis = ends(r);
        } else if let Some(r) = l.strip_prefix("quadrant-") {
            let (n, t) = r.split_once(char::is_whitespace)?;
            let i: usize = n.parse().ok()?;
            *quads.get_mut(i.checked_sub(1)?)? = t.trim().trim_matches('"').to_string();
        } else if let Some((name, at)) = l.split_once(':') {
            let at = at.split(":::").next().unwrap_or(at);
            let xy = list(
                at.trim()
                    .split(']')
                    .next()
                    .map(|s| format!("{s}]"))
                    .as_deref()
                    .unwrap_or(""),
            )?;
            if let [x, y] = xy.as_slice() {
                points.push((
                    name.trim().trim_matches('"').to_string(),
                    x.parse().ok()?,
                    y.parse().ok()?,
                ));
            }
        }
    }
    let size = 380.;
    let x0 = M + 24.;
    let w = x0 + size + M;
    let mut out = String::new();
    let top = M + draw_title(&mut out, &title, w, p);
    let half = size / 2.;
    // Quadrant 1 is top right, then counter-clockwise.
    let cells = [
        (x0 + half, top),
        (x0, top),
        (x0, top + half),
        (x0 + half, top + half),
    ];
    for (i, (cx, cy)) in cells.iter().enumerate() {
        let fill = if i % 2 == 0 {
            &p.accent_tint
        } else {
            &p.group_fill
        };
        rect(&mut out, *cx, *cy, half, half, 0., fill, None);
        text(
            &mut out,
            cx + half / 2.,
            cy + 18.,
            "middle",
            SIZE,
            600,
            &p.text,
            &quads[i],
        );
    }
    rect(
        &mut out,
        x0,
        top,
        size,
        size,
        4.,
        "none",
        Some(&p.node_stroke),
    );
    line(
        &mut out,
        (x0 + half, top),
        (x0 + half, top + size),
        &p.node_stroke,
        1.,
        false,
    );
    line(
        &mut out,
        (x0, top + half),
        (x0 + size, top + half),
        &p.node_stroke,
        1.,
        false,
    );
    let mut labels = String::new();
    for (name, x, y) in &points {
        let (px, py) = (
            x0 + x.clamp(0., 1.) * size,
            top + (1. - y.clamp(0., 1.)) * size,
        );
        let _ = write!(
            out,
            r#"<circle cx="{px:.1}" cy="{py:.1}" r="5" fill="{}"/>"#,
            p.accent
        );
        text(
            &mut labels,
            px,
            py + 13.,
            "middle",
            SMALL,
            500,
            &p.text,
            name,
        );
    }
    out.push_str(&labels);
    let by = top + size + 14.;
    text(
        &mut out,
        x0 + 4.,
        by,
        "start",
        SMALL,
        400,
        &p.muted,
        &x_axis.0,
    );
    text(
        &mut out,
        x0 + size - 4.,
        by,
        "end",
        SMALL,
        400,
        &p.muted,
        &x_axis.1,
    );
    for (t, y) in [(&y_axis.0, top + size - 4.), (&y_axis.1, top + 4.)] {
        let x = M + 8.;
        let anchor = if y > top + half { "start" } else { "end" };
        let _ = write!(
            out,
            r#"<text x="{x:.1}" y="{y:.1}" transform="rotate(-90 {x:.1} {y:.1})" text-anchor="{anchor}" dominant-baseline="central" font-family="{FONT}" font-size="{SMALL}" fill="{}">{}</text>"#,
            p.muted,
            esc(t)
        );
    }
    Some(svg(w, by + 10. + M, &out))
}

// ---------------------------------------------------------------- git graph

/// `key: "value"` from a git graph command's options.
fn opt(l: &str, key: &str) -> Option<String> {
    let i = l.find(&format!("{key}:"))?;
    let r = l[i + key.len() + 1..].trim_start();
    if let Some(r) = r.strip_prefix('"') {
        r.split_once('"').map(|(v, _)| v.to_string())
    } else {
        r.split_whitespace().next().map(String::from)
    }
}

pub(super) fn gitgraph(body: &[&str], p: &Palette) -> Option<String> {
    struct Commit {
        branch: usize,
        id: Option<String>,
        tag: Option<String>,
        parents: Vec<usize>,
    }
    let mut branches: Vec<String> = vec!["main".into()];
    let mut heads: Vec<Option<usize>> = vec![None];
    let mut cur = 0;
    let mut commits: Vec<Commit> = vec![];
    for l in body {
        let word = l.split_whitespace().next().unwrap_or("");
        let arg = l[word.len()..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches('"')
            .to_string();
        match word {
            "commit" => {
                commits.push(Commit {
                    branch: cur,
                    id: opt(l, "id"),
                    tag: opt(l, "tag"),
                    parents: heads[cur].into_iter().collect(),
                });
                heads[cur] = Some(commits.len() - 1);
            }
            "branch" => {
                let base = heads[cur];
                branches.push(arg);
                heads.push(base);
                cur = branches.len() - 1;
            }
            "checkout" | "switch" => {
                cur = branches.iter().position(|b| *b == arg)?;
            }
            "merge" => {
                let other = branches.iter().position(|b| *b == arg)?;
                let parents = heads[cur].into_iter().chain(heads[other]).collect();
                commits.push(Commit {
                    branch: cur,
                    id: opt(l, "id"),
                    tag: opt(l, "tag"),
                    parents,
                });
                heads[cur] = Some(commits.len() - 1);
            }
            "cherry-pick" => {
                commits.push(Commit {
                    branch: cur,
                    id: opt(l, "id"),
                    tag: Some("cherry-pick".into()),
                    parents: heads[cur].into_iter().collect(),
                });
                heads[cur] = Some(commits.len() - 1);
            }
            _ => {}
        }
        if commits.len() > LIMIT {
            return None;
        }
    }
    if commits.is_empty() {
        return None;
    }
    let names_w = branches
        .iter()
        .map(|b| text_width(b, SMALL))
        .fold(0., f32::max)
        + 24.;
    let (dx, lane) = (52., 44.);
    let x0 = M + names_w + 16.;
    let w = x0 + dx * (commits.len() as f32 - 1.) + 40. + M;
    let top = M + 12.;
    let h = top + lane * branches.len() as f32 + 16. + M;
    let pos = |i: usize| {
        (
            x0 + dx * i as f32,
            top + lane * commits[i].branch as f32 + lane / 2.,
        )
    };
    let mut out = String::new();
    for (b, name) in branches.iter().enumerate() {
        let y = top + lane * b as f32 + lane / 2.;
        let c = color(p, b);
        let mine: Vec<usize> = (0..commits.len())
            .filter(|i| commits[*i].branch == b)
            .collect();
        if let (Some(a), Some(z)) = (mine.first(), mine.last()) {
            line(&mut out, (pos(*a).0, y), (pos(*z).0, y), c, 3., false);
        }
        let tw = text_width(name, SMALL) + 14.;
        rect(&mut out, M, y - 10., tw, 20., 10., c, None);
        text(
            &mut out,
            M + tw / 2.,
            y,
            "middle",
            SMALL,
            600,
            &p.node_fill,
            name,
        );
    }
    for (i, c) in commits.iter().enumerate() {
        let (x, y) = pos(i);
        for &pi in &c.parents {
            let (px, py) = pos(pi);
            if py == y {
                continue;
            }
            let col = color(
                p,
                if commits[pi].branch == c.branch {
                    c.branch
                } else {
                    commits[pi].branch.max(c.branch)
                },
            );
            let _ = write!(
                out,
                r#"<path d="M{px:.1},{py:.1} C{:.1},{py:.1} {:.1},{y:.1} {x:.1},{y:.1}" fill="none" stroke="{col}" stroke-width="3"/>"#,
                px + dx * 0.6,
                x - dx * 0.6
            );
        }
    }
    for (i, c) in commits.iter().enumerate() {
        let (x, y) = pos(i);
        let r = if c.parents.len() > 1 { 8. } else { 7. };
        let _ = write!(
            out,
            r#"<circle cx="{x:.1}" cy="{y:.1}" r="{r}" fill="{}" stroke="{}" stroke-width="3"/>"#,
            p.node_fill,
            color(p, c.branch)
        );
        if let Some(id) = &c.id {
            text(
                &mut out,
                x,
                y + 17.,
                "middle",
                SMALL - 1.5,
                400,
                &p.muted,
                id,
            );
        }
        if let Some(tag) = &c.tag {
            let tw = text_width(tag, SMALL) + 12.;
            rect(
                &mut out,
                x - tw / 2.,
                y - 31.,
                tw,
                18.,
                4.,
                &p.note_fill,
                Some(&p.note_stroke),
            );
            text(&mut out, x, y - 22., "middle", SMALL, 500, &p.text, tag);
        }
    }
    Some(svg(w, h, &out))
}

#[cfg(test)]
mod tests {
    use super::super::{render, Palette};

    fn draws(src: &str) {
        let svg = render(src, &Palette::test()).unwrap_or_else(|| panic!("not drawn:\n{src}"));
        assert!(crate::diagram::view_size(&svg).is_some(), "{svg}");
        assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
    }

    /// One source per kind; `MERMAID_DUMP=dir cargo test dump_chart_samples -- --ignored`
    /// writes them as SVGs.
    const SAMPLES: &[(&str, &str)] = &[
        ("class", "classDiagram\n  direction LR\n  class Animal {\n    +String name\n    +eat() void\n  }\n  <<interface>> Pet\n  Animal <|-- Duck\n  Animal <|-- Fish\n  Duck : +swim()\n  Owner \"1\" --> \"*\" Pet : owns\n  Zoo o-- Animal\n  Pet ..|> Animal"),
        ("er", "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n  ORDER ||--|{ LINE-ITEM : contains\n  CUSTOMER {\n    string name PK\n    string email \"login\"\n  }\n  PRODUCT }|..|{ LINE-ITEM : \"ordered in\""),
        ("mindmap", "mindmap\n  root((Forge))\n    Chat\n      Diagrams\n      Files\n    Settings\n      Models\n      [Plugins]"),
        ("pie", "pie showData title Where time goes\n  \"Reading\" : 42\n  \"Writing\" : 30.5\n  \"Tests\" : 12"),
        ("gantt", "gantt\n  title Release\n  dateFormat YYYY-MM-DD\n  section Build\n  Design :done, d1, 2026-01-05, 7d\n  Code :active, c1, after d1, 14d\n  section Ship\n  Review :crit, 2026-01-26, 2026-02-02\n  Launch :milestone, after c1, 0d\n  Docs :5d"),
        ("timeline", "timeline\n  title Forge\n  section 2025\n    Q3 : Prototype : First chat\n    Q4 : Diagrams\n  section 2026\n    Q1 : Plugins\n       : Office files"),
        ("journey", "journey\n  title A first chat\n  section Start\n    Install: 4: Me\n    Set up a model: 2: Me, Forge\n  section Use\n    Ask: 5: Me"),
        ("xychart", "xychart-beta\n  title \"Sales\"\n  x-axis [jan, feb, mar, apr]\n  y-axis \"Revenue (k)\" 0 --> 100\n  bar [20, 45, 60, 80]\n  line [25, 40, 70, 90]"),
        ("quadrant", "quadrantChart\n  title Reach and engagement\n  x-axis Low Reach --> High Reach\n  y-axis Low Engagement --> High Engagement\n  quadrant-1 Expand\n  quadrant-2 Promote\n  quadrant-3 Re-evaluate\n  quadrant-4 Improve\n  Campaign A: [0.3, 0.6]\n  Campaign B: [0.45, 0.23]"),
        ("gitgraph", "gitGraph\n  commit id: \"init\"\n  branch develop\n  checkout develop\n  commit\n  commit tag: \"v0.1\"\n  checkout main\n  merge develop\n  commit"),
    ];

    #[test]
    fn every_chart_kind_draws() {
        for (_, src) in SAMPLES {
            draws(src);
        }
    }

    #[test]
    #[ignore]
    fn dump_chart_samples() {
        let Some(dir) = std::env::var_os("MERMAID_DUMP") else {
            return;
        };
        for (name, src) in SAMPLES {
            let svg = render(src, &Palette::test()).unwrap();
            std::fs::write(std::path::Path::new(&dir).join(format!("{name}.svg")), svg).unwrap();
        }
    }

    #[test]
    fn class_relationships_read_both_ways() {
        let (e, _) = super::er(&["A }|..o{ B : has"]).unwrap();
        assert_eq!(e.edges[0].label.as_ref().unwrap()[1], "1..N : 0..N");
        let (g, _) = super::class(&["Animal <|-- Duck", "Zoo o-- Animal", "A -- B"]).unwrap();
        let id = |i: usize| g.nodes[i].id.as_str();
        // Duck inherits Animal: the arrow points at Animal.
        assert_eq!((id(g.edges[0].from), id(g.edges[0].to)), ("Duck", "Animal"));
        assert!(g.edges[0].arrow && !g.edges[2].arrow);
        assert_eq!(g.nodes.len(), 5);
    }

    #[test]
    fn gantt_dates_round_trip() {
        for d in [-1000, 0, 19000, 20500] {
            let (y, m, dd) = super::civil_from_days(d);
            assert_eq!(super::days_from_civil(y, m, dd), d);
        }
        assert_eq!(super::date("1970-01-02"), Some(1.));
        assert_eq!(super::duration("2w"), Some(14.));
    }
}
