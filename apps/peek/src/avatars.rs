//! Independent avatar worker: never holds up Git requests or UI declarations.
use plico_core::ImageAsset;
use plico_dev::services::Wake;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    io::{Cursor, Read},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

pub(crate) fn normalize(email: &str) -> String {
    email.trim().to_lowercase()
}
fn url(email: &str, hash: &str) -> String {
    if let Some(user) = email.strip_suffix("@users.noreply.github.com") {
        if let Some((id, _)) = user
            .split_once('+')
            .filter(|(id, _)| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        {
            return format!("https://avatars.githubusercontent.com/u/{id}?s=64");
        }
        if !user.is_empty() && user.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return format!("https://github.com/{user}.png?size=64");
        }
    }
    format!("https://www.gravatar.com/avatar/{hash}?s=64&d=404")
}
fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache")))
        .map(|p| p.join("peek-plico/avatars-v1"))
}
pub(crate) struct Avatars {
    jobs: mpsc::SyncSender<String>,
    requested: HashSet<String>,
    pub results: mpsc::Receiver<(String, Option<ImageAsset>)>,
}
impl Avatars {
    pub fn new(wake: Arc<Mutex<Option<Wake>>>) -> Self {
        let (jobs, requests) = mpsc::sync_channel::<String>(128);
        let (results_tx, results) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .user_agent("peek-plico/0.1.0")
                .build()
            else {
                return;
            };
            let dir = cache_dir();
            if let Some(dir) = &dir {
                let _ = std::fs::create_dir_all(dir);
            }
            while let Ok(email) = requests.recv() {
                let hash = format!("{:x}", Sha256::digest(email.as_bytes()));
                let path = dir.as_ref().map(|dir| dir.join(&hash));
                let bytes = path.as_ref().and_then(|p| {
                    let f = std::fs::File::open(p).ok()?;
                    if f.metadata().ok()?.len() > 262144 {
                        return None;
                    }
                    let mut b = Vec::new();
                    f.take(262145).read_to_end(&mut b).ok()?;
                    Some(b)
                });
                let asset = bytes.and_then(|b| circular(&b)).or_else(|| {
                    let primary = url(&email, &hash);
                    let loaded = download(&client, &primary).or_else(|error| {
                        eprintln!("Avatar source failed: {error}");
                        if email.ends_with("@users.noreply.github.com") {
                            return Err(error);
                        }
                        // Public-email lookup only: never guess an account from the author name.
                        let url = github_avatar(&client, &email)?;
                        download(&client, &url)
                    });
                    let (bytes, asset) = match loaded {
                        Ok(loaded) => loaded,
                        Err(error) => {
                            eprintln!("Avatar unavailable; using initials: {error}");
                            return None;
                        }
                    };
                    if let Some(path) = &path {
                        let _ = std::fs::write(path, &bytes);
                    }
                    Some(asset)
                });
                if results_tx.send((email, asset)).is_err() {
                    break;
                }
                let callback = wake.lock().unwrap().clone();
                if let Some(wake) = callback {
                    wake();
                }
            }
        });
        Self {
            jobs,
            requested: HashSet::new(),
            results,
        }
    }
    pub fn request(&mut self, email: &str) {
        let email = normalize(email);
        if email.is_empty() || self.requested.contains(&email) || self.requested.len() >= 512 {
            return;
        }
        if self.jobs.try_send(email.clone()).is_ok() {
            self.requested.insert(email);
        }
    }
}
const MAX_BYTES: u64 = 262144;

fn download(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<(Vec<u8>, ImageAsset), String> {
    let response = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("avatar exceeds 256 KiB".into());
    }
    let asset = circular(&bytes).ok_or("invalid or oversized avatar image")?;
    Ok((bytes, asset))
}

#[derive(serde::Deserialize)]
struct GithubSearch {
    total_count: usize,
    incomplete_results: bool,
    items: Vec<GithubUser>,
}
#[derive(serde::Deserialize)]
struct GithubUser {
    id: u64,
}
impl GithubSearch {
    fn avatar_url(&self) -> Option<String> {
        if self.incomplete_results || self.total_count != 1 || self.items.len() != 1 {
            return None;
        }
        let id = self.items[0].id;
        (id != 0).then(|| format!("https://avatars.githubusercontent.com/u/{id}?s=64"))
    }
}
fn github_avatar(client: &reqwest::blocking::Client, email: &str) -> Result<String, String> {
    let response = client
        .get("https://api.github.com/search/users")
        .query(&[("q", format!("{email} in:email"))])
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("GitHub avatar lookup exceeds 256 KiB".into());
    }
    let result: GithubSearch = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    result
        .avatar_url()
        .ok_or_else(|| "no unique public GitHub email match".into())
}

fn circular(bytes: &[u8]) -> Option<ImageAsset> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let (w, h) = reader.into_dimensions().ok()?;
    if w == 0 || h == 0 || w > 512 || h > 512 {
        return None;
    }
    let mut image = image::load_from_memory(bytes)
        .ok()?
        .resize_to_fill(64, 64, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let distance = ((x as f32 + 0.5 - 32.).powi(2) + (y as f32 + 0.5 - 32.).powi(2)).sqrt();
        pixel.0[3] = (pixel.0[3] as f32 * (32. - distance).clamp(0., 1.)) as u8;
    }
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(ImageAsset::from_bytes(png.get_ref()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "network verification: set PEEK_AVATAR_EMAIL"]
    fn avatar_worker_network_smoke() {
        let email = std::env::var("PEEK_AVATAR_EMAIL").expect("set PEEK_AVATAR_EMAIL");
        let mut worker = Avatars::new(Arc::default());
        worker.request(&email);
        let (actual, asset) = worker
            .results
            .recv_timeout(Duration::from_secs(20))
            .unwrap();
        assert_eq!(actual, normalize(&email));
        assert!(asset.is_some(), "avatar lookup failed");
    }
    #[test]
    fn github_fallback_requires_a_unique_complete_match() {
        for (count, incomplete, ids, expected) in [
            (1, false, vec![16167062], true),
            (0, false, vec![], false),
            (2, false, vec![1, 2], false),
            (1, true, vec![1], false),
            (1, false, vec![], false),
            (1, false, vec![0], false),
        ] {
            let result = GithubSearch {
                total_count: count,
                incomplete_results: incomplete,
                items: ids.into_iter().map(|id| GithubUser { id }).collect(),
            };
            assert_eq!(result.avatar_url().is_some(), expected);
        }
    }
    #[test]
    fn address_mapping_and_image_validation() {
        assert_eq!(normalize(" A@B.COM "), "a@b.com");
        assert_eq!(
            url("123+alice@users.noreply.github.com", "hash"),
            "https://avatars.githubusercontent.com/u/123?s=64"
        );
        assert_eq!(
            url("alice@users.noreply.github.com", "hash"),
            "https://github.com/alice.png?size=64"
        );
        assert!(url("a@example.com", "hash").ends_with("/hash?s=64&d=404"));
        assert!(circular(b"not an image").is_none());
        let input = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]));
        let mut bytes = Cursor::new(Vec::new());
        input.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let result = circular(bytes.get_ref()).unwrap();
        let decoded = image::load_from_memory(result.data()).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (64, 64));
        assert_eq!(decoded.get_pixel(0, 0).0[3], 0);
        assert_eq!(decoded.get_pixel(32, 32).0[3], 255);
    }
}
