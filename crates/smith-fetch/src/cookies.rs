//! Per-site cookie store (Netscape cookies.txt files) for signed-in downloads.
//!
//! Cookies come from the app's own sign-in window or an imported cookies.txt,
//! are kept only on this computer under the app's data folder, one file per
//! site, and are only handed to yt-dlp/ffmpeg for URLs on that site.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// One cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    /// As stored, e.g. `.example.com` (leading dot = include subdomains).
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    /// Unix seconds; `None` = session cookie.
    pub expires: Option<i64>,
}

/// Second-level labels under which registrable domains have three labels.
const MULTI_PART_SUFFIXES: [&str; 12] = [
    "co", "com", "net", "org", "gov", "ac", "edu", "ne", "or", "go", "gob", "nic",
];

/// The site key a host belongs to: its registrable domain (`m.youtube.com`
/// → `youtube.com`, `news.bbc.co.uk` → `bbc.co.uk`). IP literals and
/// single-label hosts are returned as-is.
#[must_use]
pub fn site_key(host: &str) -> String {
    let h = host
        .trim_start_matches('.')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if h.parse::<std::net::IpAddr>().is_ok() {
        return h;
    }
    let labels: Vec<&str> = h.split('.').collect();
    if labels.len() <= 2 {
        return h;
    }
    let n = labels.len();
    let take = if labels[n - 1].len() == 2 && MULTI_PART_SUFFIXES.contains(&labels[n - 2]) {
        3
    } else {
        2
    };
    labels[n.saturating_sub(take)..].join(".")
}

/// Serialise cookies as a Netscape cookies.txt (what yt-dlp `--cookies` reads).
#[must_use]
pub fn to_netscape(cookies: &[Cookie]) -> String {
    let mut s = String::from(
        "# Netscape HTTP Cookie File\n# Saved by a Smith app. Keep this file private.\n\n",
    );
    for c in cookies {
        if c.name.contains(['\t', '\n', '\r'])
            || c.value.contains(['\t', '\n', '\r'])
            || c.domain.contains(['\t', '\n'])
        {
            continue;
        }
        let domain = if c.http_only {
            format!("#HttpOnly_{}", c.domain)
        } else {
            c.domain.clone()
        };
        let sub = if c.domain.starts_with('.') {
            "TRUE"
        } else {
            "FALSE"
        };
        let path = if c.path.is_empty() { "/" } else { &c.path };
        let secure = if c.secure { "TRUE" } else { "FALSE" };
        let exp = c.expires.unwrap_or(0).max(0);
        s.push_str(&format!(
            "{domain}\t{sub}\t{path}\t{secure}\t{exp}\t{}\t{}\n",
            c.name, c.value
        ));
    }
    s
}

/// Parse a Netscape cookies.txt. Malformed lines are skipped.
#[must_use]
pub fn parse_netscape(text: &str) -> Vec<Cookie> {
    let mut v = Vec::new();
    for line in text.lines() {
        let (http_only, line) = match line.strip_prefix("#HttpOnly_") {
            Some(rest) => (true, rest),
            None => (false, line),
        };
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
        if f.len() < 7 || f[0].is_empty() || f[5].is_empty() {
            continue;
        }
        let exp: i64 = f[4].parse().unwrap_or(0);
        v.push(Cookie {
            domain: f[0].to_string(),
            path: f[2].to_string(),
            secure: f[3].eq_ignore_ascii_case("TRUE"),
            expires: (exp > 0).then_some(exp),
            name: f[5].to_string(),
            value: f[6..].join("\t"),
            http_only,
        });
    }
    v
}

/// `name=value; ...` for a request to `host` (for direct HTTP/ffmpeg fetches).
#[must_use]
pub fn header_for_host(cookies: &[Cookie], host: &str) -> Option<String> {
    let host = host.to_ascii_lowercase();
    let parts: Vec<String> = cookies
        .iter()
        .filter(|c| {
            let d = c.domain.trim_start_matches('.').to_ascii_lowercase();
            host == d || host.ends_with(&format!(".{d}"))
        })
        .map(|c| format!("{}={}", c.name, c.value))
        .collect();
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// A saved site, for the Settings list.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedSite {
    pub site: String,
    pub cookies: usize,
}

