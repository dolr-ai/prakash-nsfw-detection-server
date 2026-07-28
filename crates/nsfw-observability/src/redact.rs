//! Redacts URLs to a safe, loggable subset: never query strings (which carry signed-URL
//! credentials), userinfo, or fragments. Mirrors Python's `_safe_url_context`.

use url::Url;

/// The only URL representation allowed at INFO+ / Sentry. Raw URLs stay at `debug!`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SafeUrl {
    pub scheme: String,
    pub host: String,
    pub path: String,
    pub port: Option<u16>,
}

/// Path is truncated to 160 chars to bound log size (Python parity). Unparseable input
/// yields an all-empty `SafeUrl` rather than leaking the raw string.
pub fn safe_url(raw: &str) -> SafeUrl {
    match Url::parse(raw) {
        Ok(url) => {
            let mut path = url.path().to_string();
            if path.len() > 160 {
                path.truncate(160);
            }
            SafeUrl {
                scheme: url.scheme().to_string(),
                host: url.host_str().unwrap_or_default().to_string(),
                path,
                port: url.port(),
            }
        }
        Err(_) => SafeUrl::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn drops_query_userinfo_and_fragment() {
        let s = safe_url("https://user:pw@cdn.example.com:8443/a/b.jpg?sig=SECRET#frag");
        assert_eq!(s.scheme, "https");
        assert_eq!(s.host, "cdn.example.com");
        assert_eq!(s.path, "/a/b.jpg");
        assert_eq!(s.port, Some(8443));
        // Nothing secret survives.
        let rendered = format!("{s:?}");
        assert!(!rendered.contains("SECRET"));
        assert!(!rendered.contains("pw"));
    }

    #[test]
    fn truncates_long_path_to_160_chars() {
        let long = "a".repeat(500);
        let s = safe_url(&format!("https://h.example/{long}"));
        assert_eq!(s.path.len(), 160);
    }

    #[rstest]
    #[case("not a url")]
    #[case("")]
    #[case("//no-scheme")] // relative URL without base -> parse error
    fn unparseable_input_yields_empty_safe_url(#[case] input: &str) {
        assert_eq!(safe_url(input), SafeUrl::default());
    }

    #[test]
    fn missing_port_is_none() {
        let s = safe_url("https://h.example/x");
        assert_eq!(s.port, None);
    }
}
