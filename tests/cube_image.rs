//! The Forge cube picture, checked pixel by pixel: the input (the original
//! file as given) against the outputs (the renderings the app shows at each
//! size and screen density).
//!
//! The original must be kept byte for byte. Each rendering must be the
//! original made smaller and nothing else: the same colours (including the
//! blue glow), the same coverage, a transparent background, and close to a
//! plain area-average reduction of the original, pixel for pixel.

use image::{ImageReader, Rgba32FImage, RgbaImage};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/images");
/// Every size rendered: 32, 48 and 96px at 1x, 1.25x, 1.5x, 1.75x, 2x, 2.5x, 3x.
const SIZES: [u32; 16] = [
    32, 40, 48, 56, 60, 64, 72, 80, 84, 96, 120, 144, 168, 192, 240, 288,
];

fn load(name: &str) -> RgbaImage {
    ImageReader::open(format!("{DIR}/{name}"))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .decode()
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .into_rgba8()
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    }
}

/// The original as the renderings frame it: its transparent margin cut
/// away (only fully transparent pixels), centred in a square.
fn framed(original: &RgbaImage) -> RgbaImage {
    let (w, h) = original.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (x, y, p) in original.enumerate_pixels() {
        if p[3] > 0 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
        }
    }
    let (cw, ch) = (x1 - x0, y1 - y0);
    let side = cw.max(ch);
    let mut out = RgbaImage::new(side, side);
    image::imageops::overlay(
        &mut out,
        &image::imageops::crop_imm(original, x0, y0, cw, ch).to_image(),
        ((side - cw) / 2) as i64,
        ((side - ch) / 2) as i64,
    );
    out
}

/// The original in linear light with premultiplied alpha, row by row.
fn linear(src: &RgbaImage) -> Vec<[f64; 4]> {
    src.pixels()
        .map(|p| {
            let a = p[3] as f64 / 255.;
            [
                to_linear(p[0] as f32 / 255.) as f64 * a,
                to_linear(p[1] as f32 / 255.) as f64 * a,
                to_linear(p[2] as f32 / 255.) as f64 * a,
                a,
            ]
        })
        .collect()
}

/// The reference: each output pixel the area-weighted average of the
/// original pixels it covers, in linear light with premultiplied alpha.
fn reference(src: &RgbaImage, lin: &[[f64; 4]], n: u32) -> Rgba32FImage {
    let side = src.width() as f64;
    let scale = side / n as f64;
    let mut out = Rgba32FImage::new(n, n);
    for oy in 0..n {
        for ox in 0..n {
            let (sx0, sx1) = (ox as f64 * scale, (ox + 1) as f64 * scale);
            let (sy0, sy1) = (oy as f64 * scale, (oy + 1) as f64 * scale);
            let mut acc = [0f64; 4];
            let mut total = 0f64;
            for y in sy0.floor() as u32..(sy1.ceil() as u32).min(src.height()) {
                let wy = (sy1.min(y as f64 + 1.) - sy0.max(y as f64)).max(0.);
                for x in sx0.floor() as u32..(sx1.ceil() as u32).min(src.width()) {
                    let wx = (sx1.min(x as f64 + 1.) - sx0.max(x as f64)).max(0.);
                    let p = lin[(y * src.width() + x) as usize];
                    let wgt = wx * wy;
                    for c in 0..4 {
                        acc[c] += p[c] * wgt;
                    }
                    total += wgt;
                }
            }
            let a = acc[3] / total;
            let px = out.get_pixel_mut(ox, oy);
            for c in 0..3 {
                px[c] = if a > 1e-6 {
                    to_srgb((acc[c] / total / a) as f32)
                } else {
                    0.
                };
            }
            px[3] = a as f32;
        }
    }
    out
}

/// The mean colour (sRGB, 0..255) and coverage (0..1). Colours are averaged
/// as light (linear, alpha-weighted): reducing an image keeps the light it
/// gives off, not the average of its sRGB numbers.
fn stats(px: impl Iterator<Item = [f32; 4]>) -> ([f64; 3], f64) {
    let (mut sum, mut alpha, mut count) = ([0f64; 3], 0f64, 0f64);
    for p in px {
        for c in 0..3 {
            sum[c] += to_linear(p[c]) as f64 * p[3] as f64;
        }
        alpha += p[3] as f64;
        count += 1.;
    }
    (
        sum.map(|s| to_srgb((s / alpha) as f32) as f64 * 255.),
        alpha / count,
    )
}

/// FNV-1a, to pin the original file's bytes.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

