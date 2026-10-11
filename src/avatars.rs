//! Your avatar: one of twelve faceless, cute head-and-shoulders portraits
//! (`assets/avatars`, drawn by `make.py`), six women and six men. A new
//! user gets one at random, keeps it between launches (`profile.rs`), and
//! can pick another on the first-run screen or from the command palette.
//!
//! The SVGs are served through the asset source as `avatars/<name>-<px>.svg`,
//! sized to the pixels they are drawn at, so each is rasterised once at
//! exactly that size and never scaled.

macro_rules! avatars {
    ($($name:literal),* $(,)?) => {
        const FILES: &[(&str, &str)] = &[$(($name, include_str!(concat!("../assets/avatars/", $name, ".svg"))),)*];
    };
}

avatars!("bow", "spacebuns", "flowerbun", "ponytail", "headband", "hijab", "quiff", "beanie", "cap", "beard", "curls", "phones");

pub const WOMEN: [&str; 6] = ["bow", "spacebuns", "flowerbun", "ponytail", "headband", "hijab"];
pub const MEN: [&str; 6] = ["quiff", "beanie", "cap", "beard", "curls", "phones"];

/// Every avatar, women first.
pub fn all() -> impl Iterator<Item = &'static str> {
    WOMEN.into_iter().chain(MEN)
}

pub fn is_known(name: &str) -> bool {
    all().any(|a| a == name)
}

/// One at random, from `seed`.
pub fn random(seed: u64) -> &'static str {
    let mut x = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    let n = WOMEN.len() + MEN.len();
    all().nth((x % n as u64) as usize).unwrap_or(WOMEN[0])
}

/// The asset path for `name` drawn `px` pixels square.
pub fn path(name: &str, px: u32) -> String {
    format!("avatars/{name}-{px}.svg")
}

/// The SVG for an asset path, sized to its pixels.
pub fn load(path: &str) -> Option<Vec<u8>> {
    let (name, px) = path.strip_prefix("avatars/")?.strip_suffix(".svg")?.rsplit_once('-')?;
    let px: u32 = px.parse().ok().filter(|p| (8..=1024).contains(p))?;
    let svg = FILES.iter().find(|(n, _)| *n == name)?.1;
    Some(svg.replacen(r#"width="64" height="64""#, &format!(r#"width="{px}" height="{px}""#), 1).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_avatar_is_bundled_and_sizes() {
        assert_eq!(all().count(), FILES.len());
        for name in all() {
            let svg = String::from_utf8(load(&path(name, 96)).unwrap()).unwrap();
            assert!(svg.contains(r#"width="96" height="96""#), "{name}");
            assert!(svg.contains(&format!("bg-{name}")), "{name} has its own ids");
        }
        assert!(load("avatars/bow-0.svg").is_none());
        assert!(load("avatars/nobody-32.svg").is_none());
        assert!(load("icons/x.svg").is_none());
    }

    #[test]
    fn random_picks_from_the_whole_set() {
        let picked: std::collections::HashSet<&str> = (0..200).map(random).collect();
        assert_eq!(picked.len(), WOMEN.len() + MEN.len());
        assert!(is_known(random(42)) && !is_known("robot"));
    }
}
