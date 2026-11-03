//! A brand from a website (Open Design's "brand from URL"): load the site
//! in the export browser, read what it actually draws (its colours by role
//! and by how much of the screen they cover, its type, sizes, corners,
//! buttons and logo) and write it down as a design system, a `DESIGN.md`
//! and `tokens.css`, which a design chat then designs with.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// What the page script reads. Areas are in square CSS pixels; text in
/// characters.
pub const READ_JS: &str = r#"(() => {
  const hex = c => {
    const m = (c || '').match(/rgba?\(([^)]+)\)/);
    if (!m) return null;
    const p = m[1].split(/[\s,\/]+/).filter(Boolean).map(Number);
    if (p.length > 3 && p[3] < 0.5) return null;
    return '#' + p.slice(0, 3).map(v => Math.round(v).toString(16).padStart(2, '0')).join('');
  };
  const bg = {}, ink = {}, border = {}, fonts = {}, sizes = {}, radii = {}, weights = {};
  const add = (o, k, n) => { if (k) o[k] = (o[k] || 0) + n; };
  const W = innerWidth, H = Math.max(innerHeight, 1);
  const els = [...document.querySelectorAll('body, body *')].slice(0, 6000);
  for (const e of els) {
    const r = e.getBoundingClientRect();
    if (!r.width || !r.height || r.bottom < 0 || r.top > H * 3) continue;
    const cs = getComputedStyle(e);
    if (cs.visibility === 'hidden' || cs.display === 'none' || +cs.opacity === 0) continue;
    const area = Math.min(r.width, W) * Math.min(r.height, H * 3);
    add(bg, hex(cs.backgroundColor), area);
    const own = [...e.childNodes].filter(n => n.nodeType === 3).map(n => n.textContent.trim()).join(' ');
    if (own) {
      const n = own.length;
      add(ink, hex(cs.color), n);
      add(fonts, cs.fontFamily.split(',')[0].trim().replace(/^["']|["']$/g, ''), n);
      add(sizes, Math.round(parseFloat(cs.fontSize)), n);
      add(weights, cs.fontWeight, n);
    }
    if (parseFloat(cs.borderTopWidth) > 0 && cs.borderTopStyle !== 'none') add(border, hex(cs.borderTopColor), r.width);
    const rad = parseFloat(cs.borderTopLeftRadius);
    if (rad > 0 && rad < 200) add(radii, Math.round(rad), 1);
  }
  // The buttons: what a call to action looks like.
  const buttons = [...document.querySelectorAll('button, a[class*="btn"], a[class*="button"], [role="button"], input[type="submit"]')]
    .filter(b => { const r = b.getBoundingClientRect(); return r.width > 40 && r.height > 20 && r.top < H * 3; })
    .slice(0, 12).map(b => { const cs = getComputedStyle(b); return {
      text: (b.innerText || b.value || '').trim().slice(0, 40), bg: hex(cs.backgroundColor), color: hex(cs.color),
      radius: Math.round(parseFloat(cs.borderTopLeftRadius) || 0), padding: cs.padding, weight: cs.fontWeight,
      size: Math.round(parseFloat(cs.fontSize)) }; });
  // The logo: a picture in the header that says so, else the icon.
  const logoEl = [...document.querySelectorAll('header img, header svg, nav img, nav svg, [class*="logo" i] img, [class*="logo" i] svg, img[alt*="logo" i], [id*="logo" i] img, [id*="logo" i] svg')]
    .find(e => { const r = e.getBoundingClientRect(); return r.width > 12 && r.height > 12 && r.top < 200; });
  let logo = null;
  if (logoEl) logo = logoEl.tagName.toLowerCase() === 'svg' ? { svg: logoEl.outerHTML.slice(0, 60000) } : { src: logoEl.currentSrc || logoEl.src };
  const icon = document.querySelector('link[rel~="icon"]');
  const meta = n => (document.querySelector(`meta[name="${n}"], meta[property="${n}"]`) || {}).content || '';
  // Custom properties the site names: its own tokens.
  const vars = {};
  for (const sheet of [...document.styleSheets]) {
    let rules; try { rules = sheet.cssRules; } catch (_) { continue; }
    for (const rule of [...rules].slice(0, 4000)) {
      if (rule.selectorText !== ':root' && rule.selectorText !== 'html') continue;
      for (const p of [...rule.style]) if (p.startsWith('--') && Object.keys(vars).length < 120) vars[p] = rule.style.getPropertyValue(p).trim();
    }
  }
  return JSON.stringify({
    title: document.title, description: meta('description') || meta('og:description'),
    bg, ink, border, fonts, sizes, radii, weights, buttons, logo,
    icon: icon ? icon.href : null, vars,
    css_failed: [...document.querySelectorAll('link[rel~="stylesheet"]')].filter(l => !l.sheet).map(l => l.href || l.getAttribute('href') || '(no address)').slice(0, 20),
    body_bg: hex(getComputedStyle(document.body).backgroundColor) || hex(getComputedStyle(document.documentElement).backgroundColor),
  });
})()"#;

#[derive(Debug, Default, Deserialize)]
pub struct Read {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub bg: BTreeMap<String, f64>,
    #[serde(default)]
    pub ink: BTreeMap<String, f64>,
    #[serde(default)]
    pub border: BTreeMap<String, f64>,
    #[serde(default)]
    pub fonts: BTreeMap<String, f64>,
    #[serde(default)]
    pub sizes: BTreeMap<String, f64>,
    #[serde(default)]
    pub radii: BTreeMap<String, f64>,
    #[serde(default)]
    pub buttons: Vec<Button>,
    #[serde(default)]
    pub logo: Option<Logo>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub vars: BTreeMap<String, String>,
    #[serde(default)]
    pub body_bg: Option<String>,
    /// Stylesheets the page links that didn't load.
    #[serde(default)]
    pub css_failed: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Button {
    #[serde(default)]
    pub text: String,
    pub bg: Option<String>,
    pub color: Option<String>,
    #[serde(default)]
    pub radius: u32,
    #[serde(default)]
    pub padding: String,
    #[serde(default)]
    pub size: u32,
}

#[derive(Debug, Deserialize)]
pub struct Logo {
    pub svg: Option<String>,
    pub src: Option<String>,
}

/// Keys by weight, most first.
fn ranked(m: &BTreeMap<String, f64>) -> Vec<(String, f64)> {
    let mut v: Vec<(String, f64)> = m.iter().map(|(k, n)| (k.clone(), *n)).collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

fn rgb(hex: &str) -> Option<(f64, f64, f64)> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let n = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16)
            .ok()
            .map(|v| f64::from(v) / 255.0)
    };
    Some((n(0)?, n(2)?, n(4)?))
}

