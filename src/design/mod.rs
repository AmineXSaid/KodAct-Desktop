//! Forge's design library: Open Design's skills, templates, design systems
//! and craft rules (`design-library/`, mirrored by
//! `scripts/sync_open_design.py`), indexed here so a design chat's system
//! prompt can be built from exactly what the run needs. The plan is
//! `docs/open-design-port.md`.

pub mod capture;
pub mod export;
pub mod fig;
pub mod frontmatter;
pub mod inline;
pub mod library;
pub mod lint;
pub mod live;
pub mod marks;
pub mod od_subset;
pub mod pets;
pub mod pptx;
pub mod prompt;
pub mod routines;
pub mod services;
pub mod skill_meta;
pub mod store;
pub mod trace;

use std::path::{Path, PathBuf};

use library::{Index, Kind};

/// The file every library has at its root: the sync's record of what was
/// taken from where.
const MARKER: &str = "UPSTREAM.json";

/// Where the design library is: `FORGE_DESIGN_LIB`, else `design-library`
/// beside the executable (where the installer puts it), else, in a
/// development build, the repository's own copy.
pub fn root() -> Option<PathBuf> {
    let is_lib = |p: &Path| p.join(MARKER).is_file();
    if let Some(p) = std::env::var_os("FORGE_DESIGN_LIB").map(PathBuf::from) {
        return is_lib(&p).then_some(p);
    }
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("design-library")));
    let dev = cfg!(debug_assertions)
        .then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("design-library"));
    [beside, dev].into_iter().flatten().find(|p| is_lib(p))
}

/// The design systems you captured from sites yourself
/// (`forge-gpui --design-capture`): `FORGE_DESIGN_SYSTEMS`, else
/// `<config>/design-systems`. They stay on this computer, beside the
/// library and never in it: each is a reading of someone else's brand,
/// for your own reference.
pub fn own_systems() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("FORGE_DESIGN_SYSTEMS") {
        return Some(PathBuf::from(d));
    }
    // Tests read only the library they were given.
    if cfg!(test) {
        return None;
    }
    crate::profile::dir().map(|d| d.join("design-systems"))
}

/// `forge-gpui --design-capture [--into <folder>] <site>…`: capture each
/// site's brand (`export::capture_at`) as one of your own design systems,
/// `<folder>/<host>/` (default `own_systems`), which Design → Design
/// systems then lists under "Captured by you". Exits 1 when any failed.
pub fn capture_cli(args: &[String]) -> i32 {
    #[cfg(windows)]
    attach_console();
    let (into, sites) = match args {
        [flag, dir, rest @ ..] if flag == "--into" => (Some(PathBuf::from(dir)), rest),
        rest => (own_systems(), rest),
    };
    let Some(into) = into else {
        eprintln!(
            "No folder for your design systems: set FORGE_DESIGN_SYSTEMS or pass --into <folder>."
        );
        return 1;
    };
    // `@list.txt`: one address per line (blank lines and `#` notes skipped).
    let mut list: Vec<String> = vec![];
    for a in sites {
        match a.strip_prefix('@') {
            Some(file) => match std::fs::read_to_string(file) {
                Ok(text) => list.extend(sites_in(&text)),
                Err(e) => {
                    eprintln!("Can't read {file}: {e}");
                    return 1;
                }
            },
            None => list.push(a.clone()),
        }
    }
    let sites = &list[..];
    if sites.is_empty() {
        eprintln!("Usage: forge-gpui --design-capture [--into <folder>] <site>… or @<list.txt>");
        return 1;
    }
    let mut failed = 0;
    for site in sites {
        let got = export::capture_at(site, |url| {
            let host = capture::folder(Path::new(""), url);
            into.join(host.file_name().unwrap_or_default())
        });
        match got {
            Ok(c) => {
                let b = &c.brand;
                println!(
                    "{site}: {} · {} colours, {} fonts → {}",
                    b.name,
                    1 + b.accents.len()
                        + [&b.surface, &b.muted, &b.border]
                            .iter()
                            .filter(|o| o.is_some())
                            .count(),
                    b.fonts.len(),
                    c.folder.display()
                );
                if !b.css_failed.is_empty() {
                    println!(
                        "  {} stylesheet(s) didn't load: read without them",
                        b.css_failed.len()
                    );
                }
            }
            Err(e) => {
                failed += 1;
                println!("{site}: failed: {e}");
            }
        }
    }
    println!(
        "{} of {} captured into {}",
        sites.len() - failed,
        sites.len(),
        into.display()
    );
    i32::from(failed > 0)
}

/// The addresses in a list file: one per line, without blanks or `#` notes.
fn sites_in(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// `forge-gpui --design-check`: load the whole library and print what is in
/// it and what could not be read. Exits 1 when anything failed (upstream's
/// own tolerated quirks are listed, but don't fail it).
///
/// `--design-check <kind> <id>` prints one entry instead, as the index read
/// it: `--design-check example html-ppt-course-module`.
pub fn check(args: &[String]) -> i32 {
    #[cfg(windows)]
    attach_console();
    let Some(root) = root() else {
        eprintln!("No design library: set FORGE_DESIGN_LIB, or put design-library beside the app.");
        return 1;
    };
    let ix = Index::load(&root);
    if let [kind, id, ..] = args {
        let Some(kind) = Kind::from_slug(kind) else {
            let kinds: Vec<_> = Kind::ALL.iter().map(|k| k.slug()).collect();
            eprintln!("Unknown kind {kind:?}: one of {}", kinds.join(", "));
            return 1;
        };
        let Some(e) = ix.get(kind, id) else {
            eprintln!("No {} named {id:?}", kind.slug());
            return 1;
        };
        println!("{e:#?}");
        return 0;
    }
    let od = &ix.upstream["open_design"];
    println!(
        "Design library at {} (Open Design {} of {})",
        root.display(),
        od["commit"].as_str().map_or("?", |c| &c[..c.len().min(12)]),
        od["date"].as_str().unwrap_or("?"),
    );
    let counts = ix.counts();
    for kind in Kind::ALL {
        println!(
            "  {:>4} {}",
            counts.get(&kind).copied().unwrap_or(0),
            kind.label()
        );
    }
    let shipped = ix.of(Kind::Skill).filter(|e| e.upstream_shipped).count();
    let links = ix
        .of(Kind::Skill)
        .filter(|e| e.catalogue && !e.upstream_shipped)
        .count();
    println!("  upstream skills: {shipped} shipped, {links} kept as links");
    let tolerated: Vec<_> = ix.problems.iter().filter(|p| p.tolerated).collect();
    let failed: Vec<_> = ix.errors().collect();
    for (title, list) in [("Tolerated", &tolerated), ("Problems", &failed)] {
        if !list.is_empty() {
            println!("{title} ({}):", list.len());
            for p in list.iter() {
                println!("  {}: {}", p.path.display(), p.message);
            }
        }
    }
    i32::from(!failed.is_empty())
}

/// A release build on Windows is a window app with no console of its own:
/// borrow the one `--design-check` was started from, so its report shows.
#[cfg(windows)]
fn attach_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // SAFETY: a plain Win32 call with no pointers; failing (no parent
    // console, or one already attached) leaves things as they were.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// A `file://` address's path, its %-escapes decoded.
pub fn inline_path(url_path: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(inline::decode(
        url_path.split(['?', '#']).next().unwrap_or(url_path),
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_site_list_skips_blanks_and_notes() {
        let got = super::sites_in("# brands\nhttps://a.test\n\n  b.test  # the shop\n");
        assert_eq!(got, ["https://a.test", "b.test"]);
    }
}
