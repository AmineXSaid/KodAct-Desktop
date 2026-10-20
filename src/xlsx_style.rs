//! What calamine leaves out of a workbook: how each cell looks and what
//! it computes. Read straight from the file's XML, so the side panel's grid
//! looks as the workbook does in Excel: number formats ("5,500", "6.8%",
//! "€ 1.20", dates), bold and italic, fills and font colours, column widths,
//! the frozen header, and each cell's formula for the formula bar.

use std::collections::HashMap;
use std::io::Cursor;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

/// How one cell looks, and its formula.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub bold: bool,
    pub italic: bool,
    /// Its fill and font colour as RGB, when set.
    pub fill: Option<u32>,
    pub color: Option<u32>,
    /// Its number format code (`#,##0`, `0.0%`), when not General.
    pub format: Option<String>,
    pub formula: Option<String>,
}

/// One sheet's looks, by (row, column) from 0, and its layout.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SheetLook {
    pub cells: HashMap<(usize, usize), Look>,
    /// Column widths in Excel's units (characters), by column from 0.
    pub widths: HashMap<usize, f32>,
    /// Rows frozen at the top (the header that stays while you scroll).
    pub frozen_rows: usize,
}

/// Every sheet's looks, by sheet name. Empty for a file that is not an
/// .xlsx (an .xls or .ods shows its values only).
pub fn read(bytes: &[u8]) -> HashMap<String, SheetLook> {
    let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        return HashMap::new();
    };
    let mut text = |path: &str| -> Option<String> {
        let mut f = zip.by_name(path).ok()?;
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s).ok()?;
        Some(s)
    };
    let styles = text("xl/styles.xml")
        .map(|x| styles(&x))
        .unwrap_or_default();
    let Some(book) = text("xl/workbook.xml") else {
        return HashMap::new();
    };
    let rels = text("xl/_rels/workbook.xml.rels").unwrap_or_default();
    let targets = relationships(&rels);
    let mut out = HashMap::new();
    for (name, rid) in sheet_ids(&book) {
        let Some(target) = targets.get(&rid) else {
            continue;
        };
        let path = if let Some(abs) = target.strip_prefix('/') {
            abs.to_string()
        } else {
            format!("xl/{target}")
        };
        if let Some(xml) = text(&path) {
            out.insert(name, sheet(&xml, &styles));
        }
    }
    out
}

fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
    let a = e
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key || a.key.local_name().as_ref() == key)?;
    let raw = std::str::from_utf8(&a.value).ok()?;
    Some(
        quick_xml::escape::unescape(raw)
            .map(|v| v.into_owned())
            .unwrap_or_else(|_| raw.to_string()),
    )
}

/// An ARGB colour attribute (`FF4472C4`) as RGB.
fn rgb(e: &BytesStart) -> Option<u32> {
    let v = attr(e, b"rgb")?;
    let hex = if v.len() == 8 { &v[2..] } else { &v };
    u32::from_str_radix(hex, 16).ok()
}

/// A cell style (`cellXfs`) as the parts a cell needs.
#[derive(Clone, Default)]
struct Xf {
    bold: bool,
    italic: bool,
    fill: Option<u32>,
    color: Option<u32>,
    format: Option<String>,
}

