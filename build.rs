//! Embeds KodAct's plugin (`assets/kodact-plugin`: the Diagram Design,
//! design and office-documents skills, the Expert output style) so the app can unpack
//! it next to its config and hand it to every KodAct CLI it starts
//! (`src/plugin.rs`). The list and a content hash are generated here.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("plugin dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn main() {
    // On Windows, KodAct's mark becomes the .exe's icon (resource 1, which
    // the window and the taskbar load too): `assets/kodact.rc`. A build
    // without a resource compiler fails here, rather than ship a blank icon.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/kodact.rc");
        println!("cargo:rerun-if-changed=assets/kodact.ico");
        embed_resource::compile("assets/kodact.rc", embed_resource::NONE)
            .manifest_required()
            .expect("the app icon (assets/kodact.rc) could not be compiled");
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/kodact-plugin");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = vec![];
    walk(&root, &mut files);
    // FNV-1a over every path and byte: a new plugin unpacks to a new folder.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut code = String::from("pub const FILES: &[(&str, &[u8])] = &[\n");
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        let rel = f
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        for b in rel.bytes().chain(std::fs::read(f).unwrap()) {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        writeln!(
            code,
            "    ({rel:?}, include_bytes!({:?})),",
            f.display().to_string()
        )
        .unwrap();
    }
    code.push_str("];\n");
    writeln!(code, "pub const HASH: &str = \"{hash:016x}\";").unwrap();
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("plugin_files.rs");
    std::fs::write(out, code).unwrap();
}
