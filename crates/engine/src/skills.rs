//! Skills: how-tos agents write for themselves after working out a tricky
//! task, and load again when a similar one comes up. Shared by the whole
//! team. One folder per skill under `skills/` in the data dir, each with a
//! `SKILL.md` in the agentskills.io format (front matter with `name` and
//! `description`, then the instructions), so skills can be copied in from
//! elsewhere too.
//!
//! Only names and descriptions go into the system prompt (they're short);
//! `use_skill` loads the instructions when they're needed.

use std::{
    fs,
    path::{Path, PathBuf},
};

use iris_providers::ToolCall;
use serde::Serialize;
use serde_json::Value;
use tracing::warn;

use crate::tools::ToolOutcome;

const MAX_DESCRIPTION: usize = 200;
const MAX_BODY: usize = 8_000;
/// At most this many skills are listed in the system prompt.
const MAX_LISTED: usize = 40;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
}

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("skills")
}

/// "Order food on Wolt!" → "order-food-on-wolt".
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(48).collect::<String>().trim_end_matches('-').to_owned()
}

/// Reads a SKILL.md: front matter (`name:`, `description:`) and the body.
fn parse(text: &str, fallback_name: &str) -> Skill {
    let text = text.trim_start_matches('\u{feff}');
    let (mut name, mut description, mut body) = (fallback_name.to_owned(), String::new(), text.trim().to_owned());
    if let Some(rest) = text.trim_start().strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                let Some((key, value)) = line.split_once(':') else { continue };
                let value = value.trim().trim_matches(['"', '\'']).to_owned();
                match key.trim() {
                    "name" if !value.is_empty() => name = value,
                    "description" => description = value,
                    _ => {}
                }
            }
            body = rest[end + 4..].trim().to_owned();
        }
    }
    Skill { name, description, body }
}

fn render(skill: &Skill) -> String {
    let one_line = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").replace('"', "'");
    format!(
        "---\nname: {}\ndescription: \"{}\"\n---\n\n{}\n",
        skill.name,
        one_line(&skill.description),
        skill.body.trim()
    )
}

/// Every skill, sorted by name.
pub fn list(data_dir: &Path) -> Vec<Skill> {
    let Ok(entries) = fs::read_dir(dir(data_dir)) else { return Vec::new() };
    let mut skills: Vec<Skill> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let folder = e.file_name().to_string_lossy().into_owned();
            let text = fs::read_to_string(e.path().join("SKILL.md")).ok()?;
            Some(parse(&text, &folder))
        })
        .collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

pub fn get(data_dir: &Path, name: &str) -> Option<Skill> {
    let slug = slug(name);
    if slug.is_empty() {
        return None;
    }
    let text = fs::read_to_string(dir(data_dir).join(&slug).join("SKILL.md")).ok()?;
    Some(parse(&text, &slug))
}

/// Creates or replaces a skill.
pub fn save(data_dir: &Path, name: &str, description: &str, body: &str) -> Result<Skill, String> {
    let slug = slug(name);
    if slug.is_empty() {
        return Err("give the skill a short name, like \"deploy-website\"".into());
    }
    let description = description.trim();
    if description.is_empty() {
        return Err("describe in one line when to use this skill".into());
    }
    if description.chars().count() > MAX_DESCRIPTION {
        return Err(format!("keep the description under {MAX_DESCRIPTION} characters"));
    }
    if body.trim().is_empty() {
        return Err("the skill needs instructions".into());
    }
    if body.chars().count() > MAX_BODY {
        return Err(format!("keep the instructions under {MAX_BODY} characters: steps, not a whole document"));
    }
    let skill = Skill { name: slug.clone(), description: description.to_owned(), body: body.trim().to_owned() };
    let folder = dir(data_dir).join(&slug);
    fs::create_dir_all(&folder).map_err(|e| format!("can't create the skill folder: {e}"))?;
    fs::write(folder.join("SKILL.md"), render(&skill)).map_err(|e| format!("can't save the skill: {e}"))?;
    Ok(skill)
}

pub fn delete(data_dir: &Path, name: &str) -> bool {
    let slug = slug(name);
    !slug.is_empty() && fs::remove_dir_all(dir(data_dir).join(slug)).is_ok()
}

