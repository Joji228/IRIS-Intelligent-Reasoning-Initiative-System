//! `web_search` and `web_fetch`.

use std::time::Duration;

use reqwest::header::{CONTENT_TYPE, USER_AGENT};
use scraper::{ElementRef, Html, Selector};
use serde_json::{Value, json};

use super::{ToolContext, ToolOutcome, short_url, truncate};

/// Pages and search results are fetched like a normal browser would.
const BROWSER_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
const MAX_PAGE_BYTES: usize = 3 * 1024 * 1024;
const MAX_PAGE_CHARS: usize = 12_000;

/// Which search backend to use. DeepSeek (its Search API, paid per search,
/// with the DeepSeek provider's key) when chosen; Tavily (made for agents)
/// with a Tavily key; otherwise DuckDuckGo's HTML page (free, but can
/// rate-limit).
#[derive(Debug, Clone, Default)]
pub struct SearchSettings {
    pub tavily_key: Option<String>,
    /// Set when DeepSeek search is chosen: also lets `web_fetch` fall back
    /// to DeepSeek's Fetch API for pages it can't read itself.
    pub deepseek_key: Option<String>,
}

/// DeepSeek's Search and Fetch APIs (same key as its chat models).
const DEEPSEEK_SEARCH: &str = "https://api.deepseek.com/v1/tools/search";
const DEEPSEEK_FETCH: &str = "https://api.deepseek.com/v1/tools/fetch";

#[derive(Debug, PartialEq)]
pub struct Hit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

pub async fn search(ctx: &ToolContext, query: &str, max: usize) -> ToolOutcome {
    let summary = format!("Searched the web for “{query}”");
    let results = match (&ctx.search.deepseek_key, &ctx.search.tavily_key) {
        (Some(key), _) => deepseek_search(ctx, key, query, max).await,
        (None, Some(key)) => tavily(ctx, key, query, max).await,
        (None, None) => duckduckgo(ctx, query, max).await,
    };
    match results {
        Ok(hits) if hits.is_empty() => ToolOutcome::ok(format!("{summary} · no results"), "No results found."),
        Ok(hits) => {
            let text = hits
                .iter()
                .enumerate()
                .map(|(i, h)| format!("{}. {}\n   {}\n   {}", i + 1, h.title, h.url, h.snippet))
                .collect::<Vec<_>>()
                .join("\n\n");
            ToolOutcome::ok(format!("{summary} · {} results", hits.len()), text)
        }
        Err(e) => ToolOutcome::err(summary, e),
    }
}

