//! `switch-use upgrade` and the daily "new version" notice, per the use-family
//! convention (leeguooooo/plugins docs/upgrade.md). Network calls go through
//! curl so the binary carries no TLS stack.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

const NAME: &str = "switch-use";
const REPO: &str = "leeguooooo/switch-use";
const CURRENT: &str = env!("CARGO_PKG_VERSION");
const DAY: u64 = 24 * 3600;

fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

fn newer(latest: &str, current: &str) -> bool {
    matches!((parse(latest), parse(current)), (Some(l), Some(c)) if l > c)
}

/// The newest non-prerelease release tag, without the leading v.
fn latest(timeout_s: u32) -> Result<String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "-m", &timeout_s.to_string(), "-H", "Accept: application/vnd.github+json"]);
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        cmd.args(["-H", &format!("Authorization: Bearer {token}")]);
    }
    let out = cmd
        .arg(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .output()
        .context("run curl")?;
    if !out.status.success() {
        bail!("GitHub release check failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let v: Value = serde_json::from_slice(&out.stdout).context("parse GitHub reply")?;
    let tag = v["tag_name"].as_str().context("no tag_name in the latest release")?;
    Ok(tag.trim_start_matches('v').to_string())
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn cache_file() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".cache"));
    base.join(NAME).join("update-check.json")
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// At most once a day, one stderr line when a newer release exists. Silent on any failure.
pub fn notice() {
    if ["CI", "SWITCH_USE_NO_UPDATE_CHECK", "USE_NO_UPDATE_CHECK"].iter().any(|k| std::env::var_os(k).is_some()) {
        return;
    }
    let path = cache_file();
    let cached: Option<Value> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok());
    let fresh = cached.as_ref().and_then(|c| c["checked_at"].as_u64()).is_some_and(|t| now().saturating_sub(t) < DAY);
    let latest = if fresh {
        cached.and_then(|c| c["latest"].as_str().map(String::from))
    } else {
        let l = latest(2).ok();
        // Record the attempt even when offline, so the next call does not retry.
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(&path, json!({ "checked_at": now(), "latest": l }).to_string());
        l
    };
    if let Some(l) = latest.filter(|l| newer(l, CURRENT)) {
        eprintln!("{NAME} {l} is available (you have {CURRENT}). Upgrade: {NAME} upgrade");
    }
}

struct Skill {
    channel: &'static str,
    path: PathBuf,
    update: String,
}

fn git_root(p: &Path) -> Option<PathBuf> {
    let out = Command::new("git").arg("-C").arg(p).args(["rev-parse", "--show-toplevel"]).output().ok()?;
    out.status.success().then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

fn find_skills() -> Vec<Skill> {
    let mut found = Vec::new();
    let plugins = home().join(".claude/plugins/installed_plugins.json");
    if std::fs::read_to_string(&plugins).is_ok_and(|s| s.contains(&format!("\"{NAME}@"))) {
        found.push(Skill {
            channel: "claude-plugin",
            path: plugins,
            update: format!("claude plugin update {NAME}@leeguooooo-plugins"),
        });
    }
    for dir in [".agents/skills", ".claude/skills", ".codex/skills"] {
        let p = home().join(dir).join(NAME);
        let Ok(real) = p.canonicalize() else { continue };
        if !real.join("SKILL.md").exists() || found.iter().any(|s| s.path == real) {
            continue;
        }
        let skill = match git_root(&real) {
            Some(root) => Skill { channel: "git", update: format!("git -C {} pull --ff-only", root.display()), path: real },
            None => Skill { channel: "copy", update: format!("npx skills update {NAME}"), path: real },
        };
        if !found.iter().any(|s: &Skill| s.update == skill.update) {
            found.push(skill);
        }
    }
    found
}

fn refresh(skill: &Skill) {
    let ran = match skill.channel {
        "claude-plugin" if which("claude") => {
            Command::new("claude").args(["plugin", "update", &format!("{NAME}@leeguooooo-plugins")]).status()
        }
        "git" => Command::new("sh").args(["-c", &skill.update]).status(),
        _ => {
            println!("skill ({}) {}: update it with `{}`", skill.channel, skill.path.display(), skill.update);
            return;
        }
    };
    match ran {
        Ok(s) if s.success() => println!("skill ({}) {}: updated", skill.channel, skill.path.display()),
        _ => println!("skill ({}) {}: not updated; run `{}`", skill.channel, skill.path.display(), skill.update),
    }
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

/// Exit code: 0 done or checked, 2 the check or download failed.
pub fn run(check: bool, json_out: bool) -> i32 {
    let latest = match latest(10) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{NAME}: {e:#}");
            return 2;
        }
    };
    let available = newer(&latest, CURRENT);
    let skills = find_skills();
    if json_out {
        let list: Vec<_> = skills
            .iter()
            .map(|s| json!({ "channel": s.channel, "path": s.path, "update": s.update }))
            .collect();
        println!(
            "{}",
            json!({ "name": NAME, "current": CURRENT, "latest": latest, "update_available": available, "skills": list })
        );
        return 0;
    }
    if check {
        if available {
            println!("{NAME} {CURRENT} -> {latest}");
        } else {
            println!("{NAME} {CURRENT} is up to date");
        }
        return 0;
    }
    if available {
        // Same route as the original install: the release installer, into this binary's directory.
        let dir = std::env::current_exe().ok().and_then(|p| p.canonicalize().ok()).and_then(|p| p.parent().map(Path::to_path_buf));
        let mut cmd = Command::new("sh");
        cmd.args(["-c", &format!("curl -fsSL https://raw.githubusercontent.com/{REPO}/main/install.sh | sh")]);
        if let Some(d) = dir {
            cmd.env("SWITCH_USE_INSTALL_DIR", d);
        }
        cmd.env("SWITCH_USE_VERSION", format!("v{latest}"));
        match cmd.status() {
            Ok(s) if s.success() => println!("{NAME} {CURRENT} -> {latest}"),
            _ => {
                eprintln!("{NAME}: install of {latest} failed");
                return 2;
            }
        }
    } else {
        println!("{NAME} {CURRENT} is up to date");
    }
    let _ = std::fs::write(cache_file(), json!({ "checked_at": now(), "latest": latest }).to_string());
    for s in &skills {
        refresh(s);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("0.2.1", "0.2.0"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("garbage", "0.2.0"));
    }
}
