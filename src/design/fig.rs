//! A Figma file (`.fig`) opened as a design page: its pages, frames,
//! shapes, text and pictures rebuilt as HTML, which the panel shows and a
//! design chat can work on (`docs/open-design-port.md` D4).
//!
//! A `.fig` is "fig-kiwi": a header, then chunks, each compressed (zstd, or
//! raw deflate in older files). The first chunk is a Kiwi schema (Evan
//! Wallace's binary format: definitions of enums, structs and messages);
//! the second is the document, a `Message` in that schema, whose
//! `nodeChanges` are every node with its parent, place, size and look.
//! Figma's exported `.fig` is often a zip holding `canvas.fig` and the
//! pictures under `images/`. Everything here is read generically through
//! the schema the file carries, so fields this code doesn't use are skipped
//! safely. Vector paths aren't drawn: a vector shows as its box.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use serde_json::{Map, Value};

// ---------------------------------------------------------------- the container

/// The document chunks of a `.fig` (decompressed), and the pictures beside
/// it in a zipped one, by their hash's hex.
pub struct FigFile {
    pub schema: Vec<u8>,
    pub data: Vec<u8>,
    pub images: HashMap<String, Vec<u8>>,
}

fn inflate(chunk: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = vec![];
    if chunk.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        let mut d = ruzstd::decoding::StreamingDecoder::new(chunk).map_err(|e| e.to_string())?;
        d.read_to_end(&mut out).map_err(|e| e.to_string())?;
        return Ok(out);
    }
    if flate2::read::DeflateDecoder::new(chunk)
        .read_to_end(&mut out)
        .is_ok()
        && !out.is_empty()
    {
        return Ok(out);
    }
    out.clear();
    flate2::read::ZlibDecoder::new(chunk)
        .read_to_end(&mut out)
        .map_err(|e| format!("a chunk couldn't be decompressed: {e}"))?;
    Ok(out)
}

/// Read a `.fig` (zipped or not).
pub fn read(bytes: &[u8]) -> Result<FigFile, String> {
    if bytes.starts_with(b"PK") {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        let mut canvas = vec![];
        z.by_name("canvas.fig")
            .map_err(|_| "this zip has no canvas.fig: is it a Figma file?".to_string())?
            .read_to_end(&mut canvas)
            .map_err(|e| e.to_string())?;
        let mut fig = read(&canvas)?;
        let names: Vec<String> = z.file_names().map(str::to_string).collect();
        for name in names.iter().filter(|n| n.starts_with("images/")) {
            let mut b = vec![];
            if z.by_name(name)
                .and_then(|mut f| Ok(f.read_to_end(&mut b)?))
                .is_ok()
            {
                fig.images
                    .insert(name.trim_start_matches("images/").to_lowercase(), b);
            }
        }
        return Ok(fig);
    }
    let rest = bytes
        .strip_prefix(b"fig-kiwi")
        .or_else(|| bytes.strip_prefix(b"fig-jam."))
        .ok_or("not a Figma file (no fig-kiwi header)")?;
    let mut at = 4; // the version
    let mut chunks = vec![];
    while at + 4 <= rest.len() {
        let len = u32::from_le_bytes(rest[at..at + 4].try_into().expect("4 bytes")) as usize;
        at += 4;
        let end = (at + len).min(rest.len());
        chunks.push(&rest[at..end]);
        at = end;
    }
    let [schema, data, ..] = chunks.as_slice() else {
        return Err("the Figma file is cut short".into());
    };
    Ok(FigFile {
        schema: inflate(schema)?,
        data: inflate(data)?,
        images: HashMap::new(),
    })
}

// ---------------------------------------------------------------- Kiwi

struct Buf<'a> {
    b: &'a [u8],
    at: usize,
}