/// How colourful: 0 for a grey.
fn chroma(hex: &str) -> f64 {
    rgb(hex).map_or(0.0, |(r, g, b)| r.max(g).max(b) - r.min(g).min(b))
}

fn luminance(hex: &str) -> f64 {
    let lin = |c: f64| {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    rgb(hex).map_or(0.0, |(r, g, b)| {
        0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    })
}

fn contrast(a: &str, b: &str) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The brand, read into roles.
#[derive(Debug)]
pub struct Brand {
    pub name: String,
    pub source: String,
    pub description: String,
    pub background: String,
    pub surface: Option<String>,
    pub text: String,
    pub muted: Option<String>,
    pub accent: Option<String>,
    pub accents: Vec<String>,
    pub border: Option<String>,
    pub fonts: Vec<String>,
    pub sizes: Vec<u32>,
    pub radius: Option<u32>,
    pub button: Option<Button>,
    pub vars: BTreeMap<String, String>,
    /// Stylesheets that didn't load: the reading is of a page without them.
    pub css_failed: Vec<String>,
}

/// Read roles out of what the page drew.
pub fn brand(read: Read, url: &str) -> Brand {
    let bgs = ranked(&read.bg);
    let inks = ranked(&read.ink);
    let background = read
        .body_bg
        .clone()
        .or_else(|| bgs.first().map(|b| b.0.clone()))
        .unwrap_or_else(|| "#ffffff".into());
    let surface = bgs
        .iter()
        .map(|b| &b.0)
        .find(|c| **c != background && chroma(c) < 0.08)
        .cloned();
    let text = inks
        .iter()
        .map(|i| &i.0)
        .find(|c| contrast(c, &background) >= 4.5)
        .cloned()
        .unwrap_or_else(|| {
            if luminance(&background) > 0.4 {
                "#111111".into()
            } else {
                "#f5f5f5".into()
            }
        });
    let muted = inks
        .iter()
        .map(|i| &i.0)
        .find(|c| **c != text && chroma(c) < 0.1 && contrast(c, &background) >= 3.0)
        .cloned();
    // Long grey copy can outweigh the main ink; muted is the quieter of
    // the two, whichever covers more.
    let (text, muted) = match muted {
        Some(m) if contrast(&m, &background) > contrast(&text, &background) => (m, Some(text)),
        m => (text, m),
    };
    // The accent: the colourful one that buttons use, else the most used
    // colourful one anywhere.
    let button = read
        .buttons
        .into_iter()
        .filter(|b| b.bg.as_deref().is_some_and(|c| chroma(c) > 0.15))
        .max_by_key(|b| b.size);
    let mut colourful: Vec<(String, f64)> = bgs
        .iter()
        .chain(inks.iter())
        .filter(|(c, _)| chroma(c) > 0.2)
        .cloned()
        .collect();
    colourful.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut accents: Vec<String> = vec![];
    if let Some(c) = button.as_ref().and_then(|b| b.bg.clone()) {
        accents.push(c);
    }
    for (c, _) in colourful {
        if !accents.contains(&c) && accents.len() < 5 {
            accents.push(c);
        }
    }
    let border = ranked(&read.border).first().map(|b| b.0.clone());
    let fonts: Vec<String> = ranked(&read.fonts)
        .into_iter()
        .map(|f| f.0)
        .filter(|f| !f.is_empty())
        .take(3)
        .collect();
    let mut sizes: Vec<u32> = ranked(&read.sizes)
        .into_iter()
        .take(8)
        .filter_map(|s| s.0.parse().ok())
        .collect();
    sizes.sort_unstable();
    sizes.dedup();
    let radius = ranked(&read.radii).first().and_then(|r| r.0.parse().ok());
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .trim_start_matches("www.")
        .to_string();
    let name = brand_name(&read.title, &host);
    Brand {
        name,
        source: url.to_string(),
        description: read.description,
        background,
        surface,
        text,
        muted,
        accent: accents.first().cloned(),
        accents,
        border,
        fonts,
        sizes,
        radius,
        button,
        vars: read.vars,
        css_failed: read.css_failed,
    }
}

/// The brand's name from the page title: the part of it that names the
/// site (`Pricing | Acme` → `Acme`), else its first part (`Kiln Studio:
/// pots` → `Kiln Studio`), else the host.
fn brand_name(title: &str, host: &str) -> String {
    let squash = |s: &str| -> String {
        s.chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect()
    };
    let label = squash(host.split('.').next().unwrap_or(host));
    let mut cut = title.to_string();
    for sep in [" |", "| ", " - ", " – ", " — ", ": ", " · "] {
        cut = cut.replace(sep, "\u{1}");
    }
    let parts: Vec<&str> = cut
        .split('\u{1}')
        .map(|p| p.trim_matches(|c: char| c.is_whitespace() || "|-–—:·".contains(c)))
        .filter(|p| !p.is_empty())
        .collect();
    let fits = |p: &&&str| p.chars().count() <= 32;
    parts
        .iter()
        .filter(fits)
        .find(|p| !label.is_empty() && squash(p) == label)
        .or_else(|| parts.first().filter(fits))
        .map(|p| p.to_string())
        .unwrap_or_else(|| host.to_string())
}

/// The generic family to fall back to after `font`.
fn generic(font: &str) -> &'static str {
    let f = font.to_ascii_lowercase();
    if ["mono", "code", "courier", "consolas", "menlo"]
        .iter()
        .any(|k| f.contains(k))
    {
        "ui-monospace, monospace"
    } else if f.contains("sans") {
        "system-ui, sans-serif"
    } else if [
        "serif",
        "georgia",
        "times",
        "garamond",
        "playfair",
        "merriweather",
        "lora",
        "baskerville",
        "caslon",
        "didot",
        "bodoni",
        "tiempos",
        "freight",
        "canela",
        "charter",
        "cambria",
    ]
    .iter()
    .any(|k| f.contains(k))
    {
        "Georgia, serif"
    } else {
        "system-ui, sans-serif"
    }
}

