//! Open Design's outside services, with the user's own keys: pictures made
//! for a design (OpenAI's images, or fal's), and a page published to the
//! web (Vercel). Keys come from the environment ForgeCLI runs in:
//! `OPENAI_API_KEY` (or Forge's `FORGE_OPENAI_API_KEY`), `FAL_KEY`,
//! `VERCEL_TOKEN`. Each service's address can be set too (`OPENAI_BASE_URL`,
//! `FAL_BASE_URL`, `VERCEL_API_URL`), for a proxy or a compatible service.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

fn env(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|n| std::env::var(n).ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Which picture service: OpenAI when its key is set, else fal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageService {
    OpenAi,
    Fal,
}

impl ImageService {
    pub fn label(self) -> &'static str {
        match self {
            ImageService::OpenAi => "OpenAI",
            ImageService::Fal => "fal",
        }
    }
}

pub fn image_service() -> Option<ImageService> {
    if env(&["OPENAI_API_KEY", "FORGE_OPENAI_API_KEY"]).is_some() {
        Some(ImageService::OpenAi)
    } else if env(&["FAL_KEY"]).is_some() {
        Some(ImageService::Fal)
    } else {
        None
    }
}

const NO_KEY: &str = "No picture service is set up: set OPENAI_API_KEY (OpenAI's images) or FAL_KEY (fal) where Forge runs, then start the chat again.";

fn post(url: &str, auth: &str, body: &Value) -> Result<Value, String> {
    match ureq::post(url)
        .timeout(Duration::from_secs(120))
        .set("Authorization", auth)
        .set("Content-Type", "application/json")
        .send_json(body.clone())
    {
        Ok(r) => r.into_json::<Value>().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(code, r)) => {
            let said = r.into_string().unwrap_or_default();
            Err(format!(
                "{url} said {code}: {}",
                said.chars().take(400).collect::<String>()
            ))
        }
        Err(e) => Err(format!("{url}: {e}")),
    }
}

fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let r = ureq::get(url)
        .timeout(Duration::from_secs(120))
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let mut out = vec![];
    std::io::Read::read_to_end(
        &mut std::io::Read::take(r.into_reader(), 40 * 1024 * 1024),
        &mut out,
    )
    .map_err(|e| e.to_string())?;
    Ok(out)
}

/// A picture made from `prompt` (`size` like `1536x1024`), saved as
/// `dir/<name>.png`. Returns the file and the service that made it.
pub fn image(
    prompt: &str,
    size: &str,
    dir: &Path,
    name: &str,
) -> Result<(PathBuf, ImageService), String> {
    let service = image_service().ok_or(NO_KEY)?;
    let bytes = match service {
        ImageService::OpenAi => {
            let key = env(&["OPENAI_API_KEY", "FORGE_OPENAI_API_KEY"]).expect("checked");
            let base =
                env(&["OPENAI_BASE_URL"]).unwrap_or_else(|| "https://api.openai.com/v1".into());
            let model = env(&["FORGE_IMAGE_MODEL"]).unwrap_or_else(|| "gpt-image-1".into());
            let r = post(
                &format!("{}/images/generations", base.trim_end_matches('/')),
                &format!("Bearer {key}"),
                &json!({ "model": model, "prompt": prompt, "size": size, "n": 1 }),
            )?;
            let first = &r["data"][0];
            if let Some(b64) = first["b64_json"].as_str() {
                crate::browser::decode_base64(b64).ok_or("the picture came back unreadable")?
            } else if let Some(url) = first["url"].as_str() {
                get_bytes(url)?
            } else {
                return Err(format!("no picture in the answer: {r}"));
            }
        }
        ImageService::Fal => {
            let key = env(&["FAL_KEY"]).expect("checked");
            let base = env(&["FAL_BASE_URL"]).unwrap_or_else(|| "https://fal.run".into());
            let model = env(&["FORGE_FAL_MODEL"]).unwrap_or_else(|| "fal-ai/flux/schnell".into());
            let (w, h) = size
                .split_once('x')
                .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
                .unwrap_or((1024, 1024));
            let r = post(
                &format!("{}/{model}", base.trim_end_matches('/')),
                &format!("Key {key}"),
                &json!({ "prompt": prompt, "image_size": { "width": w, "height": h }, "num_images": 1 }),
            )?;
            let url = r["images"][0]["url"]
                .as_str()
                .ok_or_else(|| format!("no picture in the answer: {r}"))?;
            if let Some(data) = url.strip_prefix("data:") {
                let b64 = data.split_once(',').map(|x| x.1).unwrap_or_default();
                crate::browser::decode_base64(b64).ok_or("the picture came back unreadable")?
            } else {
                get_bytes(url)?
            }
        }
    };
    // Saved as PNG whatever came (a service may answer in JPEG or WebP).
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("the picture couldn't be read: {e}"))?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let file = crate::downloads::free_path(
        dir,
        &format!("{}.png", if clean.is_empty() { "image" } else { &clean }),
    );
    img.save(&file).map_err(|e| e.to_string())?;
    Ok((file, service))
}