impl Buf<'_> {
    fn byte(&mut self) -> Result<u8, String> {
        let v = *self.b.get(self.at).ok_or("the data ends early")?;
        self.at += 1;
        Ok(v)
    }

    fn var_u64(&mut self) -> Result<u64, String> {
        let (mut v, mut shift) = (0u64, 0);
        loop {
            let b = self.byte()?;
            v |= u64::from(b & 127) << shift;
            shift += 7;
            if b & 128 == 0 || shift >= 64 {
                return Ok(v);
            }
        }
    }

    fn var_u32(&mut self) -> Result<u32, String> {
        let (mut v, mut shift) = (0u32, 0);
        loop {
            let b = self.byte()?;
            v |= u32::from(b & 127) << shift;
            shift += 7;
            if b & 128 == 0 || shift >= 35 {
                return Ok(v);
            }
        }
    }

    fn var_i32(&mut self) -> Result<i32, String> {
        let v = self.var_u32()?;
        Ok(if v & 1 != 0 {
            !((v >> 1) as i32)
        } else {
            (v >> 1) as i32
        })
    }

    fn var_i64(&mut self) -> Result<i64, String> {
        let v = self.var_u64()?;
        Ok(if v & 1 != 0 {
            !((v >> 1) as i64)
        } else {
            (v >> 1) as i64
        })
    }

    fn var_f32(&mut self) -> Result<f32, String> {
        let first = self.byte()?;
        if first == 0 {
            return Ok(0.0);
        }
        let bits = u32::from(first)
            | u32::from(self.byte()?) << 8
            | u32::from(self.byte()?) << 16
            | u32::from(self.byte()?) << 24;
        Ok(f32::from_bits(bits.rotate_left(23)))
    }

    fn string(&mut self) -> Result<String, String> {
        let start = self.at;
        while self.byte()? != 0 {}
        Ok(String::from_utf8_lossy(&self.b[start..self.at - 1]).into_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DefKind {
    Enum,
    Struct,
    Message,
}

#[derive(Debug)]
struct Field {
    name: String,
    ty: i32,
    array: bool,
    value: u32,
}

#[derive(Debug)]
struct Def {
    name: String,
    kind: DefKind,
    fields: Vec<Field>,
}

pub struct Schema {
    defs: Vec<Def>,
}

/// A Kiwi binary schema.
pub fn schema(bytes: &[u8]) -> Result<Schema, String> {
    let mut b = Buf { b: bytes, at: 0 };
    let n = b.var_u32()?;
    let mut defs = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let name = b.string()?;
        let kind = match b.byte()? {
            0 => DefKind::Enum,
            1 => DefKind::Struct,
            2 => DefKind::Message,
            k => return Err(format!("unknown kind {k} in the schema")),
        };
        let count = b.var_u32()?;
        let mut fields = Vec::with_capacity(count as usize);
        for _ in 0..count {
            fields.push(Field {
                name: b.string()?,
                ty: b.var_i32()?,
                array: b.byte()? & 1 == 1,
                value: b.var_u32()?,
            });
        }
        defs.push(Def { name, kind, fields });
    }
    Ok(Schema { defs })
}

impl Schema {
    /// Decode a value of definition `root` (by name) from `bytes`, as JSON.
    pub fn decode(&self, root: &str, bytes: &[u8]) -> Result<Value, String> {
        let i = self
            .defs
            .iter()
            .position(|d| d.name == root)
            .ok_or_else(|| format!("the schema has no {root}"))?;
        let mut b = Buf { b: bytes, at: 0 };
        self.def(i, &mut b, 0)
    }

    fn value(&self, ty: i32, b: &mut Buf, depth: usize) -> Result<Value, String> {
        Ok(match ty {
            -1 => Value::Bool(b.byte()? != 0),
            -2 => Value::from(b.byte()?),
            -3 => Value::from(b.var_i32()?),
            -4 => Value::from(b.var_u32()?),
            -5 => Value::from(f64::from(b.var_f32()?)),
            -6 => Value::String(b.string()?),
            -7 => Value::from(b.var_i64()?),
            -8 => Value::from(b.var_u64()?),
            t if t >= 0 && (t as usize) < self.defs.len() => self.def(t as usize, b, depth + 1)?,
            t => return Err(format!("unknown type {t}")),
        })
    }

    fn field(&self, f: &Field, b: &mut Buf, depth: usize) -> Result<Value, String> {
        if !f.array {
            return self.value(f.ty, b, depth);
        }
        let n = b.var_u32()? as usize;
        if f.ty == -2 {
            // Bytes: raw.
            let end =
                b.at.checked_add(n)
                    .filter(|e| *e <= b.b.len())
                    .ok_or("the data ends early")?;
            let v = b.b[b.at..end].to_vec();
            b.at = end;
            return Ok(Value::Array(v.into_iter().map(Value::from).collect()));
        }
        if n > b.b.len() {
            return Err("an array longer than the data".into());
        }
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.value(f.ty, b, depth)?);
        }
        Ok(Value::Array(out))
    }

    fn def(&self, i: usize, b: &mut Buf, depth: usize) -> Result<Value, String> {
        if depth > 64 {
            return Err("the data nests too deep".into());
        }
        let d = &self.defs[i];
        match d.kind {
            DefKind::Enum => {
                let v = b.var_u32()?;
                Ok(d.fields
                    .iter()
                    .find(|f| f.value == v)
                    .map_or(Value::from(v), |f| Value::String(f.name.clone())))
            }
            DefKind::Struct => {
                let mut m = Map::new();
                for f in &d.fields {
                    let v = self.field(f, b, depth)?;
                    m.insert(f.name.clone(), v);
                }
                Ok(Value::Object(m))
            }
            DefKind::Message => {
                let mut m = Map::new();
                loop {
                    let id = b.var_u32()?;
                    if id == 0 {
                        return Ok(Value::Object(m));
                    }
                    let f = d
                        .fields
                        .iter()
                        .find(|f| f.value == id)
                        .ok_or_else(|| format!("{} has no field {id}", d.name))?;
                    let v = self.field(f, b, depth)?;
                    m.insert(f.name.clone(), v);
                }
            }
        }
    }
}