/// The brand as a `DESIGN.md`, in the library's sections.
pub fn design_md(b: &Brand) -> String {
    let mut out = format!(
        "# Design System Captured from {}\n\n> Source: {}\n> {}\n\n",
        b.name,
        b.source,
        if b.description.is_empty() {
            "Read from the site as it draws: colours by how much of the screen they cover, type by how much text uses it."
        } else {
            b.description.as_str()
        }
    );
    if !b.css_failed.is_empty() {
        out.push_str(&format!(
            "> **Warning:** {} of the site's stylesheets didn't load{}, so this was read from the page without them. Capture it again where they load before designing with it.\n\n",
            b.css_failed.len(),
            crate::design::capture::named(&b.css_failed)
        ));
    }
    out.push_str("## 1. Visual Theme & Atmosphere\n\n");
    let dark = luminance(&b.background) < 0.2;
    out.push_str(&format!(
        "A {} canvas (`{}`) with `{}` text{}. {}\n\n",
        if dark { "dark" } else { "light" },
        b.background,
        b.text,
        b.accent
            .as_ref()
            .map(|a| format!(", and `{a}` as the accent for what matters"))
            .unwrap_or_default(),
        match b.fonts.first() {
            Some(f) => format!("Type is set in {f}."),
            None => String::new(),
        }
    ));
    out.push_str("## 2. Color Palette & Roles\n\n");
    let row = |name: &str, c: &Option<String>, what: &str| {
        c.as_ref()
            .map(|c| format!("- **{name}** (`{c}`): {what}\n"))
            .unwrap_or_default()
    };
    out.push_str(&row(
        "Background",
        &Some(b.background.clone()),
        "the page's canvas.",
    ));
    out.push_str(&row(
        "Surface",
        &b.surface,
        "cards, panels and sections that step forward.",
    ));
    out.push_str(&row(
        "Text",
        &Some(b.text.clone()),
        &format!(
            "headings and body ({:.1}:1 on the background).",
            contrast(&b.text, &b.background)
        ),
    ));
    out.push_str(&row(
        "Muted text",
        &b.muted,
        "secondary labels and metadata.",
    ));
    out.push_str(&row(
        "Accent",
        &b.accent,
        "primary buttons, links and highlights; used sparingly.",
    ));
    for (i, c) in b.accents.iter().skip(1).enumerate() {
        out.push_str(&format!(
            "- **Accent {}** (`{c}`): a secondary brand colour.\n",
            i + 2
        ));
    }
    out.push_str(&row("Border", &b.border, "hairlines between parts."));
    out.push_str("\n## 3. Typography Rules\n\n");
    for (i, f) in b.fonts.iter().enumerate() {
        out.push_str(&format!(
            "- {} `{f}`\n",
            if i == 0 { "Main family:" } else { "Also:" }
        ));
    }
    if !b.sizes.is_empty() {
        out.push_str(&format!(
            "- Sizes in use (px): {}\n",
            b.sizes
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out.push_str("\n## 4. Component Stylings\n\n");
    if let Some(btn) = &b.button {
        out.push_str(&format!(
            "- **Primary button**: `{}` on `{}`, {}px corners, padding `{}`, {}px text{}.\n",
            btn.color.clone().unwrap_or_default(),
            btn.bg.clone().unwrap_or_default(),
            btn.radius,
            btn.padding,
            btn.size,
            if btn.text.is_empty() {
                String::new()
            } else {
                format!(" (\"{}\")", btn.text)
            }
        ));
    }
    if let Some(r) = b.radius {
        out.push_str(&format!("- Corners: {r}px most often.\n"));
    }
    if !b.vars.is_empty() {
        out.push_str("\n## 5. The site's own tokens\n\n```css\n:root {\n");
        for (k, v) in b.vars.iter().take(80) {
            out.push_str(&format!("  {k}: {v};\n"));
        }
        out.push_str("}\n```\n");
    }
    out.push_str("\n## Use\n\nUse `tokens.css` beside this file: its custom properties are these roles. Keep the accent rare; text on every surface meets WCAG AA. `capture.png` shows the site as captured.\n");
    out
}

/// The first few failed stylesheets' addresses, as ` (a, b)`; empty
/// when none had one.
pub fn named(failed: &[String]) -> String {
    let named: Vec<&str> = failed
        .iter()
        .map(String::as_str)
        .filter(|h| !h.starts_with('('))
        .take(3)
        .collect();
    if named.is_empty() {
        String::new()
    } else {
        format!(" ({})", named.join(", "))
    }
}

/// The brand as CSS custom properties.
pub fn tokens_css(b: &Brand) -> String {
    let mut out = String::from(":root {\n");
    let mut put = |k: &str, v: &Option<String>| {
        if let Some(v) = v {
            out.push_str(&format!("  --{k}: {v};\n"));
        }
    };
    put("color-bg", &Some(b.background.clone()));
    put("color-surface", &b.surface);
    put("color-text", &Some(b.text.clone()));
    put("color-text-muted", &b.muted);
    put("color-accent", &b.accent);
    put("color-border", &b.border);
    for (i, c) in b.accents.iter().skip(1).enumerate() {
        put(&format!("color-accent-{}", i + 2), &Some(c.clone()));
    }
    // Named as the library's systems name them.
    put(
        "font-body",
        &b.fonts.first().map(|f| {
            let back = generic(f);
            let back = back.strip_prefix(&format!("{f}, ")).unwrap_or(back);
            format!("\"{f}\", {back}")
        }),
    );
    put("radius", &b.radius.map(|r| format!("{r}px")));
    out.push_str("}\n");
    out
}

/// A page that shows the brand as read: its colour roles, its type at the
/// sizes in use, its button, and the site as captured (`capture.png`
/// beside it). The gallery takes its card picture from it.
pub fn preview_html(b: &Brand) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let font = b
        .fonts
        .first()
        .map(|f| format!("\"{}\", {}", esc(f), generic(f)))
        .unwrap_or_else(|| "system-ui, sans-serif".into());
    let surface = b.surface.clone().unwrap_or_else(|| b.background.clone());
    let muted = b.muted.clone().unwrap_or_else(|| b.text.clone());
    // No border read: a faint wash of the muted ink.
    let line = b
        .border
        .clone()
        .unwrap_or_else(|| format!("color-mix(in srgb, {muted} 28%, transparent)"));
    let mut roles: Vec<(&str, String)> = vec![("Background", b.background.clone())];
    roles.extend(b.surface.clone().map(|c| ("Surface", c)));
    roles.push(("Text", b.text.clone()));
    roles.extend(b.muted.clone().map(|c| ("Muted", c)));
    roles.extend(b.border.clone().map(|c| ("Border", c)));
    for (i, c) in b.accents.iter().enumerate() {
        roles.push((if i == 0 { "Accent" } else { "Accent, more" }, c.clone()));
    }
    let swatches: String = roles
        .iter()
        .map(|(role, c)| {
            format!(
                "<div class=\"sw\"><div class=\"chip\" style=\"background:{c}\"></div><b>{role}</b><code>{c}</code></div>"
            )
        })
        .collect();
    let mut sizes = b.sizes.clone();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    let specimen: String = sizes
        .iter()
        .take(5)
        .map(|px| format!("<div class=\"ty\"><code>{px}px</code><span style=\"font-size:{px}px\">{}</span></div>", esc(&b.name)))
        .collect();
    let button = b
        .button
        .as_ref()
        .map(|k| {
            format!(
                "<a class=\"btn\" style=\"background:{};color:{};border-radius:{}px;padding:{}\">{}</a>",
                k.bg.clone().unwrap_or_default(),
                k.color.clone().unwrap_or_else(|| b.background.clone()),
                k.radius,
                esc(&k.padding),
                esc(if k.text.trim().is_empty() { "Button" } else { k.text.trim() })
            )
        })
        .unwrap_or_default();
    let radius = b.radius.unwrap_or(8);
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>{name}</title>
<style>
body{{margin:0;background:{bg};color:{text};font-family:{font};}}
main{{max-width:1120px;margin:0 auto;padding:48px 40px;}}
h1{{font-size:44px;margin:0 0 6px;font-weight:600;}}
.src{{color:{muted};margin:0 0 36px;font-size:15px;}}
h2{{font-size:13px;letter-spacing:.06em;text-transform:uppercase;color:{muted};margin:36px 0 14px;font-weight:600;}}
.grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(150px,1fr));gap:14px;}}
.sw{{background:{surface};border:1px solid {line};border-radius:{radius}px;padding:10px;display:flex;flex-direction:column;gap:6px;font-size:13px;}}
.chip{{height:64px;border-radius:{inner}px;border:1px solid {line};}}
code{{font-family:ui-monospace,monospace;font-size:12px;color:{muted};}}
.ty{{display:flex;align-items:baseline;gap:18px;padding:8px 0;border-bottom:1px solid {line};white-space:nowrap;overflow:hidden;}}
.ty code{{width:48px;flex:none;}}
.btn{{display:inline-block;text-decoration:none;font-size:15px;}}
img{{width:100%;border:1px solid {line};border-radius:{radius}px;display:block;}}
</style></head>
<body><main>
<h1>{name}</h1>
<p class="src">Read from {source}</p>
<h2>Colour roles</h2><div class="grid">{swatches}</div>
<h2>Type</h2>{specimen}
<h2>Button</h2>{button}
<h2>As captured</h2><img src="capture.png" alt="">
</main></body></html>
"#,
        name = esc(&b.name),
        source = esc(&b.source),
        bg = b.background,
        text = b.text,
        inner = radius.saturating_sub(4),
    )
}