fn styles(xml: &str) -> Vec<Xf> {
    let mut formats: HashMap<u32, String> = HashMap::new();
    let mut fonts: Vec<(bool, bool, Option<u32>)> = vec![];
    let mut fills: Vec<Option<u32>> = vec![];
    let mut xfs = vec![];
    let (mut section, mut in_fill_pattern) = (Vec::<Vec<u8>>::new(), false);
    let mut reader = Reader::from_str(xml);
    loop {
        let ev = reader.read_event();
        let (e, empty) = match &ev {
            Ok(Event::Start(e)) => (e.clone(), false),
            Ok(Event::Empty(e)) => (e.clone(), true),
            Ok(Event::End(e)) => {
                if e.local_name().as_ref() == b"patternFill" {
                    in_fill_pattern = false;
                }
                section.pop();
                continue;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => continue,
        };
        let name = e.local_name().as_ref().to_vec();
        let parent = section.last().cloned().unwrap_or_default();
        match name.as_slice() {
            b"numFmt" => {
                if let (Some(id), Some(code)) = (
                    attr(&e, b"numFmtId").and_then(|i| i.parse().ok()),
                    attr(&e, b"formatCode"),
                ) {
                    formats.insert(id, code);
                }
            }
            b"font" if parent == b"fonts" => fonts.push((false, false, None)),
            b"b" if parent == b"font" => {
                if let Some(f) = fonts.last_mut() {
                    f.0 = attr(&e, b"val").is_none_or(|v| v != "0" && v != "false");
                }
            }
            b"i" if parent == b"font" => {
                if let Some(f) = fonts.last_mut() {
                    f.1 = attr(&e, b"val").is_none_or(|v| v != "0" && v != "false");
                }
            }
            b"color" if parent == b"font" => {
                if let Some(f) = fonts.last_mut() {
                    f.2 = rgb(&e);
                }
            }
            b"fill" if parent == b"fills" => fills.push(None),
            b"patternFill" => {
                in_fill_pattern = attr(&e, b"patternType").as_deref() == Some("solid");
            }
            b"fgColor" if in_fill_pattern => {
                if let Some(f) = fills.last_mut() {
                    *f = rgb(&e);
                }
            }
            b"xf" if parent == b"cellXfs" => {
                let num = attr(&e, b"numFmtId")
                    .and_then(|i| i.parse::<u32>().ok())
                    .unwrap_or(0);
                let font = attr(&e, b"fontId")
                    .and_then(|i| i.parse::<usize>().ok())
                    .unwrap_or(0);
                let fill = attr(&e, b"fillId")
                    .and_then(|i| i.parse::<usize>().ok())
                    .unwrap_or(0);
                let (bold, italic, color) = fonts.get(font).copied().unwrap_or_default();
                xfs.push(Xf {
                    bold,
                    italic,
                    fill: fills.get(fill).copied().flatten(),
                    color: color.filter(|c| *c != 0x000000),
                    format: formats
                        .get(&num)
                        .cloned()
                        .or_else(|| builtin(num).map(str::to_string))
                        // General shows the value as it is.
                        .filter(|f| !f.eq_ignore_ascii_case("general") && f != "@"),
                });
            }
            _ => {}
        }
        if !empty {
            section.push(name);
        }
    }
    xfs
}

/// Excel's built-in number formats, by id (the ones worth showing).
fn builtin(id: u32) -> Option<&'static str> {
    Some(match id {
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        14 => "yyyy-mm-dd",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "yyyy-mm-dd h:mm",
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[Red](#,##0.00)",
        _ => return None,
    })
}

/// Relationship ids to targets.
fn relationships(xml: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(Event::Empty(e)) | Ok(Event::Start(e))
                if e.local_name().as_ref() == b"Relationship" =>
            {
                if let (Some(id), Some(t)) = (attr(&e, b"Id"), attr(&e, b"Target")) {
                    out.insert(id, t);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// The workbook's sheets: name and relationship id, in order.
fn sheet_ids(xml: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) if e.local_name().as_ref() == b"sheet" => {
                if let (Some(n), Some(id)) = (attr(&e, b"name"), attr(&e, b"id")) {
                    out.push((n, id));
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// `B7` as (row 6, column 1).
pub fn cell_ref(a1: &str) -> Option<(usize, usize)> {
    let letters: String = a1.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let row: usize = a1[letters.len()..].parse().ok()?;
    let col = letters.bytes().try_fold(0usize, |n, b| {
        Some(n * 26 + (b.to_ascii_uppercase().checked_sub(b'A')? as usize + 1))
    })?;
    Some((row.checked_sub(1)?, col.checked_sub(1)?))
}

fn sheet(xml: &str, styles: &[Xf]) -> SheetLook {
    let mut look = SheetLook::default();
    let mut reader = Reader::from_str(xml);
    let mut current: Option<(usize, usize)> = None;
    let mut in_formula = false;
    let mut shared: HashMap<String, String> = HashMap::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"col" => {
                    let (min, max, width) = (
                        attr(&e, b"min").and_then(|v| v.parse::<usize>().ok()),
                        attr(&e, b"max").and_then(|v| v.parse::<usize>().ok()),
                        attr(&e, b"width").and_then(|v| v.parse::<f32>().ok()),
                    );
                    if let (Some(min), Some(max), Some(w)) = (min, max, width) {
                        for c in min..=max.min(min + 200) {
                            look.widths.insert(c - 1, w);
                        }
                    }
                }
                b"pane" => {
                    if attr(&e, b"state")
                        .as_deref()
                        .is_some_and(|s| s.starts_with("frozen"))
                    {
                        look.frozen_rows = attr(&e, b"ySplit")
                            .and_then(|v| v.parse::<f32>().ok())
                            .map_or(0, |v| v as usize);
                    }
                }
                b"c" => {
                    current = attr(&e, b"r").and_then(|r| cell_ref(&r));
                    if let (Some(at), Some(s)) = (
                        current,
                        attr(&e, b"s").and_then(|s| s.parse::<usize>().ok()),
                    ) {
                        if let Some(x) = styles.get(s) {
                            let l = look.cells.entry(at).or_default();
                            (l.bold, l.italic, l.fill, l.color) =
                                (x.bold, x.italic, x.fill, x.color);
                            l.format = x.format.clone();
                        }
                    }
                }
                b"f" => {
                    in_formula = true;
                    // A shared formula's later cells only point at it: show
                    // the first's text (its references shift in Excel).
                    if let (Some(si), Some(at)) = (attr(&e, b"si"), current) {
                        if attr(&e, b"ref").is_none() {
                            if let Some(f) = shared.get(&si) {
                                look.cells.entry(at).or_default().formula = Some(f.clone());
                            }
                        } else {
                            shared.insert(si, String::new());
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Text(t)) if in_formula => {
                if let Some(at) = current {
                    let f = format!("={}", t.decode().unwrap_or_default());
                    for v in shared.values_mut().filter(|v| v.is_empty()) {
                        *v = f.clone();
                    }
                    look.cells.entry(at).or_default().formula = Some(f);
                }
            }
            Ok(Event::GeneralRef(r)) if in_formula => {
                if let Some(f) = current
                    .and_then(|at| look.cells.get_mut(&at))
                    .and_then(|l| l.formula.as_mut())
                {
                    f.push_str(match r.decode().unwrap_or_default().as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        _ => "'",
                    });
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"f" => in_formula = false,
                b"c" => current = None,
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    look
}

/// `value` written in the number format `code`, as Excel shows it.
pub fn format(value: f64, code: &str) -> String {
    let sections: Vec<&str> = split_sections(code);
    let (section, v) = match (sections.len(), value) {
        (n, v) if v < 0. && n >= 2 => (sections[1], -v),
        (n, v) if v == 0. && n >= 3 => (sections[2], v),
        (_, v) => (sections[0], v),
    };
    let negative_sign = value < 0. && sections.len() < 2;
    // The sign is written once, before the prefix, not inside the digits.
    let v = v.abs();
    let cleaned = clean(section);
    if is_date(&cleaned) {
        return date(value, &cleaned);
    }
    // The digits' pattern and the literals around it.
    let chars: Vec<char> = cleaned.chars().collect();
    let start = chars.iter().position(|c| matches!(c, '0' | '#' | '?'));
    let Some(start) = start else {
        return cleaned.replace('@', "");
    };
    let end = chars
        .iter()
        .rposition(|c| matches!(c, '0' | '#' | '?'))
        .unwrap_or(start);
    let pattern: String = chars[start..=end].iter().collect();
    let (prefix, suffix): (String, String) = (
        chars[..start].iter().collect(),
        chars[end + 1..].iter().collect(),
    );
    let percent = cleaned.contains('%');
    let v = if percent { v * 100. } else { v };
    let (int_part, frac_part) = pattern.split_once('.').unwrap_or((&pattern, ""));
    let decimals = frac_part
        .chars()
        .filter(|c| matches!(c, '0' | '#' | '?'))
        .count();
    if pattern.contains('E') || cleaned.contains("E+") {
        return format!(
            "{}{:.*E}",
            if negative_sign { "-" } else { "" },
            decimals,
            v
        );
    }
    let grouped = int_part.contains(',');
    let fixed = format!("{v:.decimals$}");
    let (whole, frac) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let whole = if grouped {
        group(whole)
    } else {
        whole.to_string()
    };
    let mut out = String::new();
    if negative_sign {
        out.push('-');
    }
    out.push_str(&prefix);
    out.push_str(&whole);
    if decimals > 0 {
        out.push('.');
        out.push_str(frac);
    }
    out.push_str(&suffix);
    out.trim().to_string()
}

/// A format code's sections (positive; negative; zero; text), split on `;`
/// outside quotes.
fn split_sections(code: &str) -> Vec<&str> {
    let mut out = vec![];
    let (mut start, mut quoted) = (0, false);
    for (i, c) in code.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => {
                out.push(&code[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&code[start..]);
    out
}

/// A section with its codes made plain: quoted text kept, colours and
/// conditions dropped, a currency tag (`[$€-407]`) as its symbol, padding
/// (`_)`) and fill (`*-`) removed, escapes (`\ `) as their character.
fn clean(section: &str) -> String {
    let mut out = String::new();
    let mut it = section.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                for q in it.by_ref() {
                    if q == '"' {
                        break;
                    }
                    out.push(q);
                }
            }
            '[' => {
                let inner: String = it.by_ref().take_while(|&b| b != ']').collect();
                if let Some(cur) = inner.strip_prefix('$') {
                    out.push_str(cur.split('-').next().unwrap_or_default());
                } else if inner.chars().all(|c| matches!(c, 'h' | 'm' | 's')) {
                    out.push_str(&inner);
                }
            }
            '_' => {
                it.next();
            }
            '*' => {
                it.next();
            }
            '\\' => {
                if let Some(n) = it.next() {
                    out.push(n);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

fn is_date(code: &str) -> bool {
    let lower = code.to_ascii_lowercase();
    !lower.contains('0')
        && !lower.contains('#')
        && lower
            .chars()
            .any(|c| matches!(c, 'y' | 'd' | 'h' | 'm' | 's'))
}

/// An Excel date serial (days since 1899-12-30) in a date or time format.
fn date(serial: f64, code: &str) -> String {
    let days = serial.floor() as i64;
    let secs = ((serial - serial.floor()) * 86_400.).round() as i64;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days - 25_569 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let (hh, mm, ss) = (secs / 3600, secs / 60 % 60, secs % 60);
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let month = MONTHS[(m - 1).clamp(0, 11) as usize];
    let ampm = code.to_ascii_uppercase().contains("AM/PM");
    let lower = code.to_ascii_lowercase().replace("am/pm", "\u{1}");
    let mut out = String::new();
    let chars: Vec<char> = lower.chars().collect();
    let mut i = 0;
    let mut last_was_hour = false;
    while i < chars.len() {
        let c = chars[i];
        let run = chars[i..].iter().take_while(|&&x| x == c).count();
        match c {
            'y' => out.push_str(&if run <= 2 {
                format!("{:02}", y % 100)
            } else {
                y.to_string()
            }),
            'd' => out.push_str(&if run == 1 {
                d.to_string()
            } else {
                format!("{d:02}")
            }),
            'm' if last_was_hour
                || chars[i + run..].iter().find(|c| c.is_ascii_alphabetic()) == Some(&'s') =>
            {
                out.push_str(&format!("{mm:02}"));
            }
            'm' => out.push_str(&match run {
                1 => m.to_string(),
                2 => format!("{m:02}"),
                3 => month[..3].to_string(),
                _ => month.to_string(),
            }),
            'h' => {
                let h = if ampm { (hh + 11) % 12 + 1 } else { hh };
                out.push_str(&if run == 1 {
                    h.to_string()
                } else {
                    format!("{h:02}")
                });
            }
            's' => out.push_str(&format!("{ss:02}")),
            '\u{1}' => out.push_str(if hh < 12 { "AM" } else { "PM" }),
            _ => out.extend(std::iter::repeat_n(c, run)),
        }
        if c.is_ascii_alphabetic() {
            last_was_hour = c == 'h';
        }
        i += run;
    }
    out
}

/// "1234567" as "1,234,567".
fn group(digits: &str) -> String {
    let (sign, digits) = digits.strip_prefix('-').map_or(("", digits), |d| ("-", d));
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    format!("{sign}{out}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_as_excel_shows_them() {
        assert_eq!(format(5500., "#,##0"), "5,500");
        assert_eq!(format(1234.5, "#,##0.00"), "1,234.50");
        assert_eq!(format(0.068, "0.0%"), "6.8%");
        assert_eq!(format(-350., "#,##0;(#,##0)"), "(350)");
        assert_eq!(format(-350., "#,##0"), "-350");
        assert_eq!(format(1.2, "[$€-407] #,##0.00"), "€ 1.20");
        assert_eq!(format(1200., "\"$\"#,##0_);[Red](\"$\"#,##0)"), "$1,200");
        assert_eq!(format(0., "#,##0;(#,##0);-"), "-");
    }

    #[test]
    fn dates_read_as_dates() {
        assert_eq!(format(45_566., "yyyy-mm-dd"), "2024-10-01");
        assert_eq!(format(45_566., "d-mmm-yy"), "1-Oct-24");
        assert_eq!(format(45_566.5, "h:mm"), "12:00");
        assert_eq!(format(45_566.75, "h:mm AM/PM"), "6:00 PM");
    }

    #[test]
    fn cell_references_read_as_row_and_column() {
        assert_eq!(cell_ref("A1"), Some((0, 0)));
        assert_eq!(cell_ref("B7"), Some((6, 1)));
        assert_eq!(cell_ref("AA10"), Some((9, 26)));
        assert_eq!(cell_ref("7"), None);
    }

    #[test]
    fn reads_the_budget_workbook_formulas_and_looks() {
        let xlsx = include_bytes!("../assets/docs/samples/q3-budget.xlsx");
        let looks = read(xlsx);
        let budget = &looks["Budget"];
        assert_eq!(
            budget
                .cells
                .get(&(6, 1))
                .and_then(|l| l.formula.clone())
                .as_deref(),
            Some("=SUM(B2:B6)")
        );
        assert!(
            budget.cells.get(&(0, 0)).is_some_and(|l| l.bold),
            "the header is bold"
        );
    }
}