fn save_bytes(dir: &Path, name: &str, ext: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let file = crate::downloads::free_path(
        dir,
        &format!("{}.{ext}", if clean.is_empty() { "media" } else { &clean }),
    );
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    Ok(file)
}

/// A short video made from `prompt` with fal (`FAL_KEY`; the model is
/// `FORGE_FAL_VIDEO_MODEL`, else LTX Video), saved as `dir/<name>.mp4`.
pub fn video(prompt: &str, dir: &Path, name: &str) -> Result<PathBuf, String> {
    let key = env(&["FAL_KEY"])
        .ok_or("Video needs fal: set FAL_KEY where Forge runs, then start the chat again.")?;
    let base = env(&["FAL_BASE_URL"]).unwrap_or_else(|| "https://fal.run".into());
    let model = env(&["FORGE_FAL_VIDEO_MODEL"]).unwrap_or_else(|| "fal-ai/ltx-video".into());
    let r = post(
        &format!("{}/{model}", base.trim_end_matches('/')),
        &format!("Key {key}"),
        &json!({ "prompt": prompt }),
    )?;
    let url = r["video"]["url"]
        .as_str()
        .ok_or_else(|| format!("no video in the answer: {r}"))?;
    save_bytes(dir, name, "mp4", &get_bytes(url)?)
}

/// Speech or narration read from `text` with ElevenLabs
/// (`ELEVENLABS_API_KEY`; the voice is `FORGE_VOICE_ID`, else a stock one),
/// saved as `dir/<name>.mp3`.
pub fn speech(text: &str, dir: &Path, name: &str) -> Result<PathBuf, String> {
    let key = env(&["ELEVENLABS_API_KEY"]).ok_or(
        "Narration needs ElevenLabs: set ELEVENLABS_API_KEY where Forge runs, then start the chat again.",
    )?;
    let base = env(&["ELEVENLABS_BASE_URL"]).unwrap_or_else(|| "https://api.elevenlabs.io".into());
    let voice = env(&["FORGE_VOICE_ID"]).unwrap_or_else(|| "21m00Tcm4TlvDq8ikWAM".into());
    let url = format!("{}/v1/text-to-speech/{voice}", base.trim_end_matches('/'));
    let r = ureq::post(&url)
        .timeout(Duration::from_secs(120))
        .set("xi-api-key", &key)
        .set("Accept", "audio/mpeg")
        .send_json(json!({ "text": text, "model_id": "eleven_multilingual_v2" }));
    let r = match r {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            return Err(format!(
                "{url} said {code}: {}",
                r.into_string()
                    .unwrap_or_default()
                    .chars()
                    .take(400)
                    .collect::<String>()
            ))
        }
        Err(e) => return Err(format!("{url}: {e}")),
    };
    let mut out = vec![];
    std::io::Read::read_to_end(
        &mut std::io::Read::take(r.into_reader(), 40 * 1024 * 1024),
        &mut out,
    )
    .map_err(|e| e.to_string())?;
    save_bytes(dir, name, "mp3", &out)
}