/// Where a capture of `url` goes in `project`: `brand/<host>/`.
pub fn folder(project: &Path, url: &str) -> PathBuf {
    let host: String = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("site")
        .trim_start_matches("www.")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    project
        .join("brand")
        .join(if host.is_empty() { "site".into() } else { host })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(v: &[(&str, f64)]) -> BTreeMap<String, f64> {
        v.iter().map(|(k, n)| (k.to_string(), *n)).collect()
    }

    #[test]
    fn the_name_is_the_part_of_the_title_that_names_the_site() {
        assert_eq!(brand_name("Pricing | Acme", "acme.test"), "Acme");
        assert_eq!(
            brand_name("ElevenLabs: AI voices", "elevenlabs.io"),
            "ElevenLabs"
        );
        assert_eq!(brand_name("Kiln Studio", "127.0.0.1:8765"), "Kiln Studio");
        assert_eq!(
            brand_name("Kiln Studio – hand-thrown pots", "kiln.test"),
            "Kiln Studio"
        );
        assert_eq!(brand_name("Real-time UI", "rt.test"), "Real-time UI");
        assert_eq!(brand_name("", "acme.test"), "acme.test");
    }

    #[test]
    fn muted_is_the_quieter_ink_even_when_it_covers_more() {
        let read = Read {
            ink: map(&[("#6b6259", 9000.0), ("#231f1b", 2000.0)]),
            body_bg: Some("#f6f1ea".into()),
            ..Read::default()
        };
        let b = brand(read, "https://kiln.test");
        assert_eq!(b.text, "#231f1b");
        assert_eq!(b.muted.as_deref(), Some("#6b6259"));
    }

    #[test]
    fn roles_come_from_what_the_page_draws() {
        let read = Read {
            title: "Acme | Rockets for everyone".into(),
            bg: map(&[
                ("#ffffff", 900_000.0),
                ("#f5f5f4", 200_000.0),
                ("#ff385c", 4_000.0),
            ]),
            ink: map(&[("#222222", 5000.0), ("#6a6a6a", 900.0), ("#ff385c", 40.0)]),
            border: map(&[("#dddddd", 3000.0)]),
            fonts: map(&[("Inter", 5000.0), ("Georgia", 300.0)]),
            sizes: map(&[("16", 4000.0), ("14", 900.0), ("32", 100.0)]),
            radii: map(&[("12", 30.0), ("4", 3.0)]),
            buttons: vec![Button {
                text: "Book".into(),
                bg: Some("#ff385c".into()),
                color: Some("#ffffff".into()),
                radius: 8,
                padding: "12px 24px".into(),
                size: 16,
            }],
            body_bg: Some("#ffffff".into()),
            ..Read::default()
        };
        let b = brand(read, "https://www.acme.test/en");
        assert_eq!(b.name, "Acme");
        assert_eq!(b.background, "#ffffff");
        assert_eq!(b.surface.as_deref(), Some("#f5f5f4"));
        assert_eq!(b.text, "#222222");
        assert_eq!(b.muted.as_deref(), Some("#6a6a6a"));
        assert_eq!(b.accent.as_deref(), Some("#ff385c"));
        assert_eq!(b.fonts[0], "Inter");
        assert_eq!(b.radius, Some(12));
        let md = design_md(&b);
        assert!(md.contains("## 2. Color Palette & Roles"));
        assert!(md.contains("**Accent** (`#ff385c`)"));
        assert!(md.contains("Primary button"));
        let page = preview_html(&b);
        assert!(page.contains("background:#ff385c"));
        assert!(page.contains("<h1>Acme</h1>"));
        assert!(page.contains("src=\"capture.png\""));
        let css = tokens_css(&b);
        assert!(css.contains("--color-accent: #ff385c;"));
        assert!(css.contains("--font-body: \"Inter\", system-ui, sans-serif;"));
        assert_eq!(generic("Georgia"), "Georgia, serif");
        assert_eq!(generic("Source Serif 4"), "Georgia, serif");
        assert_eq!(generic("IBM Plex Sans"), "system-ui, sans-serif");
        assert_eq!(generic("JetBrains Mono"), "ui-monospace, monospace");
        assert_eq!(
            folder(Path::new("/p"), "https://www.acme.test/en?x=1"),
            Path::new("/p/brand/acme.test")
        );
    }
}