// ---------------------------------------------------------------- to HTML

type Guid = (u64, u64);

fn guid(v: &Value) -> Option<Guid> {
    Some((v["sessionID"].as_u64()?, v["localID"].as_u64()?))
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

fn css_color(c: &Value, opacity: f64) -> String {
    let ch = |k: &str| (num(&c[k]).clamp(0.0, 1.0) * 255.0).round() as u8;
    let a = c["a"].as_f64().unwrap_or(1.0) * opacity;
    format!(
        "rgba({},{},{},{:.3})",
        ch("r"),
        ch("g"),
        ch("b"),
        a.clamp(0.0, 1.0)
    )
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn hex(bytes: &Value) -> String {
    bytes
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn image_uri(bytes: &[u8]) -> String {
    let kind = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF") {
        "image/gif"
    } else if bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "application/octet-stream"
    };
    format!("data:{kind};base64,{}", crate::attach::base64(bytes))
}

struct Tree<'a> {
    nodes: HashMap<Guid, &'a Value>,
    kids: HashMap<Guid, Vec<Guid>>,
    images: &'a HashMap<String, Vec<u8>>,
}

impl Tree<'_> {
    /// The CSS for a node's fills, strokes, corners, effects.
    fn look(&self, n: &Value) -> String {
        let mut css = String::new();
        let visible = |p: &&Value| p["visible"].as_bool() != Some(false);
        let fills: Vec<&Value> = n["fillPaints"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(visible)
            .collect();
        let text = n["type"] == "TEXT";
        let mut layers = vec![];
        // Set after the background, which would reset them.
        let mut sizing = String::new();
        for p in fills.iter().rev() {
            let op = p["opacity"].as_f64().unwrap_or(1.0);
            match p["type"].as_str() {
                Some("SOLID") if text => {
                    css.push_str(&format!("color:{};", css_color(&p["color"], op)))
                }
                Some("SOLID") => {
                    let c = css_color(&p["color"], op);
                    layers.push(format!("linear-gradient({c},{c})"));
                }
                Some("IMAGE") => {
                    let key = hex(&p["image"]["hash"]);
                    if let Some(b) = self.images.get(&key) {
                        let fit = match p["imageScaleMode"].as_str() {
                            Some("FIT") => "contain",
                            Some("TILE") => "auto",
                            _ => "cover",
                        };
                        sizing = format!("background-size:{fit};background-position:center;background-repeat:no-repeat;");
                        // Single quotes: this goes in a double-quoted attribute.
                        layers.push(format!("url('{}')", image_uri(b)));
                    }
                }
                Some(t) if t.starts_with("GRADIENT_") => {
                    let stops: Vec<String> = p["stops"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|s| {
                            format!(
                                "{} {:.0}%",
                                css_color(&s["color"], op),
                                num(&s["position"]) * 100.0
                            )
                        })
                        .collect();
                    if stops.len() > 1 {
                        let kind = if t == "GRADIENT_RADIAL" {
                            "radial-gradient("
                        } else {
                            "linear-gradient(180deg,"
                        };
                        layers.push(format!("{kind}{})", stops.join(",")));
                    }
                }
                _ => {}
            }
        }
        if !layers.is_empty() {
            css.push_str(&format!("background:{};", layers.join(",")));
            css.push_str(&sizing);
        }
        let strokes: Vec<&Value> = n["strokePaints"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(visible)
            .collect();
        // A text's stroke outlines its letters, not its box: left out.
        if let Some(s) = strokes.first().filter(|s| s["type"] == "SOLID" && !text) {
            let w = n["strokeWeight"].as_f64().unwrap_or(1.0);
            if w > 0.0 {
                css.push_str(&format!(
                    "box-shadow:inset 0 0 0 {w}px {};",
                    css_color(&s["color"], s["opacity"].as_f64().unwrap_or(1.0))
                ));
            }
        }
        if n["type"] == "ELLIPSE" {
            css.push_str("border-radius:50%;");
        } else {
            let corner = |k: &str| n[k].as_f64();
            match (
                corner("rectangleTopLeftCornerRadius"),
                corner("rectangleTopRightCornerRadius"),
                corner("rectangleBottomRightCornerRadius"),
                corner("rectangleBottomLeftCornerRadius"),
            ) {
                (Some(a), Some(b), Some(c), Some(d)) if !(a == b && b == c && c == d) => {
                    css.push_str(&format!("border-radius:{a}px {b}px {c}px {d}px;"))
                }
                _ => {
                    if let Some(r) = corner("cornerRadius").filter(|r| *r > 0.0) {
                        css.push_str(&format!("border-radius:{r}px;"));
                    }
                }
            }
        }
        let shadows: Vec<String> = n["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["visible"].as_bool() != Some(false))
            .filter_map(|e| {
                let inset = match e["type"].as_str()? {
                    "DROP_SHADOW" => "",
                    "INNER_SHADOW" => "inset ",
                    _ => return None,
                };
                Some(format!(
                    "{inset}{}px {}px {}px {}px {}",
                    num(&e["offset"]["x"]),
                    num(&e["offset"]["y"]),
                    num(&e["radius"]),
                    num(&e["spread"]),
                    css_color(&e["color"], 1.0)
                ))
            })
            .collect();
        if !shadows.is_empty() {
            let had = css.contains("box-shadow:");
            if had {
                css = css.replacen(
                    "box-shadow:",
                    &format!("box-shadow:{},", shadows.join(",")),
                    1,
                );
            } else {
                css.push_str(&format!("box-shadow:{};", shadows.join(",")));
            }
        }
        if n["clipsContent"].as_bool() == Some(true)
            || n["frameMaskDisabled"].as_bool() == Some(false)
        {
            css.push_str("overflow:hidden;");
        }
        if let Some(o) = n["opacity"].as_f64().filter(|o| *o < 1.0) {
            css.push_str(&format!("opacity:{o};"));
        }
        css
    }

    fn text(&self, n: &Value) -> String {
        let mut css = String::new();
        if let Some(f) = n["fontName"]["family"].as_str() {
            css.push_str(&format!(
                "font-family:'{}',system-ui,sans-serif;",
                f.replace('\'', "")
            ));
        }
        let style = n["fontName"]["style"].as_str().unwrap_or("").to_lowercase();
        let weight = [
            ("thin", 100),
            ("extralight", 200),
            ("extra light", 200),
            ("light", 300),
            ("medium", 500),
            ("semibold", 600),
            ("semi bold", 600),
            ("extrabold", 800),
            ("extra bold", 800),
            ("black", 900),
            ("bold", 700),
        ]
        .iter()
        .find(|(w, _)| style.contains(w))
        .map_or(400, |(_, n)| *n);
        css.push_str(&format!("font-weight:{weight};"));
        if style.contains("italic") {
            css.push_str("font-style:italic;");
        }
        if let Some(s) = n["fontSize"].as_f64() {
            css.push_str(&format!("font-size:{s}px;"));
        }
        match n["lineHeight"]["units"].as_str() {
            Some("PIXELS") => css.push_str(&format!(
                "line-height:{}px;",
                num(&n["lineHeight"]["value"])
            )),
            Some("PERCENT") => css.push_str(&format!(
                "line-height:{};",
                num(&n["lineHeight"]["value"]) / 100.0
            )),
            _ => {}
        }
        if let Some(ls) = n["letterSpacing"]["value"].as_f64().filter(|v| *v != 0.0) {
            let unit = if n["letterSpacing"]["units"] == "PERCENT" {
                "em"
            } else {
                "px"
            };
            let v = if unit == "em" { ls / 100.0 } else { ls };
            css.push_str(&format!("letter-spacing:{v}{unit};"));
        }
        let align = match n["textAlignHorizontal"].as_str() {
            Some("CENTER") => "center",
            Some("RIGHT") => "right",
            Some("JUSTIFIED") => "justify",
            _ => "left",
        };
        css.push_str(&format!("text-align:{align};white-space:pre-wrap;"));
        css
    }

    fn node(&self, id: Guid, out: &mut String, depth: usize) {
        let Some(n) = self.nodes.get(&id) else { return };
        if n["visible"].as_bool() == Some(false) || depth > 200 {
            return;
        }
        let t = &n["transform"];
        let (x, y) = (num(&t["m02"]), num(&t["m12"]));
        let (w, h) = (num(&n["size"]["x"]), num(&n["size"]["y"]));
        let (a, b, c, d) = (
            t["m00"].as_f64().unwrap_or(1.0),
            t["m10"].as_f64().unwrap_or(0.0),
            t["m01"].as_f64().unwrap_or(0.0),
            t["m11"].as_f64().unwrap_or(1.0),
        );
        let turn =
            if (a - 1.0).abs() > 1e-4 || b.abs() > 1e-4 || c.abs() > 1e-4 || (d - 1.0).abs() > 1e-4
            {
                format!("transform:matrix({a},{b},{c},{d},0,0);transform-origin:0 0;")
            } else {
                String::new()
            };
        let kind = n["type"].as_str().unwrap_or("");
        let name = esc(n["name"].as_str().unwrap_or(kind));
        let mut css =
            format!("position:absolute;left:{x}px;top:{y}px;width:{w}px;height:{h}px;{turn}");
        css.push_str(&self.look(n));
        if kind == "TEXT" {
            css.push_str(&self.text(n));
            let chars = n["textData"]["characters"].as_str().unwrap_or("");
            out.push_str(&format!(
                "<div data-fig=\"{name}\" style=\"{css}\">{}</div>",
                esc(chars)
            ));
            return;
        }
        out.push_str(&format!("<div data-fig=\"{name}\" style=\"{css}\">"));
        for k in self.kids.get(&id).into_iter().flatten() {
            self.node(*k, out, depth + 1);
        }
        out.push_str("</div>");
    }
}

/// The document as one HTML page: each Figma page a section, its
/// top-level frames laid out as in the file.
pub fn to_html(
    doc: &Value,
    images: &HashMap<String, Vec<u8>>,
    title: &str,
) -> Result<String, String> {
    let changes = doc["nodeChanges"]
        .as_array()
        .ok_or("the file has no nodes")?;
    let mut nodes = HashMap::new();
    let mut kids: HashMap<Guid, Vec<(String, Guid)>> = HashMap::new();
    let mut root = None;
    for n in changes {
        let Some(id) = guid(&n["guid"]) else { continue };
        if n["type"] == "DOCUMENT" {
            root = Some(id);
        }
        if let Some(parent) = guid(&n["parentIndex"]["guid"]) {
            let at = n["parentIndex"]["position"]
                .as_str()
                .unwrap_or("")
                .to_string();
            kids.entry(parent).or_default().push((at, id));
        }
        nodes.insert(id, n);
    }
    let kids: HashMap<Guid, Vec<Guid>> = kids
        .into_iter()
        .map(|(p, mut v)| {
            v.sort();
            (p, v.into_iter().map(|(_, g)| g).collect())
        })
        .collect();
    let tree = Tree {
        nodes,
        kids,
        images,
    };
    let root = root.ok_or("the file has no document")?;
    let mut body = String::new();
    for page in tree.kids.get(&root).into_iter().flatten() {
        let Some(p) = tree.nodes.get(page) else {
            continue;
        };
        if p["type"] != "CANVAS" || p["visible"].as_bool() == Some(false) {
            continue;
        }
        let top: Vec<Guid> = tree.kids.get(page).cloned().unwrap_or_default();
        if top.is_empty() {
            continue;
        }
        // The page's extent, so its frames sit from the top left.
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for k in &top {
            let n = tree.nodes[k];
            let (x, y) = (num(&n["transform"]["m02"]), num(&n["transform"]["m12"]));
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + num(&n["size"]["x"]));
            y1 = y1.max(y + num(&n["size"]["y"]));
        }
        let bg = if p["backgroundColor"].is_object() {
            css_color(&p["backgroundColor"], 1.0)
        } else {
            "#f5f5f5".into()
        };
        body.push_str(&format!(
            "<section class=\"fig-page\" style=\"background:{bg}\"><h2>{}</h2><div class=\"fig-canvas\" style=\"width:{}px;height:{}px\"><div style=\"position:absolute;left:{}px;top:{}px\">",
            esc(p["name"].as_str().unwrap_or("Page")),
            (x1 - x0).max(1.0),
            (y1 - y0).max(1.0),
            -x0,
            -y0
        ));
        for k in &top {
            tree.node(*k, &mut body, 0);
        }
        body.push_str("</div></div></section>");
    }
    if body.is_empty() {
        return Err("the file's pages are empty".into());
    }
    Ok(format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title>\n<style>\nbody{{margin:0;font-family:system-ui,sans-serif}}\n.fig-page{{padding:32px;overflow:auto}}\n.fig-page h2{{font:600 13px system-ui,sans-serif;color:#666;margin:0 0 16px}}\n.fig-canvas{{position:relative}}\n.fig-canvas div{{box-sizing:border-box}}\n</style></head>\n<body>\n<!-- Imported from Figma by Forge: each page, its frames placed as in the file. -->\n{body}\n</body></html>\n",
        esc(title)
    ))
}

/// Import the Figma file at `path` as an HTML page saved at `to`: how many
/// pages it had.
pub fn import(path: &Path, to: &Path) -> Result<usize, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Can't read {}: {e}", path.display()))?;
    let fig = read(&bytes)?;
    let schema = schema(&fig.schema)?;
    let doc = schema.decode("Message", &fig.data)?;
    let title = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Figma".into());
    let html = to_html(&doc, &fig.images, &title)?;
    let pages = html.matches("class=\"fig-page\"").count();
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(to, html).map_err(|e| e.to_string())?;
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A Kiwi writer, for making `.fig` files to read back.
    #[derive(Default)]
    struct W(Vec<u8>);

    impl W {
        fn u(&mut self, mut v: u64) -> &mut Self {
            loop {
                let b = (v & 127) as u8;
                v >>= 7;
                if v == 0 {
                    self.0.push(b);
                    return self;
                }
                self.0.push(b | 128);
            }
        }
        fn i(&mut self, v: i32) -> &mut Self {
            self.u(((v << 1) ^ (v >> 31)) as u32 as u64)
        }
        fn s(&mut self, s: &str) -> &mut Self {
            self.0.extend_from_slice(s.as_bytes());
            self.0.push(0);
            self
        }
        fn f(&mut self, v: f32) -> &mut Self {
            if v == 0.0 {
                self.0.push(0);
                return self;
            }
            let bits = v.to_bits().rotate_right(23);
            self.0.extend_from_slice(&bits.to_le_bytes());
            self
        }
        fn b(&mut self, v: u8) -> &mut Self {
            self.0.push(v);
            self
        }
    }

    // The schema's definitions, by index.
    const GUID: i32 = 0;
    const COLOR: i32 = 1;
    const PAINT: i32 = 2;
    const NODE: i32 = 3;
    const VEC: i32 = 5;
    const MATRIX: i32 = 6;
    const PARENT: i32 = 7;
    const NODE_TYPE: i32 = 8;
    const PAINT_TYPE: i32 = 9;
    const FONT: i32 = 10;
    const TEXT_DATA: i32 = 11;
    const IMAGE: i32 = 12;

    fn write_schema() -> Vec<u8> {
        let mut w = W::default();
        let defs: Vec<(&str, u8, Vec<(&str, i32, bool, u64)>)> = vec![
            (
                "GUID",
                1,
                vec![("sessionID", -4, false, 1), ("localID", -4, false, 2)],
            ),
            (
                "Color",
                1,
                vec![
                    ("r", -5, false, 1),
                    ("g", -5, false, 2),
                    ("b", -5, false, 3),
                    ("a", -5, false, 4),
                ],
            ),
            (
                "Paint",
                2,
                vec![
                    ("type", PAINT_TYPE, false, 1),
                    ("color", COLOR, false, 2),
                    ("opacity", -5, false, 3),
                    ("visible", -1, false, 4),
                    ("image", IMAGE, false, 5),
                ],
            ),
            (
                "NodeChange",
                2,
                vec![
                    ("guid", GUID, false, 1),
                    ("parentIndex", PARENT, false, 2),
                    ("type", NODE_TYPE, false, 3),
                    ("name", -6, false, 4),
                    ("size", VEC, false, 5),
                    ("transform", MATRIX, false, 6),
                    ("fillPaints", PAINT, true, 7),
                    ("cornerRadius", -5, false, 8),
                    ("fontSize", -5, false, 9),
                    ("fontName", FONT, false, 10),
                    ("textData", TEXT_DATA, false, 11),
                    ("visible", -1, false, 12),
                    ("strokeWeight", -5, false, 13),
                    ("strokePaints", PAINT, true, 14),
                ],
            ),
            (
                "Message",
                2,
                vec![
                    ("type", -4, false, 1),
                    ("nodeChanges", NODE, true, 2),
                    ("blobs", -2, true, 3),
                ],
            ),
            ("Vector", 1, vec![("x", -5, false, 1), ("y", -5, false, 2)]),
            (
                "Matrix",
                1,
                vec![
                    ("m00", -5, false, 1),
                    ("m01", -5, false, 2),
                    ("m02", -5, false, 3),
                    ("m10", -5, false, 4),
                    ("m11", -5, false, 5),
                    ("m12", -5, false, 6),
                ],
            ),
            (
                "ParentIndex",
                1,
                vec![("guid", GUID, false, 1), ("position", -6, false, 2)],
            ),
            (
                "NodeType",
                0,
                vec![
                    ("DOCUMENT", 0, false, 1),
                    ("CANVAS", 0, false, 2),
                    ("FRAME", 0, false, 3),
                    ("TEXT", 0, false, 4),
                    ("RECTANGLE", 0, false, 5),
                    ("ELLIPSE", 0, false, 6),
                ],
            ),
            (
                "PaintType",
                0,
                vec![("SOLID", 0, false, 0), ("IMAGE", 0, false, 1)],
            ),
            (
                "FontName",
                1,
                vec![("family", -6, false, 1), ("style", -6, false, 2)],
            ),
            ("TextData", 2, vec![("characters", -6, false, 1)]),
            (
                "Image",
                2,
                vec![("hash", -2, true, 1), ("name", -6, false, 2)],
            ),
        ];
        w.u(defs.len() as u64);
        for (name, kind, fields) in defs {
            w.s(name).b(kind).u(fields.len() as u64);
            for (f, ty, arr, v) in fields {
                w.s(f).i(ty).b(u8::from(arr)).u(v);
            }
        }
        w.0
    }

    struct N<'a> {
        id: u64,
        parent: Option<(u64, &'a str)>,
        kind: u64,
        name: &'a str,
        at: (f32, f32, f32, f32),
        fill: Option<(f32, f32, f32)>,
        image: Option<[u8; 3]>,
        text: Option<&'a str>,
        hidden: bool,
    }

    fn node(w: &mut W, n: &N) {
        let g = |w: &mut W, id: u64| {
            w.u(0).u(id);
        };
        w.u(1);
        g(w, n.id);
        if let Some((p, pos)) = n.parent {
            w.u(2);
            g(w, p);
            w.s(pos);
        }
        w.u(3).u(n.kind);
        w.u(4).s(n.name);
        let (x, y, wd, h) = n.at;
        w.u(5).f(wd).f(h);
        w.u(6).f(1.0).f(0.0).f(x).f(0.0).f(1.0).f(y);
        if let Some((r, gg, b)) = n.fill {
            w.u(7).u(1);
            w.u(1)
                .u(0)
                .u(2)
                .f(r)
                .f(gg)
                .f(b)
                .f(1.0)
                .u(3)
                .f(1.0)
                .u(4)
                .b(1)
                .u(0);
        }
        if let Some(hash) = n.image {
            w.u(7).u(1);
            w.u(1)
                .u(1)
                .u(5)
                .u(1)
                .u(3)
                .b(hash[0])
                .b(hash[1])
                .b(hash[2])
                .u(0)
                .u(0);
        }
        if let Some(t) = n.text {
            w.u(9).f(24.0);
            w.u(10).s("Inter").s("Bold Italic");
            w.u(11).u(1).s(t).u(0);
        }
        if n.hidden {
            w.u(12).b(0);
        }
        w.u(13).f(2.0);
        w.u(14).u(1).u(1).u(0).u(2).f(0.0).f(0.0).f(0.0).f(1.0).u(0);
        w.u(0);
    }

    fn doc(image: bool) -> Vec<u8> {
        let mut w = W::default();
        let n = |id, parent, kind, name, at| N {
            id,
            parent,
            kind,
            name,
            at,
            fill: None,
            image: None,
            text: None,
            hidden: false,
        };
        let nodes = vec![
            n(0, None, 1, "Document", (0.0, 0.0, 0.0, 0.0)),
            n(1, Some((0, "a")), 2, "Page 1", (0.0, 0.0, 0.0, 0.0)),
            N {
                fill: Some((1.0, 1.0, 1.0)),
                ..n(2, Some((1, "a")), 3, "Hero", (100.0, 200.0, 1440.0, 900.0))
            },
            N {
                text: Some("Ship <faster>"),
                fill: Some((0.1, 0.2, 0.3)),
                ..n(3, Some((2, "b")), 4, "Title", (64.0, 80.0, 600.0, 40.0))
            },
            N {
                fill: Some((1.0, 0.0, 0.0)),
                ..n(4, Some((2, "a")), 6, "Dot", (10.0, 10.0, 20.0, 20.0))
            },
            N {
                hidden: true,
                ..n(5, Some((2, "c")), 5, "Hidden", (0.0, 0.0, 5.0, 5.0))
            },
            N {
                image: image.then_some([0xab, 0xcd, 0xef]),
                ..n(6, Some((2, "d")), 5, "Photo", (700.0, 80.0, 300.0, 200.0))
            },
        ];
        w.u(1).u(1);
        w.u(2).u(nodes.len() as u64);
        for nd in &nodes {
            node(&mut w, nd);
        }
        w.u(0);
        w.0
    }

    fn fig(zstd: bool, image: bool) -> Vec<u8> {
        let pack = |b: Vec<u8>, zstd: bool| -> Vec<u8> {
            if zstd {
                ruzstd::encoding::compress_to_vec(
                    &b[..],
                    ruzstd::encoding::CompressionLevel::Fastest,
                )
            } else {
                let mut e =
                    flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
                e.write_all(&b).unwrap();
                e.finish().unwrap()
            }
        };
        let mut out = b"fig-kiwi".to_vec();
        out.extend_from_slice(&48u32.to_le_bytes());
        for chunk in [pack(write_schema(), false), pack(doc(image), zstd)] {
            out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
            out.extend_from_slice(&chunk);
        }
        out
    }

    #[test]
    fn kiwi_floats_and_ints_read_back() {
        let mut w = W::default();
        w.f(1.5).f(-0.25).f(0.0).i(-7).i(300).u(1 << 40);
        let mut b = Buf { b: &w.0, at: 0 };
        assert_eq!(b.var_f32().unwrap(), 1.5);
        assert_eq!(b.var_f32().unwrap(), -0.25);
        assert_eq!(b.var_f32().unwrap(), 0.0);
        assert_eq!(b.var_i32().unwrap(), -7);
        assert_eq!(b.var_i32().unwrap(), 300);
        assert_eq!(b.var_u64().unwrap(), 1 << 40);
    }

    #[test]
    fn a_fig_file_becomes_a_page() {
        for zstd in [true, false] {
            let f = read(&fig(zstd, false)).unwrap();
            let s = schema(&f.schema).unwrap();
            let doc = s.decode("Message", &f.data).unwrap();
            assert_eq!(doc["nodeChanges"].as_array().unwrap().len(), 7);
            assert_eq!(doc["nodeChanges"][2]["type"], "FRAME");
            let html = to_html(&doc, &HashMap::new(), "Launch").unwrap();
            assert!(html.contains("<h2>Page 1</h2>"));
            assert!(html.contains("data-fig=\"Hero\""), "{html}");
            assert!(html.contains("width:1440px;height:900px"));
            // The page's frames start at its top left.
            assert!(html.contains("left:-100px;top:-200px"));
            assert!(html.contains("Ship &lt;faster&gt;"));
            assert!(
                html.contains("font-family:'Inter'")
                    && html.contains("font-weight:700")
                    && html.contains("font-style:italic")
            );
            assert!(html.contains("color:rgba(26,51,77,1.000)"), "{html}");
            assert!(html.contains("border-radius:50%"));
            assert!(html.contains("box-shadow:inset 0 0 0 2px rgba(0,0,0,1.000)"));
            assert!(!html.contains("Hidden"));
            // Children in their order: the dot (a) before the title (b).
            assert!(html.find("\"Dot\"").unwrap() < html.find("\"Title\"").unwrap());
        }
    }

    #[test]
    fn a_zipped_fig_brings_its_pictures() {
        let png = {
            let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]));
            let mut out = std::io::Cursor::new(vec![]);
            img.write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("canvas.fig", o).unwrap();
        z.write_all(&fig(true, true)).unwrap();
        z.start_file("images/abcdef", o).unwrap();
        z.write_all(&png).unwrap();
        z.start_file("meta.json", o).unwrap();
        z.write_all(b"{}").unwrap();
        let bytes = z.finish().unwrap().into_inner();
        let dir = std::env::temp_dir().join(format!("forge-fig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Launch.fig");
        std::fs::write(&path, bytes).unwrap();
        // A copy to try by hand: FORGE_FIG_SAMPLE=<path> cargo test design::fig
        if let Some(keep) = std::env::var_os("FORGE_FIG_SAMPLE") {
            std::fs::copy(&path, keep).unwrap();
        }
        let to = dir.join("launch.html");
        assert_eq!(import(&path, &to).unwrap(), 1);
        let html = std::fs::read_to_string(&to).unwrap();
        assert!(html.contains("url('data:image/png;base64,"), "{html}");
        // Nothing breaks out of its attribute.
        assert!(!html.contains("url(\""));
        assert!(html.contains("<title>Launch</title>"));
        assert!(read(b"nope").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
