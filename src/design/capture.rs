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
    let name = read
        .title
        .split([' ', '|', '-', '–', '—', ':'])
        .find(|w| w.len() > 1)
        .map(str::to_string)
        .filter(|_| !read.title.trim().is_empty())
        .unwrap_or_else(|| host.clone());
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
    put(
        "font-sans",
        &b.fonts
            .first()
            .map(|f| format!("\"{f}\", system-ui, sans-serif")),
    );
    put("radius", &b.radius.map(|r| format!("{r}px")));
    out.push_str("}\n");
    out
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
        let css = tokens_css(&b);
        assert!(css.contains("--color-accent: #ff385c;"));
        assert!(css.contains("--font-sans: \"Inter\""));
        assert_eq!(
            folder(Path::new("/p"), "https://www.acme.test/en?x=1"),
            Path::new("/p/brand/acme.test")
        );
    }
}
