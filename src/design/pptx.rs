//! A PowerPoint file from a deck's slides, written from scratch (Office
//! Open XML, zipped): each slide is a picture of the HTML slide filling it,
//! and, for an editable deck, the slide's text as text boxes over a picture
//! taken with the text hidden. Slides are 16:9 at 13.333 by 7.5 inches.

use std::io::Write;

/// EMUs (English Metric Units) across a 16:9 PowerPoint slide.
const SLIDE_W: i64 = 12_192_000;
const SLIDE_H: i64 = 6_858_000;

/// A run of text on a slide, where the HTML slide drew it: its box in the
/// slide's pixels and how it looked.
#[derive(Clone, Debug, serde::Deserialize, PartialEq)]
pub struct TextBox {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Font size, in the slide's pixels.
    pub size: f64,
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    /// `rrggbb`.
    pub color: String,
    pub font: String,
    /// `l`, `ctr` or `r`.
    pub align: String,
}

/// One slide: its picture (PNG) at `width` by `height` pixels, and its
/// text boxes (none for a picture-only deck), and speaker notes.
pub struct Slide {
    pub png: Vec<u8>,
    pub width: f64,
    pub height: f64,
    pub text: Vec<TextBox>,
    pub notes: String,
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Control characters aren't allowed in XML.
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

const XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;
const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn content_types(n: usize) -> String {
    let slides: String = (1..=n)
        .map(|i| format!(r#"<Override PartName="/ppt/slides/slide{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/><Override PartName="/ppt/notesSlides/notesSlide{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"/>"#))
        .collect();
    format!(
        r#"{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/notesMasters/notesMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/><Override PartName="/ppt/theme/theme2.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>{slides}</Types>"#
    )
}

fn root_rels() -> String {
    format!(
        r#"{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="ppt/presentation.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="{REL}/extended-properties" Target="docProps/app.xml"/></Relationships>"#
    )
}

fn core(title: &str) -> String {
    format!(
        r#"{XML}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title><dc:creator>KodAct</dc:creator></cp:coreProperties>"#,
        esc(title)
    )
}

fn app(n: usize) -> String {
    format!(
        r#"{XML}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>KodAct</Application><Slides>{n}</Slides></Properties>"#
    )
}

fn presentation(n: usize) -> String {
    let ids: String = (1..=n)
        .map(|i| format!(r#"<p:sldId id="{}" r:id="rId{}"/>"#, 255 + i, i + 3))
        .collect();
    format!(
        r#"{XML}<p:presentation {NS} saveSubsetFonts="1"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:notesMasterIdLst><p:notesMasterId r:id="rId3"/></p:notesMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx="{SLIDE_W}" cy="{SLIDE_H}"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#
    )
}

fn presentation_rels(n: usize) -> String {
    let slides: String = (1..=n)
        .map(|i| {
            format!(
                r#"<Relationship Id="rId{}" Type="{REL}/slide" Target="slides/slide{i}.xml"/>"#,
                i + 3
            )
        })
        .collect();
    format!(
        r#"{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/slideMaster" Target="slideMasters/slideMaster1.xml"/><Relationship Id="rId2" Type="{REL}/theme" Target="theme/theme1.xml"/><Relationship Id="rId3" Type="{REL}/notesMaster" Target="notesMasters/notesMaster1.xml"/>{slides}</Relationships>"#
    )
}

/// A shape tree's required group properties.
const GROUP: &str = r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>"#;

fn master() -> String {
    format!(
        r#"{XML}<p:sldMaster {NS}><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree>{GROUP}</p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"#
    )
}

fn master_rels() -> String {
    format!(
        r#"{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="{REL}/theme" Target="../theme/theme1.xml"/></Relationships>"#
    )
}

fn layout() -> String {
    format!(
        r#"{XML}<p:sldLayout {NS} type="blank" preserve="1"><p:cSld name="Blank"><p:spTree>{GROUP}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#
    )
}

fn layout_rels() -> String {
    format!(
        r#"{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#
    )
}

fn notes_master() -> String {
    format!(
        r#"{XML}<p:notesMaster {NS}><p:cSld><p:spTree>{GROUP}</p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/></p:notesMaster>"#
    )
}

fn notes_master_rels() -> String {
    format!(
        r#"{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/theme" Target="../theme/theme2.xml"/></Relationships>"#
    )
}

/// A plain Office theme: what a master needs to open.
fn theme(name: &str) -> String {
    let fill = r#"<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>"#;
    let line = r#"<a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>"#;
    let effect = "<a:effectStyle><a:effectLst/></a:effectStyle>";
    let colours = [
        ("dk1", "000000"),
        ("lt1", "FFFFFF"),
        ("dk2", "1F2937"),
        ("lt2", "F3F4F6"),
        ("accent1", "2563EB"),
        ("accent2", "DB2777"),
        ("accent3", "059669"),
        ("accent4", "D97706"),
        ("accent5", "7C3AED"),
        ("accent6", "0891B2"),
        ("hlink", "2563EB"),
        ("folHlink", "7C3AED"),
    ]
    .iter()
    .map(|(k, v)| format!(r#"<a:{k}><a:srgbClr val="{v}"/></a:{k}>"#))
    .collect::<String>();
    format!(
        r#"{XML}<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="{name}"><a:themeElements><a:clrScheme name="KodAct">{colours}</a:clrScheme><a:fontScheme name="KodAct"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="KodAct"><a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst><a:lnStyleLst>{line}{line}{line}</a:lnStyleLst><a:effectStyleLst>{effect}{effect}{effect}</a:effectStyleLst><a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#
    )
}

/// A text box's shape, at the place the HTML drew it.
fn text_shape(id: usize, b: &TextBox, sx: f64, sy: f64) -> String {
    let emu = |v: f64, s: f64| (v * s).round().max(0.0) as i64;
    // A little room: PowerPoint's fonts and the browser's don't measure
    // the same.
    let (x, y) = (emu(b.x, sx), emu(b.y, sy));
    let (w, h) = (emu(b.w * 1.04 + 4.0, sx), emu(b.h, sy));
    // Pixels on a 13.333 in slide → hundredths of a point.
    let size = ((b.size * sx / 12_700.0) * 100.0)
        .round()
        .clamp(100.0, 400_000.0) as i64;
    let colour = if b.color.len() == 6 && b.color.chars().all(|c| c.is_ascii_hexdigit()) {
        b.color.to_uppercase()
    } else {
        "000000".into()
    };
    let font = b
        .font
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(['"', '\'']);
    let font = if font.is_empty() { "Calibri" } else { font };
    let algn = match b.align.as_str() {
        "ctr" | "r" | "just" => b.align.as_str(),
        _ => "l",
    };
    let paragraphs: String = b
        .text
        .split('\n')
        .map(|line| {
            let run = if line.is_empty() {
                format!(r#"<a:endParaRPr lang="en-US" sz="{size}"/>"#)
            } else {
                format!(
                    r#"<a:r><a:rPr lang="en-US" sz="{size}" b="{}" i="{}" dirty="0"><a:solidFill><a:srgbClr val="{colour}"/></a:solidFill><a:latin typeface="{}"/><a:cs typeface="{}"/></a:rPr><a:t>{}</a:t></a:r>"#,
                    u8::from(b.bold),
                    u8::from(b.italic),
                    esc(font),
                    esc(font),
                    esc(line)
                )
            };
            format!(r#"<a:p><a:pPr algn="{algn}"/>{run}</a:p>"#)
        })
        .collect();
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="Text {id}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="square" lIns="0" tIns="0" rIns="0" bIns="0" anchor="t"><a:noAutofit/></a:bodyPr><a:lstStyle/>{paragraphs}</p:txBody></p:sp>"#
    )
}

fn slide_xml(s: &Slide) -> String {
    let (sx, sy) = (
        SLIDE_W as f64 / s.width.max(1.0),
        SLIDE_H as f64 / s.height.max(1.0),
    );
    let texts: String = s
        .text
        .iter()
        .enumerate()
        .map(|(i, b)| text_shape(i + 3, b, sx, sy))
        .collect();
    format!(
        r#"{XML}<p:sld {NS}><p:cSld><p:spTree>{GROUP}<p:pic><p:nvPicPr><p:cNvPr id="2" name="Slide"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId2"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{SLIDE_W}" cy="{SLIDE_H}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>{texts}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#
    )
}

fn slide_rels(i: usize) -> String {
    format!(
        r#"{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="{REL}/image" Target="../media/slide{i}.png"/><Relationship Id="rId3" Type="{REL}/notesSlide" Target="../notesSlides/notesSlide{i}.xml"/></Relationships>"#
    )
}

fn notes_xml(text: &str) -> String {
    let paragraphs: String = text
        .lines()
        .map(|l| {
            format!(
                r#"<a:p><a:r><a:rPr lang="en-US"/><a:t>{}</a:t></a:r></a:p>"#,
                esc(l)
            )
        })
        .collect();
    let paragraphs = if paragraphs.is_empty() {
        "<a:p/>".to_string()
    } else {
        paragraphs
    };
    format!(
        r#"{XML}<p:notes {NS}><p:cSld><p:spTree>{GROUP}<p:sp><p:nvSpPr><p:cNvPr id="2" name="Notes"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="685800" y="4400550"/><a:ext cx="5486400" cy="3600450"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>{paragraphs}</p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#
    )
}

fn notes_rels(i: usize) -> String {
    format!(
        r#"{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/notesMaster" Target="../notesMasters/notesMaster1.xml"/><Relationship Id="rId2" Type="{REL}/slide" Target="../slides/slide{i}.xml"/></Relationships>"#
    )
}

/// The `.pptx` file's bytes.
pub fn write(title: &str, slides: &[Slide]) -> Result<Vec<u8>, String> {
    let n = slides.len();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let deflate = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    // PNGs are compressed already.
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut put = |name: &str, bytes: &[u8], opts| -> Result<(), String> {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())
    };
    put("[Content_Types].xml", content_types(n).as_bytes(), deflate)?;
    put("_rels/.rels", root_rels().as_bytes(), deflate)?;
    put("docProps/core.xml", core(title).as_bytes(), deflate)?;
    put("docProps/app.xml", app(n).as_bytes(), deflate)?;
    put("ppt/presentation.xml", presentation(n).as_bytes(), deflate)?;
    put(
        "ppt/_rels/presentation.xml.rels",
        presentation_rels(n).as_bytes(),
        deflate,
    )?;
    put(
        "ppt/slideMasters/slideMaster1.xml",
        master().as_bytes(),
        deflate,
    )?;
    put(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        master_rels().as_bytes(),
        deflate,
    )?;
    put(
        "ppt/slideLayouts/slideLayout1.xml",
        layout().as_bytes(),
        deflate,
    )?;
    put(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        layout_rels().as_bytes(),
        deflate,
    )?;
    put(
        "ppt/notesMasters/notesMaster1.xml",
        notes_master().as_bytes(),
        deflate,
    )?;
    put(
        "ppt/notesMasters/_rels/notesMaster1.xml.rels",
        notes_master_rels().as_bytes(),
        deflate,
    )?;
    put("ppt/theme/theme1.xml", theme("KodAct").as_bytes(), deflate)?;
    put(
        "ppt/theme/theme2.xml",
        theme("KodAct Notes").as_bytes(),
        deflate,
    )?;
    for (i, s) in slides.iter().enumerate() {
        let i = i + 1;
        put(
            &format!("ppt/slides/slide{i}.xml"),
            slide_xml(s).as_bytes(),
            deflate,
        )?;
        put(
            &format!("ppt/slides/_rels/slide{i}.xml.rels"),
            slide_rels(i).as_bytes(),
            deflate,
        )?;
        put(
            &format!("ppt/notesSlides/notesSlide{i}.xml"),
            notes_xml(&s.notes).as_bytes(),
            deflate,
        )?;
        put(
            &format!("ppt/notesSlides/_rels/notesSlide{i}.xml.rels"),
            notes_rels(i).as_bytes(),
            deflate,
        )?;
        put(&format!("ppt/media/slide{i}.png"), &s.png, stored)?;
    }
    let cursor = zip.finish().map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(16, 9, image::Rgba([20, 40, 200, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_deck_reads_back_with_its_slides_text_and_notes() {
        let slides: Vec<Slide> = (0..3)
            .map(|i| Slide {
                png: png(),
                width: 1920.0,
                height: 1080.0,
                text: vec![TextBox {
                    text: format!("Slide {i} & <more>\nsecond line"),
                    x: 120.0,
                    y: 100.0,
                    w: 800.0,
                    h: 200.0,
                    size: 64.0,
                    bold: true,
                    italic: false,
                    color: "1a2b3c".into(),
                    font: "\"Inter\", sans-serif".into(),
                    align: "l".into(),
                }],
                notes: "Say hello".into(),
            })
            .collect();
        let bytes = write("Deck", &slides).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        fn read(z: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str) -> String {
            let mut s = String::new();
            z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
            s
        }
        let pres = read(&mut z, "ppt/presentation.xml");
        assert_eq!(pres.matches("<p:sldId ").count(), 3);
        let s2 = read(&mut z, "ppt/slides/slide2.xml");
        assert!(s2.contains("Slide 1 &amp; &lt;more&gt;"), "{s2}");
        assert!(s2.contains(r#"typeface="Inter""#));
        // 64 px on a 1920 px slide: 32 pt.
        assert!(s2.contains(r#"sz="3200""#), "{s2}");
        assert!(s2.contains("1A2B3C"));
        assert!(read(&mut z, "ppt/notesSlides/notesSlide3.xml").contains("Say hello"));
        // Every part the content types name is in the file.
        let types = read(&mut z, "[Content_Types].xml");
        for part in types.split("PartName=\"/").skip(1) {
            let name = part.split('"').next().unwrap();
            assert!(z.by_name(name).is_ok(), "{name} missing");
        }
        // Every XML part is well formed.
        let names: Vec<String> = z.file_names().map(str::to_string).collect();
        for name in names
            .iter()
            .filter(|n| n.ends_with(".xml") || n.ends_with(".rels"))
        {
            let xml = read(&mut z, name);
            let mut r = quick_xml::Reader::from_str(&xml);
            loop {
                match r.read_event() {
                    Ok(quick_xml::events::Event::Eof) => break,
                    Ok(_) => {}
                    Err(e) => panic!("{name}: {e}"),
                }
            }
        }
    }
}
