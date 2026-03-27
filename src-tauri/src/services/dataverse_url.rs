use crate::domain::errors::{bad_request, AppResult};

pub fn normalize_server_url(value: &str) -> AppResult<String> {
    let trimmed = value.trim().trim_end_matches('/');
    let parsed = url::Url::parse(trimmed).map_err(|err| bad_request(err.to_string()))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(bad_request("server URL must use http or https"));
    }

    let mut segments = parsed
        .path_segments()
        .map(|it| it.collect::<Vec<_>>())
        .unwrap_or_default();
    if segments.len() >= 2 && segments[0].eq_ignore_ascii_case("dataverse") {
        segments.clear();
        let host = parsed
            .host_str()
            .ok_or_else(|| bad_request("server URL must include a host"))?;
        let origin = if let Some(port) = parsed.port() {
            format!("{}://{}:{}", parsed.scheme(), host, port)
        } else {
            format!("{}://{}", parsed.scheme(), host)
        };
        return Ok(origin);
    }

    Ok(trimmed.to_string())
}

pub fn server_url_api_candidates(server_url: &str) -> Vec<String> {
    let mut candidates = vec![server_url.trim_end_matches('/').to_string()];
    let parsed = match url::Url::parse(server_url) {
        Ok(value) => value,
        Err(_) => return candidates,
    };

    if parsed.path() == "/" {
        return candidates;
    }

    let host = match parsed.host_str() {
        Some(value) => value,
        None => return candidates,
    };
    let origin = if let Some(port) = parsed.port() {
        format!("{}://{}:{}", parsed.scheme(), host, port)
    } else {
        format!("{}://{}", parsed.scheme(), host)
    };
    if !candidates.iter().any(|it| it == &origin) {
        candidates.push(origin);
    }

    candidates
}

pub fn extract_dataverse_alias(server_url: &str) -> Option<String> {
    let parsed = url::Url::parse(server_url).ok()?;
    let mut segments = parsed.path_segments()?;
    let first = segments.next()?;
    let second = segments.next()?;
    if first.eq_ignore_ascii_case("dataverse") {
        let alias = second.trim();
        if !alias.is_empty() {
            return Some(alias.to_string());
        }
    }
    None
}

pub fn resolve_url(server_url: &str, candidate: &str) -> String {
    if candidate.starts_with("http://") || candidate.starts_with("https://") {
        candidate.to_string()
    } else {
        format!("{}{}", server_url.trim_end_matches('/'), candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_dataverse_subtree_to_origin() {
        let normalized =
            normalize_server_url("https://example.org/dataverse/demo/").expect("must normalize");
        assert_eq!(normalized, "https://example.org");
    }

    #[test]
    fn keeps_origin_and_adds_fallback_candidate() {
        let candidates = server_url_api_candidates("https://example.org/dataverse/demo");
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0], "https://example.org/dataverse/demo");
        assert_eq!(candidates[1], "https://example.org");
    }

    #[test]
    fn extracts_alias_from_dataverse_path() {
        let alias = extract_dataverse_alias("https://example.org/dataverse/root");
        assert_eq!(alias.as_deref(), Some("root"));
    }

    #[test]
    fn resolves_relative_upload_url_against_server_base() {
        let resolved = resolve_url("https://example.org", "/api/upload/part");
        assert_eq!(resolved, "https://example.org/api/upload/part");
    }
}