#[test]
fn the_original_is_kept_byte_for_byte() {
    let bytes =
        std::fs::read(format!("{DIR}/cube-original.webp")).expect("the original is in the repo");
    assert_eq!(bytes.len(), 209_940, "the original's size changed");
    assert_eq!(
        fnv1a(&bytes),
        0xa5a8_cadc_d0fc_f3ef,
        "the original's bytes changed"
    );
    let img = load("cube-original.webp");
    assert_eq!(img.dimensions(), (1254, 1254));
    assert_eq!(
        img.get_pixel(0, 0)[3],
        0,
        "the original's background is transparent"
    );
}

#[test]
fn every_rendering_is_the_original_made_smaller() {
    let source = framed(&load("cube-original.webp"));
    let src_f = || {
        source.pixels().map(|p| {
            [
                p[0] as f32 / 255.,
                p[1] as f32 / 255.,
                p[2] as f32 / 255.,
                p[3] as f32 / 255.,
            ]
        })
    };
    let (src_mean, src_cover) = stats(src_f());
    let lin = linear(&source);
    let mut report = vec![];
    for n in SIZES {
        let out = load(&format!("cube-{n}.png"));
        assert_eq!(out.dimensions(), (n, n), "cube-{n}.png is {n}x{n}");
        let refimg = reference(&source, &lin, n);

        // Transparent around the cube.
        for (x, y) in [(0, 0), (n - 1, 0), (0, n - 1), (n - 1, n - 1)] {
            assert_eq!(
                out.get_pixel(x, y)[3],
                0,
                "cube-{n}.png corner ({x},{y}) is transparent"
            );
        }

        // Pixel for pixel against the reference, premultiplied (what you see over any background).
        let mut se = 0f64;
        let mut worst = 0f64;
        for (o, r) in out.pixels().zip(refimg.pixels()) {
            let oa = o[3] as f64 / 255.;
            for c in 0..3 {
                let d = (o[c] as f64 / 255. * oa - r[c] as f64 * r[3] as f64) * 255.;
                se += d * d;
                worst = worst.max(d.abs());
            }
            let da = (oa - r[3] as f64) * 255.;
            se += da * da;
        }
        let mse = se / (n as f64 * n as f64 * 4.);
        let psnr = 10. * (255f64 * 255. / mse.max(1e-9)).log10();

        // The same colours and coverage as the original.
        let (mean, cover) = stats(out.pixels().map(|p| {
            [
                p[0] as f32 / 255.,
                p[1] as f32 / 255.,
                p[2] as f32 / 255.,
                p[3] as f32 / 255.,
            ]
        }));
        let colour_shift = (0..3)
            .map(|c| (mean[c] - src_mean[c]).abs())
            .fold(0., f64::max);

        // The glow: the brightest blues are still as blue.
        let glow = |it: &mut dyn Iterator<Item = (f64, f64)>| {
            let mut blues: Vec<f64> = it.filter(|(a, _)| *a > 0.9).map(|(_, b)| b).collect();
            blues.sort_by(f64::total_cmp);
            blues[blues.len() * 99 / 100]
        };
        let glow_out = glow(&mut out.pixels().map(|p| (p[3] as f64 / 255., p[2] as f64)));
        let glow_ref = glow(&mut refimg.pixels().map(|p| (p[3] as f64, p[2] as f64 * 255.)));

        report.push(format!("{n:>3}px  PSNR {psnr:5.1} dB  colour shift {colour_shift:.2}/255  coverage {:.4} vs {:.4}  glow {glow_out:.0} vs {glow_ref:.0}", cover, src_cover));
        // The renderings use Lanczos, which keeps edges crisper than a plain
        // average; that sharpening is all the difference allowed.
        assert!(
            psnr >= 28.,
            "cube-{n}.png differs from the original reduced: PSNR {psnr:.1} dB"
        );
        assert!(
            colour_shift <= 2.,
            "cube-{n}.png colours shifted by {colour_shift:.2}/255"
        );
        assert!(
            (cover - src_cover).abs() <= 0.01,
            "cube-{n}.png coverage {cover:.4} vs {src_cover:.4}"
        );
        assert!(
            (glow_out - glow_ref).abs() <= 8.,
            "cube-{n}.png glow {glow_out:.0} vs {glow_ref:.0}"
        );
    }
    println!("{}", report.join("\n"));
}

#[test]
fn every_rendering_is_bundled() {
    let assets = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/assets.rs"))
        .expect("assets.rs");
    for n in SIZES {
        assert!(
            assets.contains(&format!("\"cube-{n}\"")),
            "cube-{n}.png is not in the asset table"
        );
    }
}
