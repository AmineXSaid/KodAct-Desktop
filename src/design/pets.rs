//! Pets: Open Design's animated companions (`assets/community-pets/` in
//! the library, and any you add under `pets/` in Forge's config folder).
//! A pet is a `pet.json` and a sprite sheet: an 8 by 9 atlas of 192 by 208
//! cells, one row per mood (`hatch-pet`'s contract). Forge shows the one you
//! pick beside the chat, in the mood of what Forge is doing.

use std::path::{Path, PathBuf};

/// A cell of the atlas.
pub const CELL: (u32, u32) = (192, 208);

/// The moods, in the atlas's row order, with each frame's time (ms).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mood {
    Idle,
    RunningRight,
    RunningLeft,
    Waving,
    Jumping,
    Failed,
    Waiting,
    Running,
    Review,
}

impl Mood {
    pub const ALL: [Mood; 9] = [
        Mood::Idle,
        Mood::RunningRight,
        Mood::RunningLeft,
        Mood::Waving,
        Mood::Jumping,
        Mood::Failed,
        Mood::Waiting,
        Mood::Running,
        Mood::Review,
    ];

    pub fn row(self) -> u32 {
        Mood::ALL.iter().position(|m| *m == self).unwrap_or(0) as u32
    }

    /// Each frame's time, as the contract gives it (its last frame held a
    /// little longer).
    pub fn timings(self) -> Vec<u64> {
        let even = |n: usize, each: u64, last: u64| {
            let mut v = vec![each; n];
            if let Some(l) = v.last_mut() {
                *l = last;
            }
            v
        };
        match self {
            Mood::Idle => vec![280, 110, 110, 140, 140, 320],
            Mood::RunningRight | Mood::RunningLeft => even(8, 120, 220),
            Mood::Waving => even(4, 140, 280),
            Mood::Jumping => even(5, 140, 280),
            Mood::Failed => even(8, 140, 240),
            Mood::Waiting => even(6, 150, 260),
            Mood::Running => even(6, 120, 220),
            Mood::Review => even(6, 150, 280),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pet {
    pub id: String,
    pub name: String,
    pub description: String,
    pub sheet: PathBuf,
}

/// Where your own pets go: `pets/<id>/pet.json` in Forge's config folder.
pub fn own_dir() -> Option<PathBuf> {
    crate::profile::dir().map(|d| d.join("pets"))
}

fn read_dir(dir: &Path) -> Vec<Pet> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out: Vec<Pet> = rd
        .flatten()
        .filter_map(|e| {
            let folder = e.path();
            let json: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(folder.join("pet.json")).ok()?)
                    .ok()?;
            let id = json["id"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| e.file_name().to_string_lossy().into_owned());
            let sheet = folder.join(
                json["spritesheetPath"]
                    .as_str()
                    .unwrap_or("spritesheet.webp"),
            );
            sheet.is_file().then(|| Pet {
                name: json["displayName"].as_str().unwrap_or(&id).to_string(),
                description: json["description"].as_str().unwrap_or("").to_string(),
                id,
                sheet,
            })
        })
        .collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// Every pet: yours, then the library's.
pub fn all() -> Vec<Pet> {
    let mut out = own_dir().map(|d| read_dir(&d)).unwrap_or_default();
    if let Some(root) = super::root() {
        for p in read_dir(&root.join("assets/community-pets")) {
            if !out.iter().any(|o| o.id == p.id) {
                out.push(p);
            }
        }
    }
    out
}

fn choice_file() -> Option<PathBuf> {
    crate::profile::dir().map(|d| d.join("pet.txt"))
}

/// The pet you picked, if any.
pub fn chosen() -> Option<String> {
    std::fs::read_to_string(choice_file()?)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn choose(id: Option<&str>) -> std::io::Result<()> {
    let Some(f) = choice_file() else {
        return Ok(());
    };
    if let Some(dir) = f.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(f, id.unwrap_or(""))
}

/// A mood's frames, cut from the sheet and scaled to `width` wide (RGBA).
pub type Frames = Vec<image::RgbaImage>;

/// Every mood's frames from a sheet, `width` wide: the cells that aren't
/// empty, in order.
pub fn frames(sheet: &Path, width: u32) -> Result<Vec<(Mood, Frames)>, String> {
    let img = image::open(sheet)
        .map_err(|e| format!("{}: {e}", sheet.display()))?
        .to_rgba8();
    let (cw, ch) = CELL;
    if img.width() < cw || img.height() < ch {
        return Err(format!("{} is smaller than one cell", sheet.display()));
    }
    let height = width * ch / cw;
    let mut out = vec![];
    for mood in Mood::ALL {
        let row = mood.row();
        let mut list = vec![];
        for col in 0..mood.timings().len() as u32 {
            let (x, y) = (col * cw, row * ch);
            if x + cw > img.width() || y + ch > img.height() {
                break;
            }
            let cell = image::imageops::crop_imm(&img, x, y, cw, ch).to_image();
            // An empty cell (all clear) ends the row.
            if cell.pixels().all(|p| p.0[3] == 0) {
                break;
            }
            list.push(image::imageops::resize(
                &cell,
                width,
                height,
                image::imageops::FilterType::Triangle,
            ));
        }
        if !list.is_empty() {
            out.push((mood, list));
        }
    }
    if out.is_empty() {
        return Err(format!("{} has no frames", sheet.display()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_pets_load_and_cut_into_moods() {
        let Some(root) = crate::design::root() else {
            return;
        };
        let pets = read_dir(&root.join("assets/community-pets"));
        assert!(pets.len() >= 8, "{pets:?}");
        let tux = pets.iter().find(|p| p.id == "tux").expect("tux");
        assert_eq!(tux.name, "Tux");
        let moods = frames(&tux.sheet, 96).unwrap();
        let idle = &moods.iter().find(|(m, _)| *m == Mood::Idle).unwrap().1;
        assert!(!idle.is_empty() && idle.len() <= 6);
        assert_eq!((idle[0].width(), idle[0].height()), (96, 104));
        // Every mood's timings match its frames' most.
        for (m, f) in &moods {
            assert!(f.len() <= m.timings().len(), "{m:?}");
        }
    }

    #[test]
    fn rows_and_timings_follow_the_contract() {
        assert_eq!(Mood::Idle.row(), 0);
        assert_eq!(Mood::Review.row(), 8);
        assert_eq!(
            Mood::RunningRight.timings(),
            vec![120, 120, 120, 120, 120, 120, 120, 220]
        );
        assert_eq!(Mood::Waving.timings().len(), 4);
    }
}