async fn tavily(ctx: &ToolContext, key: &str, query: &str, max: usize) -> Result<Vec<Hit>, String> {
    let resp = ctx
        .http
        .post("https://api.tavily.com/search")
        .bearer_auth(key)
        .json(&json!({ "query": query, "max_results": max, "search_depth": "basic" }))
        .timeout(Duration::from_secs(25))
        .send()
        .await
        .map_err(|e| format!("Tavily request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Tavily returned {status}: {}", truncate(&body, 300)));
    }
    let v: Value = resp.json().await.map_err(|e| format!("Tavily sent an unreadable answer: {e}"))?;
    let hits = v["results"]
        .as_array()
        .map(|results| {
            results
                .iter()
                .map(|r| Hit {
                    title: r["title"].as_str().unwrap_or_default().to_owned(),
                    url: r["url"].as_str().unwrap_or_default().to_owned(),
                    snippet: truncate(r["content"].as_str().unwrap_or_default(), 500),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(hits)
}

/// What a DeepSeek Search/Fetch error means for the user.
fn deepseek_error(status: reqwest::StatusCode, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| truncate(body, 200));
    match status.as_u16() {
        401 => "DeepSeek refused the API key (Settings → AI providers)".into(),
        402 => "the DeepSeek balance is empty: top it up to keep searching".into(),
        429 => "DeepSeek's search limit was hit; wait a moment and try again".into(),
        _ => format!("DeepSeek returned {status}: {detail}"),
    }
}

async fn deepseek_search(ctx: &ToolContext, key: &str, query: &str, max: usize) -> Result<Vec<Hit>, String> {
    // The API takes 1–128 characters.
    let query: String = query.trim().chars().take(128).collect();
    let resp = ctx
        .http
        .post(DEEPSEEK_SEARCH)
        .bearer_auth(key)
        .json(&json!({ "query": query, "count": max.clamp(1, 50) }))
        .timeout(Duration::from_secs(25))
        .send()
        .await
        .map_err(|e| format!("DeepSeek search failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(deepseek_error(status, &body));
    }
    let v: Value = resp.json().await.map_err(|e| format!("DeepSeek sent an unreadable answer: {e}"))?;
    Ok(deepseek_hits(&v))
}

/// Results with the long summary (made for models) and the date when known.
pub fn deepseek_hits(v: &Value) -> Vec<Hit> {
    let s = |r: &Value, k: &str| r[k].as_str().unwrap_or_default().trim().to_owned();
    v["results"]
        .as_array()
        .map(|results| {
            results
                .iter()
                .map(|r| {
                    let text = Some(s(r, "summary")).filter(|t| !t.is_empty()).unwrap_or_else(|| s(r, "snippet"));
                    let date = s(r, "date");
                    let date = date.get(..10).filter(|_| !date.is_empty());
                    Hit {
                        title: s(r, "title"),
                        url: s(r, "url"),
                        snippet: match date {
                            Some(d) => format!("({d}) {}", truncate(&text, 800)),
                            None => truncate(&text, 800),
                        },
                    }
                })
                .filter(|h| !h.url.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// A page through DeepSeek's Fetch API, as Markdown (for pages the engine
/// can't read itself: blocked, script-only, or not plain HTML).
async fn deepseek_fetch(ctx: &ToolContext, key: &str, url: &str) -> Result<(String, String), String> {
    let resp = ctx
        .http
        .post(DEEPSEEK_FETCH)
        .bearer_auth(key)
        .json(&json!({ "url": url }))
        .timeout(Duration::from_secs(40))
        .send()
        .await
        .map_err(|e| format!("DeepSeek fetch failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(if status.as_u16() == 404 { "DeepSeek couldn't read it either".into() } else { deepseek_error(status, &body) });
    }
    let v: Value = resp.json().await.map_err(|e| format!("DeepSeek sent an unreadable answer: {e}"))?;
    let content = v["content"].as_str().unwrap_or_default().trim().to_owned();
    if content.is_empty() {
        return Err("DeepSeek couldn't read it either".into());
    }
    Ok((v["title"].as_str().unwrap_or_default().trim().to_owned(), content))
}

/// The page read through DeepSeek instead, or why the engine's own read
/// failed (and DeepSeek's too, when it was tried).
async fn fetch_via_deepseek(ctx: &ToolContext, url: &url::Url, summary: String, own_error: String) -> ToolOutcome {
    let Some(key) = ctx.search.deepseek_key.as_deref() else { return ToolOutcome::err(summary, own_error) };
    match deepseek_fetch(ctx, key, url.as_str()).await {
        Ok((title, content)) => {
            let mut output = format!("URL: {url}\n");
            if !title.is_empty() {
                output.push_str(&format!("Title: {title}\n"));
            }
            output.push('\n');
            output.push_str(&truncate(&content, MAX_PAGE_CHARS));
            let label = if title.is_empty() { summary } else { format!("{summary} · “{}”", truncate(&title, 60)) };
            ToolOutcome::ok(format!("{label} · via DeepSeek"), output)
        }
        Err(e) => ToolOutcome::err(summary, format!("{own_error}; {e}")),
    }
}

async fn duckduckgo(ctx: &ToolContext, query: &str, max: usize) -> Result<Vec<Hit>, String> {
    let resp = ctx
        .http
        .post("https://html.duckduckgo.com/html/")
        .header(USER_AGENT, BROWSER_UA)
        .form(&[("q", query)])
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("DuckDuckGo request failed: {e}"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("DuckDuckGo returned {status}"));
    }
    let hits = parse_duckduckgo(&body, max);
    if hits.is_empty() && (body.contains("anomaly") || body.contains("captcha")) {
        return Err("DuckDuckGo is rate-limiting searches right now. For reliable search, add a free \
                    Tavily key in Settings → Web search."
            .into());
    }
    Ok(hits)
}

/// Pulls results out of DuckDuckGo's plain-HTML results page.
pub fn parse_duckduckgo(html: &str, max: usize) -> Vec<Hit> {
    let doc = Html::parse_document(html);
    let result = Selector::parse("div.result").expect("selector");
    let link = Selector::parse("a.result__a").expect("selector");
    let snippet = Selector::parse(".result__snippet").expect("selector");
    doc.select(&result)
        .filter(|r| !r.value().classes().any(|c| c == "result--ad"))
        .filter_map(|r| {
            let a = r.select(&link).next()?;
            let url = ddg_target(a.value().attr("href")?)?;
            Some(Hit { title: text_of(a), url, snippet: r.select(&snippet).next().map(text_of).unwrap_or_default() })
        })
        .take(max)
        .collect()
}

/// Result links point at a DuckDuckGo redirect (`//duckduckgo.com/l/?uddg=<url>`);
/// return the real destination.
fn ddg_target(href: &str) -> Option<String> {
    let full = if let Some(rest) = href.strip_prefix("//") {
        format!("https://{rest}")
    } else if href.starts_with('/') {
        format!("https://duckduckgo.com{href}")
    } else {
        href.to_owned()
    };
    let url = url::Url::parse(&full).ok()?;
    if let Some((_, target)) = url.query_pairs().find(|(k, _)| k == "uddg") {
        return Some(target.into_owned());
    }
    if url.domain().is_some_and(|d| d.ends_with("duckduckgo.com")) {
        return None;
    }
    Some(full)
}

fn text_of(el: ElementRef) -> String {
    el.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub async fn fetch(ctx: &ToolContext, raw_url: &str) -> ToolOutcome {
    let url = match url::Url::parse(raw_url.trim()) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => u,
        _ => return ToolOutcome::err("Couldn't read the page", format!("`{raw_url}` is not a valid http(s) URL")),
    };
    let summary = format!("Read {}", short_url(url.as_str()));
    if is_own_machine(&url) {
        return ToolOutcome::err(summary, "fetching this computer's own addresses (localhost) is not allowed");
    }

    let resp = match ctx.http.get(url.clone()).header(USER_AGENT, BROWSER_UA).timeout(Duration::from_secs(25)).send().await {
        Ok(r) => r,
        Err(e) => return fetch_via_deepseek(ctx, &url, summary, format!("request failed: {e}")).await,
    };
    let status = resp.status();
    if !status.is_success() {
        // Often a bot wall (403, 429…): DeepSeek's reader may get through.
        let label = format!("{summary} · HTTP {}", status.as_u16());
        return fetch_via_deepseek(ctx, &url, label, format!("the server answered HTTP {status}")).await;
    }
    let final_url = resp.url().to_string();
    let content_type =
        resp.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_ascii_lowercase();

    let mut resp = resp;
    let mut body: Vec<u8> = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                body.extend_from_slice(&chunk);
                if body.len() >= MAX_PAGE_BYTES {
                    body.truncate(MAX_PAGE_BYTES);
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => return ToolOutcome::err(summary, format!("download failed: {e}")),
        }
    }

    let (title, text) = if content_type.contains("html") || content_type.is_empty() {
        let (title, text) = html_to_text(&String::from_utf8_lossy(&body));
        // Almost no text (a page built by scripts) or a bot wall: DeepSeek's
        // reader may get the real page.
        if ctx.search.deepseek_key.is_some() && (text.trim().chars().count() < 200 || looks_blocked(&text)) {
            let read = fetch_via_deepseek(ctx, &url, summary.clone(), "the page had no readable content".into()).await;
            if read.ok {
                return read;
            }
        }
        (title, text)
    } else if content_type.starts_with("text/") || content_type.contains("json") || content_type.contains("xml") {
        (String::new(), String::from_utf8_lossy(&body).into_owned())
    } else {
        return ToolOutcome::ok(
            format!("{summary} · {content_type}"),
            format!("URL: {final_url}\nThis is not a text page ({content_type}, {} bytes); its content is not shown.", body.len()),
        );
    };

    let mut output = format!("URL: {final_url}\n");
    if !title.is_empty() {
        output.push_str(&format!("Title: {title}\n"));
    }
    output.push('\n');
    output.push_str(&truncate(text.trim(), MAX_PAGE_CHARS));
    let label = if title.is_empty() { summary } else { format!("{summary} · “{}”", truncate(&title, 60)) };
    ToolOutcome::ok(label, output)
}

/// A short page that is really a "prove you're human" or "blocked" wall.
pub fn looks_blocked(text: &str) -> bool {
    const WALLS: &[&str] = &[
        "enable javascript",
        "verify you are human",
        "are you a robot",
        "are you human",
        "access denied",
        "just a moment",
        "you've been blocked",
        "you have been blocked",
        "blocked by network security",
        "unusual traffic",
        "captcha",
    ];
    let t = text.trim();
    t.chars().count() < 2000 && {
        let lower = t.to_lowercase();
        WALLS.iter().any(|w| lower.contains(w))
    }
}

/// Title and readable text of an HTML page.
pub fn html_to_text(html: &str) -> (String, String) {
    let doc = Html::parse_document(html);
    let title = doc
        .select(&Selector::parse("title").expect("selector"))
        .next()
        .map(text_of)
        .unwrap_or_default();
    let text = html2text::from_read(html.as_bytes(), 100).unwrap_or_default();
    (title, text)
}

/// Agents must not poke at the engine's own API or other local services.
pub fn is_own_machine(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost") || d.ends_with(".localhost"),
        Some(url::Host::Ipv4(ip)) => is_own_ip(ip.into()),
        Some(url::Host::Ipv6(ip)) => is_own_ip(ip.into()),
        None => true,
    }
}

fn is_own_ip(ip: std::net::IpAddr) -> bool {
    let ip = match ip {
        std::net::IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, std::net::IpAddr::V4),
        v4 => v4,
    };
    ip.is_loopback() || ip.is_unspecified()
}

/// Redirects are checked like the first address: a public page can't send a
/// request on to this computer (`302 → http://127.0.0.1:9333/json/list`).
/// Requests that start on this computer (a local AI provider) may stay there.
pub fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let started_local = attempt.previous().first().is_some_and(is_own_machine);
        if attempt.previous().len() >= 8 {
            attempt.error("too many redirects")
        } else if !started_local && is_own_machine(attempt.url()) {
            attempt.error("redirect to this computer refused")
        } else {
            attempt.follow()
        }
    })
}

/// DNS for the engine's HTTP client: names other than `localhost` that point
/// at this computer (`127.0.0.1.nip.io`, a rebinding attacker's domain) don't
/// resolve, so they can't be used to get around [`is_own_machine`].
pub struct GuardedDns;

impl reqwest::dns::Resolve for GuardedDns {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            if host.eq_ignore_ascii_case("localhost") {
                return Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs);
            }
            let outside: Vec<_> = addrs.into_iter().filter(|a| !is_own_ip(a.ip())).collect();
            if outside.is_empty() {
                return Err(format!("{host} points at this computer; that isn't allowed").into());
            }
            Ok(Box::new(outside.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_results_use_the_long_summary_and_date() {
        let v = json!({ "results": [
            { "title": "Rust", "url": "https://www.rust-lang.org/", "snippet": "short", "summary": "A long summary.", "date": "2026-10-17T08:00:00Z" },
            { "title": "No date", "url": "https://example.com/", "snippet": "only a snippet", "summary": "", "date": null },
            { "title": "No url", "url": "", "snippet": "x" }
        ]});
        let hits = deepseek_hits(&v);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].snippet, "(2026-10-17) A long summary.");
        assert_eq!(hits[1].snippet, "only a snippet");
        assert!(deepseek_hits(&json!({ "results": [] })).is_empty());
    }

    #[test]
    fn bot_walls_are_recognised() {
        assert!(looks_blocked("You've been blocked by network security. To continue, log in."));
        assert!(looks_blocked("Just a moment... Checking your browser"));
        assert!(!looks_blocked("Rust is a language empowering everyone to build reliable software."));
        assert!(!looks_blocked(&"A long article about CAPTCHA history. ".repeat(80)));
    }

    #[test]
    fn deepseek_errors_read_plainly() {
        let s = reqwest::StatusCode::from_u16(402).unwrap();
        assert!(deepseek_error(s, "").contains("balance"));
        let s = reqwest::StatusCode::from_u16(400).unwrap();
        let body = r#"{"error":{"message":"'count' must be between 1 and 50","type":"invalid_request_error"}}"#;
        assert!(deepseek_error(s, body).contains("'count' must be between 1 and 50"));
    }

    #[test]
    fn parses_duckduckgo_results() {
        let html = r#"
            <div class="result results_links result--ad"><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fads.example%2F">Ad</a></div>
            <div class="result results_links">
              <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F&amp;rut=x">Rust   Programming Language</a>
              <a class="result__snippet">A language empowering everyone.</a>
            </div>
            <div class="result"><a class="result__a" href="https://example.org/page">Example</a></div>"#;
        let hits = parse_duckduckgo(html, 10);
        assert_eq!(
            hits,
            vec![
                Hit {
                    title: "Rust Programming Language".into(),
                    url: "https://www.rust-lang.org/".into(),
                    snippet: "A language empowering everyone.".into()
                },
                Hit { title: "Example".into(), url: "https://example.org/page".into(), snippet: String::new() },
            ]
        );
        assert_eq!(parse_duckduckgo(html, 1).len(), 1);
    }

    #[test]
    fn html_text_and_title() {
        let (title, text) = html_to_text(
            "<html><head><title>Hello Page</title><script>var x = 1;</script></head>\
             <body><h1>Heading</h1><p>Some <b>bold</b> text.</p></body></html>",
        );
        assert_eq!(title, "Hello Page");
        assert!(text.contains("Heading"));
        assert!(text.contains("bold"));
        assert!(!text.contains("var x"), "scripts are dropped: {text}");
    }

    #[test]
    fn blocks_local_addresses() {
        let own = |u: &str| is_own_machine(&url::Url::parse(u).unwrap());
        assert!(own("http://localhost:7700/v1/agents"));
        assert!(own("http://127.0.0.1:7700/"));
        assert!(own("http://[::1]/"));
        assert!(own("http://[::ffff:127.0.0.1]/"));
        assert!(own("http://0.0.0.0:7700/"));
        assert!(own("http://iris.localhost/"));
        assert!(!own("https://example.com/"));
        assert!(!own("http://192.168.1.1/"), "LAN devices (e.g. the router) are allowed");
    }

    #[tokio::test]
    async fn names_and_redirects_cannot_reach_this_computer() {
        use reqwest::dns::Resolve;
        // `localhost` itself still resolves (local AI providers use it).
        let name: reqwest::dns::Name = "localhost".parse().unwrap();
        assert!(GuardedDns.resolve(name).await.is_ok());

        // A request that starts on this computer (a local provider) may
        // follow redirects that stay here.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let reply = if String::from_utf8_lossy(&buf[..n]).starts_with("GET /done") {
                    "HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok".to_owned()
                } else {
                    format!("HTTP/1.1 302 Found\r\nlocation: http://127.0.0.1:{port}/done\r\ncontent-length: 0\r\n\r\n")
                };
                let _ = sock.write_all(reply.as_bytes()).await;
            }
        });
        let client = reqwest::Client::builder().redirect(redirect_policy()).build().unwrap();
        let resp = client.get(format!("http://127.0.0.1:{port}/")).send().await.unwrap();
        assert!(resp.url().path() == "/done" && resp.status().is_success());
    }
}
