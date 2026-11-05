//! Diagrams KodAct draws itself. The bundled Diagram Design skill
//! (`assets/kodact-plugin/skills/diagram-design`) writes each diagram as a
//! self-contained HTML page around one inline `<svg>`; KodAct takes that SVG
//! out and renders it natively (resvg, through GPUI's image pipeline), sized
//! to the exact device pixels it is shown at, so nothing is scaled twice.

/// Whether a file is a diagram KodAct can draw: an `.svg`, or an `.html`
/// page that holds one.
pub fn is_diagram(name: &str, content: &str) -> bool {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "svg" => content.contains("<svg"),
        "html" | "htm" => extract_svg(content).is_some(),
        _ => false,
    }
}

/// `svg` with every font it names set in KodAct Sans: each
/// `font-family` attribute and CSS declaration, whatever face the drawing
/// asked for. KodAct draws in no other face.
pub fn brand_fonts(svg: &str) -> String {
    const FACE: &str = "'KodAct Sans'";
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(at) = rest.find("font-family") {
        let after = &rest[at + "font-family".len()..];
        let lead = after.len() - after.trim_start().len();
        let sep = after[lead..].chars().next();
        out.push_str(&rest[..at + "font-family".len() + lead]);
        let value = &after[lead..];
        match sep {
            // An attribute: font-family="…" (or '…').
            Some('=') => {
                let v = value[1..].trim_start();
                let quote = v.chars().next().filter(|q| *q == '"' || *q == '\'');
                match quote.and_then(|q| v[1..].find(q).map(|end| (q, end))) {
                    Some((q, end)) => {
                        let face = if q == '"' {
                            FACE.to_string()
                        } else {
                            FACE.replace('\'', "\"")
                        };
                        out.push_str(&format!("={q}{face}{q}"));
                        rest = &v[1 + end + 1..];
                    }
                    None => {
                        out.push('=');
                        rest = &value[1..];
                    }
                }
            }
            // A declaration: font-family: …; up to `;`, `}` or the attribute's quote.
            Some(':') => {
                let body = &value[1..];
                let end = body.find([';', '}', '"', '<']).unwrap_or(body.len());
                out.push_str(": ");
                out.push_str(FACE);
                rest = &body[end..];
            }
            _ => rest = value,
        }
    }
    out.push_str(rest);
    out
}