/// The on-disk store: `<dir>/<site>.txt`.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn file(&self, site: &str) -> Option<PathBuf> {
        let ok = !site.is_empty()
            && site
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
            && !site.starts_with('.');
        ok.then(|| self.dir.join(format!("{site}.txt")))
    }

    /// Save cookies, grouped by site. Returns the sites written.
    ///
    /// # Errors
    /// If the folder or a file can't be written.
    pub fn save(&self, cookies: &[Cookie]) -> std::io::Result<Vec<String>> {
        std::fs::create_dir_all(&self.dir)?;
        let mut by_site: std::collections::BTreeMap<String, Vec<Cookie>> = Default::default();
        for c in cookies {
            by_site
                .entry(site_key(&c.domain))
                .or_default()
                .push(c.clone());
        }
        let mut written = Vec::new();
        for (site, list) in by_site {
            let Some(path) = self.file(&site) else {
                continue;
            };
            // Merge with what's there: newer values win by (domain, path, name).
            let mut merged = std::fs::read_to_string(&path)
                .map(|t| parse_netscape(&t))
                .unwrap_or_default();
            for c in list {
                merged.retain(|m| !(m.domain == c.domain && m.path == c.path && m.name == c.name));
                merged.push(c);
            }
            write_private(&path, &to_netscape(&merged))?;
            written.push(site);
        }
        Ok(written)
    }

    /// Import a cookies.txt the user picked (split per site).
    ///
    /// # Errors
    /// If the file can't be read, has no cookies, or the store can't be written.
    pub fn import(&self, path: &Path) -> Result<Vec<String>, String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("Couldn't read that file: {e}"))?;
        let cookies = parse_netscape(&text);
        if cookies.is_empty() {
            return Err("That file has no cookies in Netscape cookies.txt format.".to_string());
        }
        self.save(&cookies)
            .map_err(|e| format!("Couldn't save the cookies: {e}"))
    }

    /// The cookies file for a URL's site, if one is saved.
    #[must_use]
    pub fn file_for_url(&self, url: &str) -> Option<PathBuf> {
        let host = url::Url::parse(url).ok()?.host_str()?.to_string();
        self.file(&site_key(&host)).filter(|p| p.is_file())
    }

    /// Cookies for a URL's site.
    #[must_use]
    pub fn cookies_for_url(&self, url: &str) -> Vec<Cookie> {
        self.file_for_url(url)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| parse_netscape(&t))
            .unwrap_or_default()
    }

    /// Saved sites.
    #[must_use]
    pub fn list(&self) -> Vec<SavedSite> {
        let mut v: Vec<SavedSite> = std::fs::read_dir(&self.dir)
            .map(|r| {
                r.filter_map(Result::ok)
                    .filter_map(|e| {
                        let p = e.path();
                        let site = p.file_stem()?.to_str()?.to_string();
                        (p.extension()? == "txt").then(|| SavedSite {
                            cookies: std::fs::read_to_string(&p)
                                .map(|t| parse_netscape(&t).len())
                                .unwrap_or(0),
                            site,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| a.site.cmp(&b.site));
        v
    }

    /// Forget one site (or all, with `None`).
    pub fn forget(&self, site: Option<&str>) {
        match site {
            Some(s) => {
                if let Some(p) = self.file(s) {
                    let _ = std::fs::remove_file(p);
                }
            }
            None => {
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }
    }
}

/// Write a file readable only by the current user where the OS allows it.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(name: &str, domain: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: "v".into(),
            domain: domain.into(),
            path: "/".into(),
            secure: true,
            http_only: false,
            expires: Some(2_000_000_000),
        }
    }

    #[test]
    fn site_keys() {
        assert_eq!(site_key("m.youtube.com"), "youtube.com");
        assert_eq!(site_key(".youtube.com"), "youtube.com");
        assert_eq!(site_key("news.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(site_key("example.com"), "example.com");
        assert_eq!(site_key("127.0.0.1"), "127.0.0.1");
        assert_eq!(site_key("a.b.example.com.au"), "example.com.au");
    }

    #[test]
    fn netscape_round_trip() {
        let mut list = vec![c("SID", ".youtube.com"), c("x", "vimeo.com")];
        list[1].http_only = true;
        list[1].expires = None;
        let text = to_netscape(&list);
        assert!(text.starts_with("# Netscape HTTP Cookie File"));
        let back = parse_netscape(&text);
        assert_eq!(back, list);
        assert!(parse_netscape("garbage\nmore\tgarbage").is_empty());
    }

    #[test]
    fn header_matches_domain_and_subdomains() {
        let list = vec![c("a", ".example.com"), c("b", "other.com")];
        assert_eq!(
            header_for_host(&list, "cdn.example.com").as_deref(),
            Some("a=v")
        );
        assert_eq!(header_for_host(&list, "notexample.com"), None);
    }

    #[test]
    fn store_saves_per_site_merges_and_forgets() {
        let dir = std::env::temp_dir().join(format!("smith-cookies-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = Store::new(dir.clone());
        let sites = s
            .save(&[
                c("SID", ".youtube.com"),
                c("PREF", "m.youtube.com"),
                c("x", ".vimeo.com"),
            ])
            .unwrap();
        assert_eq!(sites, ["vimeo.com", "youtube.com"]);
        let mut newer = c("SID", ".youtube.com");
        newer.value = "new".into();
        s.save(&[newer]).unwrap();
        let yt = s.cookies_for_url("https://www.youtube.com/watch?v=1");
        assert_eq!(yt.len(), 2);
        assert!(yt.iter().any(|c| c.name == "SID" && c.value == "new"));
        assert!(s.file_for_url("https://example.org/").is_none());
        assert_eq!(s.list().len(), 2);
        assert!(s.file("../evil").is_none());
        s.forget(Some("vimeo.com"));
        assert_eq!(s.list().len(), 1);
        s.forget(None);
        assert!(s.list().is_empty());
    }
}