/// The skill list for the system prompt (empty when there are none).
pub fn prompt_section(data_dir: &Path) -> String {
    let skills = list(data_dir);
    let mut out = String::from(
        "\n\nSkills: when you work out a multi-step task that's likely to come up again (a procedure, a site's quirks, \
         a working command sequence), save it with save_skill: short, concrete steps. One-off facts go in memory instead.",
    );
    if skills.is_empty() {
        return out;
    }
    out.push_str(" Saved skills (load one with use_skill before doing such a task):");
    for s in skills.iter().take(MAX_LISTED) {
        out.push_str(&format!("\n- {}: {}", s.name, s.description));
    }
    if skills.len() > MAX_LISTED {
        out.push_str(&format!("\n… and {} more (use_skill with a name to load one)", skills.len() - MAX_LISTED));
    }
    out
}

/// `use_skill` and `save_skill`.
pub fn run(data_dir: &Path, agent_name: &str, call: &ToolCall) -> ToolOutcome {
    let args: Value = serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments })
        .unwrap_or(Value::Null);
    let s = |k: &str| args[k].as_str().unwrap_or_default().to_owned();
    match call.name.as_str() {
        "use_skill" => match get(data_dir, &s("name")) {
            Some(skill) => ToolOutcome {
                ok: true,
                summary: format!("Loaded the skill “{}”", skill.name),
                output: format!("Skill {} ({}):\n\n{}", skill.name, skill.description, skill.body),
            },
            None => {
                let names: Vec<String> = list(data_dir).into_iter().map(|s| s.name).collect();
                ToolOutcome::err(
                    format!("No skill “{}”", s("name")),
                    format!("There's no skill called {:?}. Saved skills: {}", s("name"), if names.is_empty() { "none".into() } else { names.join(", ") }),
                )
            }
        },
        "save_skill" => {
            let existed = get(data_dir, &s("name")).is_some();
            match save(data_dir, &s("name"), &s("description"), &s("instructions")) {
                Ok(skill) => {
                    tracing::info!("{agent_name} saved the skill {}", skill.name);
                    ToolOutcome {
                        ok: true,
                        summary: format!("{} the skill “{}”", if existed { "Updated" } else { "Saved" }, skill.name),
                        output: format!("Saved. Every agent sees “{}” in its skill list from now on.", skill.name),
                    }
                }
                Err(e) => ToolOutcome::err("Couldn't save the skill", e),
            }
        }
        other => {
            warn!("skills: unknown function {other}");
            ToolOutcome::err("Unknown skill function", format!("no function {other}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Order food on Wolt!"), "order-food-on-wolt");
        assert_eq!(slug("  --deploy  site-- "), "deploy-site");
        assert_eq!(slug("../../etc"), "etc");
        assert_eq!(slug("Ενημέρωση"), "ενημέρωση");
        assert_eq!(slug("!!!"), "");
    }

    #[test]
    fn save_list_load_delete() {
        let data = std::env::temp_dir().join(format!("iris-skills-{}", uuid::Uuid::new_v4().simple()));
        assert!(prompt_section(&data).contains("save_skill"));
        let saved = save(&data, "Check Site Uptime", "When asked if a site is up", "1. curl -I the URL\n2. Report the status").unwrap();
        assert_eq!(saved.name, "check-site-uptime");
        assert!(save(&data, "x", "", "body").is_err());
        assert!(save(&data, "x", "d", &"a".repeat(MAX_BODY + 1)).is_err());

        let text = fs::read_to_string(dir(&data).join("check-site-uptime/SKILL.md")).unwrap();
        assert!(text.starts_with("---\nname: check-site-uptime\ndescription: \"When asked if a site is up\"\n---"));
        assert_eq!(list(&data), vec![saved.clone()]);
        assert_eq!(get(&data, "check site uptime").unwrap(), saved);
        assert!(prompt_section(&data).contains("- check-site-uptime: When asked if a site is up"));

        // A skill copied in by hand, in the agentskills.io format.
        fs::create_dir_all(dir(&data).join("pdf")).unwrap();
        fs::write(dir(&data).join("pdf/SKILL.md"), "---\nname: pdf\ndescription: 'Work with PDFs'\nlicense: MIT\n---\n# PDF\nUse pypdf.").unwrap();
        assert_eq!(get(&data, "pdf").unwrap().description, "Work with PDFs");
        assert_eq!(get(&data, "pdf").unwrap().body, "# PDF\nUse pypdf.");

        let call = ToolCall { id: "1".into(), name: "use_skill".into(), arguments: r#"{"name":"pdf"}"#.into() };
        assert!(run(&data, "Iris", &call).output.contains("Use pypdf"));
        assert!(delete(&data, "pdf"));
        assert!(!run(&data, "Iris", &call).ok);
        let _ = fs::remove_dir_all(&data);
    }
}