/// The first top-level `<svg>…</svg>` in `content`, nested ones included.
pub fn extract_svg(content: &str) -> Option<&str> {
    let start = content.find("<svg")?;
    let mut depth = 0usize;
    let mut i = start;
    while i < content.len() {
        let rest = &content[i..];
        if rest.starts_with("<svg")
            && rest[4..].starts_with(|c: char| c.is_whitespace() || c == '>')
        {
            depth += 1;
            i += 4;
        } else if rest.starts_with("</svg>") {
            depth -= 1;
            i += 6;
            if depth == 0 {
                return Some(&content[start..i]);
            }
        } else {
            i += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    None
}

/// The root tag of an SVG: `<svg …>`.
fn root_tag(svg: &str) -> Option<&str> {
    svg.find('>').map(|end| &svg[..=end])
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    tag[at..].find('"').map(|end| &tag[at..at + end])
}

/// The drawing's own size, from its viewBox (else its width and height).
pub fn view_size(svg: &str) -> Option<(f32, f32)> {
    let tag = root_tag(svg)?;
    if let Some(vb) = attr(tag, "viewBox") {
        let n: Vec<f32> = vb
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        if n.len() == 4 && n[2] > 0. && n[3] > 0. {
            return Some((n[2], n[3]));
        }
    }
    let num = |s: &str| {
        s.trim_end_matches("px")
            .parse::<f32>()
            .ok()
            .filter(|v| *v > 0.)
    };
    Some((num(attr(tag, "width")?)?, num(attr(tag, "height")?)?))
}

/// The SVG with its root sized to `width` pixels (height from its aspect).
pub fn sized(svg: &str, width: u32) -> Option<String> {
    let (w, h) = view_size(svg)?;
    let tag = root_tag(svg)?;
    let mut new_tag = tag.to_string();
    for name in ["width", "height"] {
        if let Some(v) = attr(&new_tag, name) {
            let old = format!(" {name}=\"{v}\"");
            new_tag = new_tag.replacen(&old, "", 1);
        }
    }
    let height = (width as f32 * h / w).round().max(1.) as u32;
    new_tag = new_tag.replacen(
        "<svg",
        &format!("<svg width=\"{width}\" height=\"{height}\""),
        1,
    );
    Some(format!("{new_tag}{}", &svg[tag.len()..]))
}

/// The KodAct skin's light colours and their dark counterparts (the bundled
/// skill's `references/style-guide.md`), for drawing a light diagram in the
/// dark theme.
const TO_DARK: &[(&str, &str)] = &[
    ("#f9f7f5", "#1c1b19"),                  // paper
    ("#f1efeb", "#22211f"),                  // paper-2
    ("#1c1b19", "#f6f5f4"),                  // ink
    ("#615d59", "#b5b0aa"),                  // muted
    ("#6f6a64", "#9d978f"),                  // soft
    ("#c9c4bd", "#4a4744"),                  // rule-solid
    ("#005fb3", "#4ba1f3"),                  // accent
    ("#6a4fb4", "#ac93e6"),                  // link
    ("#ffffff", "#111111"),                  // ink-strong, on accent fills
    ("rgba(0,95,179,", "rgba(75,161,243,"),  // accent-tint
    ("rgba(28,27,25,", "rgba(246,245,244,"), // rule
];

/// A light, KodAct-skinned drawing in the skin's dark colours; `None` when
/// the drawing is not in the KodAct skin (it is then shown as it is).
pub fn for_dark(svg: &str) -> Option<String> {
    if !svg.to_ascii_lowercase().contains("fill=\"#f9f7f5\"") {
        return None;
    }
    // Every swap at once, so a colour swapped in is never swapped again
    // (ink and dark paper are the same value).
    let lower = svg.to_ascii_lowercase();
    let mut out = String::with_capacity(svg.len());
    let mut i = 0;
    'scan: while i < svg.len() {
        for (from, to) in TO_DARK {
            if lower[i..].starts_with(from) {
                out.push_str(to);
                i += from.len();
                continue 'scan;
            }
        }
        let ch = svg[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    Some(out)
}

fn inner_text<'a>(content: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let at = content.find(open)?;
    let body = &content[at..];
    let start = body.find('>')? + 1;
    let end = body.find(close)?;
    (start <= end)
        .then(|| body[start..end].trim())
        .filter(|s| !s.is_empty())
}

/// What the diagram is called: the page's `<h1>`, else the SVG's `<title>`.
pub fn title(content: &str) -> Option<String> {
    inner_text(content, "<h1", "</h1>")
        .or_else(|| inner_text(extract_svg(content)?, "<title", "</title>"))
        .map(unescape)
}

/// Its one-sentence description, from the SVG's `<desc>`.
pub fn description(content: &str) -> Option<String> {
    inner_text(extract_svg(content)?, "<desc", "</desc>").map(unescape)
}

fn unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_font_a_drawing_names_becomes_anthropic_sans() {
        let svg = r#"<svg><style>text { font-family: 'Geist', sans-serif; fill: red } .m{font-family:JetBrains Mono}</style><text font-family="Hanken Grotesk, Arial" x="1">a</text><text style="font-family: Inter; font-size: 9px">b</text><text font-family='Lilex'>c</text></svg>"#;
        let out = super::brand_fonts(svg);
        for gone in [
            "Geist",
            "sans-serif",
            "JetBrains",
            "Hanken",
            "Arial",
            "Inter",
            "Lilex",
        ] {
            assert!(!out.contains(gone), "{gone} in {out}");
        }
        assert_eq!(out.matches("KodAct Sans").count(), 5, "{out}");
        assert!(
            out.contains("fill: red") && out.contains("font-size: 9px") && out.contains(r#"x="1""#)
        );
        assert_eq!(super::brand_fonts("<svg/>"), "<svg/>");
    }

    use super::*;

    const PAGE: &str = r##"<!DOCTYPE html><html><body><p class="eyebrow">Architecture · Diagram Design</p><h1>How a chat reaches a model</h1>
<div class="diagram-container"><svg viewBox="0 0 960 600" xmlns="http://www.w3.org/2000/svg" role="img"><title id="t">Chat path</title><desc id="d">From KodAct to an endpoint &amp; back.</desc>
<g><svg x="4" y="4" width="16" height="16" viewBox="0 0 24 24"><path d="M0 0"/></svg></g><rect width="100%" height="100%" fill="#f9f7f5"/></svg></div></body></html>"##;

    #[test]
    fn finds_the_whole_drawing_with_nested_icons() {
        let svg = extract_svg(PAGE).unwrap();
        assert!(svg.starts_with("<svg viewBox=\"0 0 960 600\""));
        assert!(svg.ends_with(r##"fill="#f9f7f5"/></svg>"##), "{svg}");
        assert!(is_diagram("arch.html", PAGE));
        assert!(!is_diagram("notes.md", PAGE));
        assert!(!is_diagram(
            "page.html",
            "<html><body>no drawing</body></html>"
        ));
        assert!(extract_svg("<svg viewBox='0 0 1 1'>unclosed").is_none());
    }

    #[test]
    fn sizes_to_exact_pixels_keeping_the_aspect() {
        let svg = extract_svg(PAGE).unwrap();
        assert_eq!(view_size(svg), Some((960., 600.)));
        let out = sized(svg, 1360).unwrap();
        assert!(
            out.starts_with("<svg width=\"1360\" height=\"850\" viewBox=\"0 0 960 600\""),
            "{}",
            &out[..80]
        );
        // Only the root is resized; the nested icon keeps its 16px.
        assert!(out.contains(r#"<svg x="4" y="4" width="16" height="16""#));
        let sized_twice = sized(&out, 480).unwrap();
        assert!(
            sized_twice.starts_with("<svg width=\"480\" height=\"300\" viewBox"),
            "{}",
            &sized_twice[..60]
        );
        assert_eq!(
            view_size(r#"<svg width="200px" height="100px">"#),
            Some((200., 100.))
        );
    }

    #[test]
    fn a_forge_drawing_turns_dark_in_one_pass() {
        let light = r##"<svg viewBox="0 0 10 10"><rect fill="#F9F7F5"/><text fill="#1c1b19">x</text><rect fill="rgba(0,95,179,0.08)" stroke="#005fb3"/></svg>"##;
        let dark = for_dark(light).unwrap();
        // Paper became dark ink-coloured paper, ink became light: not swapped twice.
        assert!(dark.contains(r##"<rect fill="#1c1b19"/>"##), "{dark}");
        assert!(dark.contains(r##"<text fill="#f6f5f4">"##), "{dark}");
        assert!(dark.contains("rgba(75,161,243,0.08)") && dark.contains("#4ba1f3"));
        assert!(
            for_dark(r##"<svg><rect fill="#f5f5f5"/></svg>"##).is_none(),
            "another skin is left alone"
        );
    }

    #[test]
    fn the_bundled_skill_wears_the_forge_skin() {
        let root = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/kodact-plugin/skills/diagram-design"
        );
        let guide = std::fs::read_to_string(format!("{root}/references/style-guide.md")).unwrap();
        assert!(guide.starts_with("<!-- diagram-design-profile\nname: KodAct\nslug: forge"));
        for template in [
            "template.html",
            "template-full.html",
            "template-motion.html",
            "template-dark.html",
        ] {
            let t = std::fs::read_to_string(format!("{root}/assets/{template}")).unwrap();
            assert!(t.contains("'KodAct Sans'"), "{template}");
            for old in [
                "Geist",
                "Instrument",
                "Hanken",
                "JetBrains",
                "googleapis",
                "#bf4520",
                "#2d3142",
                "#f5f5f5",
            ] {
                assert!(!t.contains(old), "{template} still has {old}");
            }
        }
        // The skin's colours meet the skill's own contrast contract, in both themes.
        use crate::palette_check::contrast;
        let c = |x: u32| -> gpui::Hsla { gpui::rgb(x).into() };
        for (fg, bg, floor) in [
            (0x1c1b19, 0xf9f7f5, 4.5),
            (0x615d59, 0xf9f7f5, 4.5),
            (0x6f6a64, 0xf9f7f5, 4.5),
            (0x005fb3, 0xf9f7f5, 4.5),
            (0x6a4fb4, 0xf9f7f5, 4.5),
            (0xf6f5f4, 0x1c1b19, 4.5),
            (0xb5b0aa, 0x1c1b19, 4.5),
            (0x9d978f, 0x1c1b19, 4.5),
            (0x4ba1f3, 0x1c1b19, 4.5),
            (0xac93e6, 0x1c1b19, 4.5),
        ] {
            let r = contrast(c(fg), c(bg));
            assert!(r >= floor, "#{fg:06x} on #{bg:06x} is {r:.2}:1");
        }
        let page = include_str!("../assets/docs/forge-chat-path.html");
        assert!(
            is_diagram("chat-path.html", page) && for_dark(extract_svg(page).unwrap()).is_some()
        );
    }

    #[test]
    fn names_and_describes_it() {
        assert_eq!(title(PAGE).as_deref(), Some("How a chat reaches a model"));
        assert_eq!(
            description(PAGE).as_deref(),
            Some("From KodAct to an endpoint & back.")
        );
        let bare = r#"<svg viewBox="0 0 10 10"><title>Only a title</title></svg>"#;
        assert_eq!(title(bare).as_deref(), Some("Only a title"));
    }
}
