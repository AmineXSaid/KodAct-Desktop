//! Motion settings shared by the animated pieces.

/// Motion turned down: `FORGE_REDUCED_MOTION=1` shows still frames instead.
pub fn reduced_motion() -> bool {
    std::env::var("FORGE_REDUCED_MOTION").is_ok_and(|v| v == "1")
}