/// Publish `page` on Vercel as one self-contained HTML file (its styles,
/// scripts and pictures inside): the address it is live at.
pub fn deploy(page: &Path, name: &str) -> Result<String, String> {
    let token = env(&["VERCEL_TOKEN"]).ok_or(
        "Publishing needs a Vercel token: create one at vercel.com/account/tokens and set VERCEL_TOKEN where Forge runs, then start the chat again.",
    )?;
    let base = env(&["VERCEL_API_URL"]).unwrap_or_else(|| "https://api.vercel.com".into());
    let html =
        std::fs::read_to_string(page).map_err(|e| format!("Can't read {}: {e}", page.display()))?;
    let one = super::inline::single_file(&html, page, &mut |u| get_bytes(u).ok());
    let project: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(90)
        .collect();
    let project = if project.is_empty() {
        "forge-design".into()
    } else {
        project
    };
    let r = post(
        &format!("{}/v13/deployments", base.trim_end_matches('/')),
        &format!("Bearer {token}"),
        &json!({
            "name": project,
            "files": [{ "file": "index.html", "data": one.html }],
            "projectSettings": { "framework": null },
            "target": "production",
        }),
    )?;
    let url = r["alias"][0]
        .as_str()
        .or(r["url"].as_str())
        .ok_or_else(|| format!("Vercel gave no address: {r}"))?;
    Ok(if url.starts_with("http") {
        url.to_string()
    } else {
        format!("https://{url}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A one-request-at-a-time HTTP server answering `answer(path, body)`;
    /// what it was sent is kept.
    fn mock(
        answer: impl Fn(&str, &str, &str) -> (String, Vec<u8>) + Send + 'static,
    ) -> (String, Arc<Mutex<Vec<(String, String, String)>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", l.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(vec![]));
        let kept = seen.clone();
        let me = addr.clone();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut first = String::new();
                r.read_line(&mut first).unwrap();
                let path = first.split_whitespace().nth(1).unwrap_or("").to_string();
                let (mut len, mut auth) = (0usize, String::new());
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                    let lower = h.to_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                    if lower.starts_with("authorization:") {
                        auth = h["authorization:".len()..].trim().to_string();
                    }
                }
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                let body = String::from_utf8_lossy(&body).to_string();
                kept.lock()
                    .unwrap()
                    .push((path.clone(), auth, body.clone()));
                let (kind, bytes) = answer(&me, &path, &body);
                let mut s = s;
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len());
                let _ = s.write_all(&bytes);
            }
        });
        (addr, seen)
    }

    fn png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(8, 4, image::Rgba([200, 60, 40, 255]));
        let mut out = std::io::Cursor::new(vec![]);
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// The services' keys are read from the environment, so the tests that
    /// set them run one at a time.
    static ENV: Mutex<()> = Mutex::new(());

    fn clear() {
        for k in [
            "OPENAI_API_KEY",
            "FORGE_OPENAI_API_KEY",
            "FAL_KEY",
            "VERCEL_TOKEN",
            "OPENAI_BASE_URL",
            "FAL_BASE_URL",
            "VERCEL_API_URL",
            "ELEVENLABS_API_KEY",
            "ELEVENLABS_BASE_URL",
        ] {
            std::env::remove_var(k);
        }
    }

    #[test]
    fn pictures_come_from_openai_or_fal_with_your_key() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let dir = std::env::temp_dir().join(format!("forge-img-{}", std::process::id()));
        assert!(image("a fox", "1024x1024", &dir, "fox")
            .unwrap_err()
            .contains("OPENAI_API_KEY"));
        // OpenAI: the picture in the answer.
        let b64 = crate::attach::base64(&png());
        let (addr, seen) = mock(move |_, _, _| {
            (
                "application/json".into(),
                json!({ "data": [{ "b64_json": b64 }] })
                    .to_string()
                    .into_bytes(),
            )
        });
        std::env::set_var("OPENAI_API_KEY", "sk-test");
        std::env::set_var("OPENAI_BASE_URL", &addr);
        let (file, by) = image("a fox in snow", "1536x1024", &dir, "hero fox").unwrap();
        assert_eq!(by, ImageService::OpenAi);
        assert_eq!(file.file_name().unwrap(), "hero-fox.png");
        assert_eq!(image::open(&file).unwrap().width(), 8);
        let (path, auth, body) = seen.lock().unwrap()[0].clone();
        assert_eq!(path, "/images/generations");
        assert_eq!(auth, "Bearer sk-test");
        assert!(body.contains("a fox in snow") && body.contains("1536x1024"));
        // fal: a link to the picture, fetched from where it says.
        clear();
        let (addr, seen) = mock({
            let pic = png();
            move |me, path, _| {
                if path.starts_with("/files/") {
                    ("image/png".into(), pic.clone())
                } else {
                    let url = format!("{me}/files/1.png");
                    (
                        "application/json".into(),
                        json!({ "images": [{ "url": url }] })
                            .to_string()
                            .into_bytes(),
                    )
                }
            }
        });
        std::env::set_var("FAL_KEY", "fal-test");
        std::env::set_var("FAL_BASE_URL", &addr);
        let (file, by) = image("a fox", "800x600", &dir, "fox").unwrap();
        assert_eq!(by, ImageService::Fal);
        assert!(file.is_file());
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen[0].1, "Key fal-test");
        assert!(seen[0].0.ends_with("/fal-ai/flux/schnell"));
        assert!(seen[0].2.contains("\"width\":800"));
        assert_eq!(seen[1].0, "/files/1.png");
        clear();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_page_is_published_on_vercel_as_one_file() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let page = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/demo/course-intro.html");
        assert!(deploy(&page, "x").unwrap_err().contains("VERCEL_TOKEN"));
        let (addr, seen) = mock(|_, _, _| {
            ("application/json".into(), json!({ "url": "course-intro-abc.vercel.app", "alias": ["course-intro.vercel.app"] }).to_string().into_bytes())
        });
        std::env::set_var("VERCEL_TOKEN", "vt");
        std::env::set_var("VERCEL_API_URL", &addr);
        let url = deploy(&page, "Course Intro!").unwrap();
        assert_eq!(url, "https://course-intro.vercel.app");
        let (path, auth, body) = seen.lock().unwrap()[0].clone();
        assert_eq!(path, "/v13/deployments");
        assert_eq!(auth, "Bearer vt");
        let sent: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(sent["name"], "course-intro");
        assert_eq!(sent["files"][0]["file"], "index.html");
        assert!(sent["files"][0]["data"]
            .as_str()
            .unwrap()
            .contains("Design Systems 101"));
        clear();
    }

    #[test]
    fn video_from_fal_and_speech_from_elevenlabs() {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let dir = std::env::temp_dir().join(format!("forge-media-{}", std::process::id()));
        assert!(video("waves", &dir, "v").unwrap_err().contains("FAL_KEY"));
        assert!(speech("hi", &dir, "s")
            .unwrap_err()
            .contains("ELEVENLABS_API_KEY"));
        let (addr, seen) = mock(|me, path, _| {
            if path.starts_with("/files/") {
                ("video/mp4".into(), b"\0\0\0\x18ftypmp42".to_vec())
            } else if path.starts_with("/v1/text-to-speech/") {
                ("audio/mpeg".into(), b"ID3fake".to_vec())
            } else {
                let url = format!("{me}/files/v.mp4");
                (
                    "application/json".into(),
                    json!({ "video": { "url": url } }).to_string().into_bytes(),
                )
            }
        });
        std::env::set_var("FAL_KEY", "f");
        std::env::set_var("FAL_BASE_URL", &addr);
        std::env::set_var("ELEVENLABS_API_KEY", "e");
        std::env::set_var("ELEVENLABS_BASE_URL", &addr);
        let v = video("waves at dusk", &dir, "intro").unwrap();
        assert_eq!(v.file_name().unwrap(), "intro.mp4");
        assert!(std::fs::read(&v).unwrap().ends_with(b"ftypmp42"));
        let a = speech("Welcome to the course.", &dir, "welcome").unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), b"ID3fake");
        let seen = seen.lock().unwrap().clone();
        assert!(seen[0].0.ends_with("/fal-ai/ltx-video") && seen[0].2.contains("waves at dusk"));
        assert!(seen.iter().any(|r| r.2.contains("Welcome to the course.")));
        clear();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
