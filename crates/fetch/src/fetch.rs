use scraper::{Html, Selector};

/// Fetch a URL and return the raw body text.
pub async fn fetch_url(
    http: &reqwest::Client,
    url: &str,
    timeout: Option<std::time::Duration>,
) -> Result<(String, u16, Option<String>), FetchError> {
    let start = std::time::Instant::now();

    let mut request = http.get(url);
    if let Some(timeout) = timeout {
        request = request.timeout(timeout);
    }

    let response = request
        .send()
        .await
        .map_err(|e| FetchError::Http(e.to_string()))?;

    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let body = response
        .text()
        .await
        .map_err(|e| FetchError::Http(e.to_string()))?;

    let latency = start.elapsed().as_secs_f64();
    let domain = extract_domain(url);
    metrics::histogram!("fetch.request.latency", "domain" => domain).record(latency);

    Ok((body, status, content_type))
}

/// Extract readable text from HTML by removing script, style, nav, footer, header elements.
pub fn extract_html_content(html: &str) -> String {
    let document = Html::parse_document(html);

    // Selectors for elements to remove.
    let remove_selectors = [
        "script", "style", "nav", "footer", "header", "noscript", "svg",
    ];

    let mut skip_ids = std::collections::HashSet::new();

    for sel_str in &remove_selectors {
        if let Ok(selector) = Selector::parse(sel_str) {
            for element in document.select(&selector) {
                skip_ids.insert(element.id());
            }
        }
    }

    // Walk the document tree and collect text from elements not in skip set.
    let mut text_parts = Vec::new();

    for node in document.tree.nodes() {
        // Skip if this node or any ancestor is in the skip set.
        let mut should_skip = false;
        let mut check_id = Some(node.id());
        while let Some(id) = check_id {
            if skip_ids.contains(&id) {
                should_skip = true;
                break;
            }
            check_id = document
                .tree
                .get(id)
                .and_then(|n| n.parent())
                .map(|p| p.id());
        }

        if should_skip {
            continue;
        }

        if let Some(text) = node.value().as_text() {
            let trimmed = text.text.trim();
            if !trimmed.is_empty() {
                text_parts.push(trimmed.to_string());
            }
        }
    }

    // Collapse multiple whitespace/newlines.
    let joined = text_parts.join(" ");
    collapse_whitespace(&joined)
}

fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_was_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(c);
            prev_was_space = false;
        }
    }
    result.trim().to_string()
}

fn extract_domain(url: &str) -> String {
    url.split("//")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("unknown")
        .to_string()
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Rate limited for domain: {0}")]
    #[allow(dead_code)]
    RateLimited(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_html_basic() {
        let html = r#"
            <html>
            <head><title>Test</title></head>
            <body>
                <nav>Navigation here</nav>
                <main>
                    <h1>Article Title</h1>
                    <p>This is the main content of the article.</p>
                    <p>Second paragraph with more information.</p>
                </main>
                <footer>Footer content</footer>
                <script>alert('bad');</script>
            </body>
            </html>
        "#;

        let text = extract_html_content(html);
        assert!(text.contains("Article Title"));
        assert!(text.contains("main content"));
        assert!(!text.contains("Navigation here"));
        assert!(!text.contains("Footer content"));
        assert!(!text.contains("alert"));
    }

    #[test]
    fn test_collapse_whitespace() {
        assert_eq!(collapse_whitespace("hello   world"), "hello world");
        assert_eq!(collapse_whitespace("  hello\n\n  world  "), "hello world");
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://example.com/path"), "example.com");
        assert_eq!(extract_domain("http://www.test.org/a/b"), "www.test.org");
    }
}
