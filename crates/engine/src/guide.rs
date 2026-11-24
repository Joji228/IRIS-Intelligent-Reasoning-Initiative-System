//! What agents know about I.R.I.S. itself: a built-in guide to the app
//! (`guide.md`, compiled in) and the latest release notes from GitHub, behind
//! one always-on tool, `about_iris`. The prompt tells agents to use it for
//! questions about the app instead of guessing.

use std::time::Duration;

use serde_json::Value;

use crate::{api::AppState, tools::ToolOutcome};

/// The guide, `## ` sections with a short overview first.
pub const GUIDE: &str = include_str!("guide.md");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Starts the release-notes part of an answer (it counts as outside content, see `outside::taints`).
pub const RELEASES_HEADING: &str = "Latest releases on GitHub:";
/// Release notes are read from GitHub at most this many at a time.
const RELEASES: usize = 5;
/// Each release's notes are cut to this many characters.
const NOTES_CHARS: usize = 1_500;

/// The guide's sections: (heading, body).
fn sections() -> Vec<(&'static str, &'static str)> {
    GUIDE
        .split("\n## ")
        .skip(1)
        .map(|s| s.split_once('\n').map_or((s.trim(), ""), |(h, b)| (h.trim(), b.trim())))
        .collect()
}

fn contents() -> String {
    sections().iter().map(|(h, _)| *h).collect::<Vec<_>>().join(", ")
}

/// The sections that best match `topic` (by its words in headings and text), best first.
fn find(topic: &str) -> Vec<(&'static str, &'static str)> {
    let words: Vec<String> = topic
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !["the", "and", "how", "what", "does", "can", "iris", "with", "for"].contains(w))
        .map(str::to_owned)
        .collect();
    if words.is_empty() {
        return vec![];
    }
    let mut scored: Vec<(usize, (&str, &str))> = sections()
        .into_iter()
        .map(|(h, b)| {
            let (hl, bl) = (h.to_lowercase(), b.to_lowercase());
            // A word's stem also counts ("compacting" finds "compaction").
            let stem = |w: &str| w.chars().take(6).collect::<String>();
            let score = words.iter().map(|w| 4 * usize::from(hl.contains(&stem(w))) + bl.matches(&stem(w)).count().min(5)).sum();
            (score, (h, b))
        })
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(3).map(|(_, s)| s).collect()
}

fn wants_releases(topic: &str) -> bool {
    let t = topic.to_lowercase();
    ["release", "version", "what's new", "whats new", "changelog", "update", "new in"].iter().any(|k| t.contains(k))
}

/// The latest releases from GitHub, newest first.
async fn releases(http: &reqwest::Client) -> Result<String, String> {
    let base = std::env::var("IRIS_UPDATE_API").unwrap_or_else(|_| "https://api.github.com".into());
    let resp = http
        .get(format!("{base}/repos/{}/releases?per_page={RELEASES}", crate::update::repo()))
        .header("User-Agent", concat!("iris-engine/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("couldn't reach GitHub: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub answered {}", resp.status().as_u16()));
    }
    let list: Vec<Value> = resp.json().await.map_err(|e| e.to_string())?;
    let out: Vec<String> = list
        .iter()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .map(|r| {
            let notes = r["body"].as_str().unwrap_or("").trim();
            let notes = if notes.chars().count() > NOTES_CHARS {
                format!("{}…", notes.chars().take(NOTES_CHARS).collect::<String>())
            } else {
                notes.to_owned()
            };
            format!(
                "### {} ({})\n{}",
                r["tag_name"].as_str().unwrap_or("?"),
                r["published_at"].as_str().unwrap_or("").get(..10).unwrap_or(""),
                notes
            )
        })
        .collect();
    Ok(out.join("\n\n"))
}

/// Answers `about_iris`: the guide's matching sections, or the release notes.
pub async fn run(st: &AppState, call: &iris_providers::ToolCall) -> ToolOutcome {
    let args: Value = serde_json::from_str(&call.arguments).unwrap_or_default();
    let topic = args["topic"].as_str().unwrap_or("").trim();
    let head = format!("I.R.I.S. version {VERSION} (running on this server).");
    if topic.is_empty() {
        let overview = sections().first().map_or("", |(_, b)| *b);
        return ToolOutcome {
            ok: true,
            output: format!("{head}\n\n{overview}\n\nGuide sections (ask about any): {}", contents()),
            summary: "Read the I.R.I.S. guide".into(),
        };
    }
    if wants_releases(topic) {
        return match releases(&st.http).await {
            Ok(notes) => ToolOutcome {
                ok: true,
                output: format!("{head}\n\n{RELEASES_HEADING}\n\n{notes}\n\nUpdating: {}", update_section()),
                summary: "Read the I.R.I.S. release notes".into(),
            },
            Err(e) => ToolOutcome {
                ok: true,
                output: format!("{head}\n\nThe release notes couldn't be read from GitHub ({e}).\n\nUpdating: {}", update_section()),
                summary: "Couldn't read the release notes".into(),
            },
        };
    }
    let found = find(topic);
    let output = if found.is_empty() {
        format!("{head}\n\nNothing in the guide matches “{topic}”. Its sections: {}.", contents())
    } else {
        let parts: Vec<String> = found.iter().map(|(h, b)| format!("## {h}\n{b}")).collect();
        format!("{head}\n\n{}", parts.join("\n\n"))
    };
    ToolOutcome { ok: true, output, summary: format!("Looked up “{topic}” in the I.R.I.S. guide") }
}

fn update_section() -> &'static str {
    sections().into_iter().find(|(h, _)| *h == "Updates").map_or("", |(_, b)| b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guide_has_its_sections() {
        let names: Vec<&str> = sections().iter().map(|(h, _)| *h).collect();
        for want in ["Overview", "Thinking effort", "Compaction", "Approvals and auto mode", "Updates", "Routines"] {
            assert!(names.contains(&want), "missing {want}: {names:?}");
        }
        assert!(sections().iter().all(|(_, b)| !b.is_empty()));
    }

    #[test]
    fn topics_find_the_right_section() {
        assert_eq!(find("how do I compact a chat?")[0].0, "Compaction");
        assert_eq!(find("thinking effort slider")[0].0, "Thinking effort");
        assert_eq!(find("full auto approvals")[0].0, "Approvals and auto mode");
        assert_eq!(find("pair my phone")[0].0, "Devices, remote access and the phone app");
        assert_eq!(find("voice call in greek")[0].0, "Voice and calls");
        assert!(find("the and how").is_empty());
        assert!(wants_releases("what's new in the latest version?"));
        assert!(!wants_releases("how do I compact"));
        // Release notes from GitHub count as outside content; guide answers don't.
        assert!(crate::outside::taints("about_iris", true, &format!("x\n\n{RELEASES_HEADING}\n\n### v1")));
        assert!(!crate::outside::taints("about_iris", true, "## Compaction\n…"));
    }
}
