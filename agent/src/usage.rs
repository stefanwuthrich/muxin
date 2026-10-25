//! What the agent CLIs on this machine used: tokens by hour, model and
//! folder, from their own logs, so the app can say what agents cost.
//! - Claude Code: `projects/*/*.jsonl` under every Claude config (`~/.claude`,
//!   `~/.claude-*`, `CLAUDE_CONFIG_DIR`). Each response is logged once per
//!   content block with the same usage: counted once, by message and request id.
//!   A subscription's plan meters (what `/usage` shows) come from Anthropic,
//!   using the Claude login already in that config. The tokens stay on this
//!   machine.
//! - Codex: `~/.codex/sessions/**/rollout-*.jsonl`, its `token_count` events
//!   (the last response's tokens), with the model from the turn before.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

/// Tokens, as they're priced: fresh input, cache reads, cache writes (5
/// minutes and an hour), output.
#[derive(Default, Clone, Copy)]
struct Sum {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    cache_write_1h: u64,
    requests: u64,
}

impl Sum {
    fn add(&mut self, o: &Sum) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
        self.cache_write_1h += o.cache_write_1h;
        self.requests += o.requests;
    }

    fn json(&self) -> Value {
        json!({ "input": self.input, "output": self.output, "cache_read": self.cache_read, "cache_write": self.cache_write, "cache_write_1h": self.cache_write_1h, "requests": self.requests })
    }
}

#[derive(Default)]
struct Tally {
    /// (hour "YYYY-MM-DDTHH" UTC, tool, model).
    hours: HashMap<(String, &'static str, String), Sum>,
    /// (tool, folder): its tokens, last use and models.
    folders: HashMap<(&'static str, String), (Sum, String, HashSet<String>)>,
}

impl Tally {
    fn add(&mut self, at: &str, tool: &'static str, model: &str, cwd: &str, sum: Sum) {
        let hour = at.get(..13).unwrap_or(at).to_string();
        self.hours
            .entry((hour, tool, model.to_string()))
            .or_default()
            .add(&sum);
        let f = self.folders.entry((tool, cwd.to_string())).or_default();
        f.0.add(&sum);
        if at > f.1.as_str() {
            f.1 = at.to_string();
        }
        f.2.insert(model.to_string());
    }
}

/// `{"op": "usage", "days"?}`: tokens the agent CLIs here used in the last
/// `days` (30 by default), by hour (UTC), tool and model, and by folder.
pub fn usage(req: &Value) -> Value {
    let days = req["days"].as_u64().unwrap_or(30).clamp(1, 400);
    let since = SystemTime::now() - Duration::from_secs(days * 86_400);
    let cutoff = utc_hour(since);
    // The user's home (crate::home_dir is the agent's own ~/.muxen).
    let home = crate::sys::user_home();
    let mut tally = Tally::default();
    let mut found = Vec::new();

    let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
    let claude_dirs = claude_homes(&home, config_dir.as_deref());
    let mut seen = HashSet::new();
    for dir in &claude_dirs {
        let files = jsonl_files(&dir.join("projects"), 2, since);
        if !files.is_empty() && !found.contains(&"claude") {
            found.push("claude");
        }
        for f in files {
            claude_file(&f, &cutoff, &mut seen, &mut tally);
        }
    }
    let plans = claude_dirs
        .iter()
        .filter_map(|dir| claude_plan(dir, &home))
        .collect::<Vec<_>>();
    let codex = jsonl_files(&home.join(".codex/sessions"), 4, since);
    if !codex.is_empty() {
        found.push("codex");
    }
    for f in codex {
        codex_file(&f, &cutoff, &mut tally);
    }

    let mut hours: Vec<Value> = tally
        .hours
        .iter()
        .map(|((hour, tool, model), s)| {
            let mut v = s.json();
            v["hour"] = json!(hour);
            v["tool"] = json!(tool);
            v["model"] = json!(model);
            v
        })
        .collect();
    hours.sort_by(|a, b| a["hour"].as_str().cmp(&b["hour"].as_str()));
    let mut folders: Vec<Value> = tally
        .folders
        .iter()
        .map(|((tool, cwd), (s, last, models))| {
            let mut v = s.json();
            v["tool"] = json!(tool);
            v["cwd"] = json!(cwd);
            v["last"] = json!(last);
            let mut m: Vec<&String> = models.iter().collect();
            m.sort();
            v["models"] = json!(m);
            v
        })
        .collect();
    folders.sort_by(|a, b| b["last"].as_str().cmp(&a["last"].as_str()));
    json!({ "ok": true, "days": days, "tools": found, "hours": hours, "folders": folders, "plans": plans })
}

/// Claude configs under `user_home`: `~/.claude`, `~/.config/claude`,
/// `CLAUDE_CONFIG_DIR`, and any `~/.claude-*` that holds a Claude config.
/// Missing directories are left out. `config_dir` is `CLAUDE_CONFIG_DIR`.
fn claude_homes(user_home: &Path, config_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = config_dir {
        dirs.push(dir.to_path_buf());
    }
    dirs.push(user_home.join(".claude"));
    dirs.push(user_home.join(".config").join("claude"));
    if let Ok(entries) = std::fs::read_dir(user_home) {
        let mut extra: Vec<PathBuf> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name();
                let name = name.to_str()?;
                if !name.starts_with(".claude-") {
                    return None;
                }
                let path = e.path();
                (path.join(".claude.json").is_file() || path.join(".credentials.json").is_file())
                    .then_some(path)
            })
            .collect();
        extra.sort();
        dirs.extend(extra);
    }
    let mut seen = HashSet::new();
    dirs.retain(|dir| {
        if !dir.is_dir() {
            return false;
        }
        let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
        seen.insert(key)
    });
    dirs
}

/// The public client id baked into Claude Code. It is not a secret.
const CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";

/// One Claude config's plan meters, or why they couldn't be read. `None` when
/// the directory has no Claude login file at all.
fn claude_plan(dir: &Path, user_home: &Path) -> Option<Value> {
    let label = home_label(dir, user_home);
    let cred = dir.join(".credentials.json");
    if !cred.is_file() {
        if dir.join(".claude.json").is_file() {
            return Some(plan_error(&label, "No Claude login here."));
        }
        return None;
    }
    let mut file = match read_oauth(&cred) {
        Ok(Some(file)) => file,
        Ok(None) => return Some(plan_error(&label, "No Claude login here.")),
        Err(e) => return Some(plan_error(&label, &e)),
    };
    let now = unix_ms();
    let token = if !file.access.is_empty() && file.expires_ms > now + 60_000 {
        file.access.clone()
    } else if file.refresh.is_empty() {
        return Some(plan_error(
            &label,
            "Claude's login has expired. Open Claude Code once.",
        ));
    } else {
        match refresh_login(&file.refresh) {
            Ok(fresh) => {
                file.access = fresh.access;
                if !fresh.refresh.is_empty() {
                    file.refresh = fresh.refresh;
                }
                file.expires_ms = fresh.expires_ms;
                if let Err(e) = write_oauth(&cred, &file) {
                    return Some(plan_error(&label, &e));
                }
                file.access.clone()
            }
            Err(e) => return Some(plan_error(&label, &e)),
        }
    };
    match fetch_usage(&token) {
        Ok(body) => Some(plan_json(&label, &body)),
        Err(e) => Some(plan_error(&label, &e)),
    }
}

fn home_label(dir: &Path, user_home: &Path) -> String {
    dir.strip_prefix(user_home)
        .map(|rel| rel.display().to_string())
        .unwrap_or_else(|_| {
            dir.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| dir.display().to_string())
        })
}

fn plan_error(home: &str, error: &str) -> Value {
    json!({ "tool": "claude", "home": home, "windows": [], "error": error })
}

struct OauthFile {
    root: Value,
    access: String,
    refresh: String,
    /// Unix milliseconds. 0 when the file doesn't say.
    expires_ms: i64,
}

struct FreshLogin {
    access: String,
    refresh: String,
    expires_ms: i64,
}

fn read_oauth(path: &Path) -> Result<Option<OauthFile>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Couldn't read the Claude login: {e}"))?;
    let root: Value = serde_json::from_str(&text)
        .map_err(|_| "The Claude login file isn't valid.".to_string())?;
    let Some(oauth) = root.get("claudeAiOauth").filter(|v| v.is_object()) else {
        return Ok(None);
    };
    let access = oauth
        .get("accessToken")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let refresh = oauth
        .get("refreshToken")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if access.is_empty() && refresh.is_empty() {
        return Ok(None);
    }
    let expires = oauth.get("expiresAt").and_then(|v| v.as_i64()).unwrap_or(0);
    Ok(Some(OauthFile {
        root,
        access,
        refresh,
        expires_ms: normalize_expiry(expires),
    }))
}

fn normalize_expiry(expires: i64) -> i64 {
    if expires > 0 && expires < 10_000_000_000 {
        expires * 1000
    } else {
        expires
    }
}

fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The body of the token refresh. The refresh token is the one already stored
/// for this Claude config; it is not logged.
fn refresh_body(refresh_token: &str) -> Value {
    json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": CLAUDE_CLIENT_ID,
    })
}

fn write_oauth(path: &Path, file: &OauthFile) -> Result<(), String> {
    let mut root = file.root.clone();
    let oauth = root
        .get_mut("claudeAiOauth")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| "The Claude login file isn't valid.".to_string())?;
    oauth.insert("accessToken".into(), json!(file.access));
    oauth.insert("refreshToken".into(), json!(file.refresh));
    if file.expires_ms > 0 {
        oauth.insert("expiresAt".into(), json!(file.expires_ms));
    }
    let text = serde_json::to_string_pretty(&root)
        .map_err(|_| "Couldn't write the Claude login.".to_string())?;
    let tmp = path.with_extension("json.tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mode = std::fs::metadata(path)
            .map(|m| m.permissions().mode())
            .unwrap_or(0o600);
        opts.mode(mode & 0o777);
    }
    let mut out = opts
        .open(&tmp)
        .map_err(|e| format!("Couldn't write the Claude login: {e}"))?;
    out.write_all(text.as_bytes())
        .map_err(|e| format!("Couldn't write the Claude login: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Couldn't write the Claude login: {e}"))?;
    Ok(())
}

fn refresh_login(refresh_token: &str) -> Result<FreshLogin, String> {
    let body = refresh_body(refresh_token);
    let response = http(move |client| {
        client
            .post(TOKEN_URL)
            .json(&body)
            .send()
            .map_err(|_| "Couldn't reach Claude's login service.".to_string())
    })?;
    let status = response.status().as_u16();
    if let Some(error) = refresh_status_error(status) {
        return Err(error);
    }
    let payload: Value = response
        .json()
        .map_err(|_| "Claude's login service gave an unexpected answer.".to_string())?;
    let access = payload["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Claude's login service gave no access token.".to_string())?
        .to_string();
    let refresh = payload["refresh_token"].as_str().unwrap_or("").to_string();
    let expires_ms = if let Some(secs) = payload["expires_in"].as_i64() {
        unix_ms() + secs.saturating_mul(1000)
    } else if let Some(at) = payload["expires_at"].as_i64().filter(|n| *n > 0) {
        normalize_expiry(at)
    } else {
        // No expiry in the answer: don't refresh again on the next read.
        unix_ms() + 5 * 60 * 1000
    };
    Ok(FreshLogin {
        access,
        refresh,
        expires_ms,
    })
}

fn fetch_usage(access: &str) -> Result<Value, String> {
    let access = access.to_string();
    let response = http(move |client| {
        client
            .get(USAGE_URL)
            .header("authorization", format!("Bearer {access}"))
            .header("anthropic-beta", OAUTH_BETA)
            .send()
            .map_err(|_| "Couldn't reach Claude's usage service.".to_string())
    })?;
    let status = response.status().as_u16();
    if let Some(error) = usage_status_error(status) {
        return Err(error);
    }
    response
        .json()
        .map_err(|_| "Claude's usage service gave an unexpected answer.".to_string())
}

/// Runs `call` on a fresh thread. `usage` is called inside the agent's tokio
/// runtime, and a blocking HTTP client must not start another runtime there.
fn http<T: Send + 'static>(
    call: impl FnOnce(reqwest::blocking::Client) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let built = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| "Couldn't start the usage request.".to_string());
        let result = built.and_then(call);
        let _ = tx.send(result);
    });
    rx.recv()
        .unwrap_or_else(|_| Err("The usage request stopped.".into()))
}

/// A failed token refresh. 401 and 403 mean the stored login can't be renewed.
fn refresh_status_error(status: u16) -> Option<String> {
    match status {
        401 | 403 => Some("Claude's login has expired. Open Claude Code once.".into()),
        s if s >= 400 => Some(format!("Claude's login service returned {s}.")),
        _ => None,
    }
}

/// A failed usage read. 401 and 403 mean the access token was refused.
fn usage_status_error(status: u16) -> Option<String> {
    match status {
        401 | 403 => Some("Claude's login was refused.".into()),
        s if s >= 400 => Some(format!("Claude's usage service returned {s}.")),
        _ => None,
    }
}

fn plan_json(home: &str, body: &Value) -> Value {
    let util = utilization_of(body);
    let mut windows = windows_from_limits(util);
    if windows.is_empty() {
        windows = windows_from_named(util);
    }
    let mut plan = json!({ "tool": "claude", "home": home, "windows": windows });
    if let Some(extra) = extra_usage(util) {
        plan["extra"] = extra;
    }
    if windows.is_empty() && plan.get("extra").is_none() {
        plan["error"] = json!("Claude's usage reply had no meters.");
    }
    plan
}

fn utilization_of(body: &Value) -> &Value {
    if body.get("limits").is_some()
        || body.get("extra_usage").is_some()
        || body.get("five_hour").is_some()
    {
        body
    } else if body.get("utilization").is_some() {
        &body["utilization"]
    } else {
        body
    }
}

fn windows_from_limits(util: &Value) -> Vec<Value> {
    let Some(limits) = util.get("limits").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    limits
        .iter()
        .filter_map(|limit| {
            let percent = limit.get("percent").and_then(Value::as_f64)?;
            let kind = limit.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let model = limit
                .pointer("/scope/model/display_name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());
            let label = match kind {
                "session" => "Current session".to_string(),
                "weekly_all" => "Current week".to_string(),
                "weekly_scoped" => match model {
                    Some(name) => format!("Current week, {name}"),
                    None => "Current week".to_string(),
                },
                _ => return None,
            };
            Some(json!({
                "label": label,
                "percent": percent,
                "resets_at": limit.get("resets_at").and_then(|v| v.as_str()).unwrap_or(""),
                "severity": limit.get("severity").and_then(|v| v.as_str()).unwrap_or(""),
            }))
        })
        .collect()
}

fn windows_from_named(util: &Value) -> Vec<Value> {
    [
        ("five_hour", "Current session"),
        ("seven_day", "Current week"),
    ]
    .into_iter()
    .filter_map(|(key, label)| {
        let block = util.get(key)?;
        let percent = block.get("utilization").and_then(Value::as_f64)?;
        Some(json!({
            "label": label,
            "percent": percent,
            "resets_at": block.get("resets_at").and_then(|v| v.as_str()).unwrap_or(""),
            "severity": "",
        }))
    })
    .collect()
}

fn extra_usage(util: &Value) -> Option<Value> {
    let extra = util.get("extra_usage")?;
    if extra.get("is_enabled").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let places = extra
        .get("decimal_places")
        .and_then(Value::as_u64)
        .unwrap_or(2)
        .min(6) as i32;
    let scale = 10f64.powi(places);
    let used = extra
        .get("used_credits")
        .and_then(Value::as_f64)
        .unwrap_or(0.)
        / scale;
    let limit = extra
        .get("monthly_limit")
        .and_then(Value::as_f64)
        .unwrap_or(0.)
        / scale;
    let currency = extra
        .get("currency")
        .and_then(|v| v.as_str())
        .unwrap_or("USD");
    Some(json!({ "used": used, "limit": limit, "currency": currency }))
}

/// `.jsonl` files under `dir` (down to `depth` folders) changed since `since`.
pub(crate) fn jsonl_files(dir: &Path, depth: usize, since: SystemTime) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            if depth > 1 {
                out.extend(jsonl_files(&path, depth - 1, since));
            }
        } else if path.extension().is_some_and(|x| x == "jsonl")
            && meta.modified().is_ok_and(|m| m >= since)
        {
            out.push(path);
        }
    }
    out
}

fn n(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

fn claude_file(path: &Path, cutoff: &str, seen: &mut HashSet<String>, tally: &mut Tally) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        // A response (with its usage); the fields needed are read straight off
        // the line, which also carries the response's whole text.
        if !line.contains("\"usage\":{") || !line.contains("\"role\":\"assistant\"") {
            continue;
        }
        let at = field(&line, "\"timestamp\":\"").unwrap_or("");
        let model = field(&line, "\"model\":\"").unwrap_or("");
        if at < cutoff || model.is_empty() || model.starts_with('<') {
            continue;
        }
        // The same response, once per content block: count it once.
        let id = line
            .find("\"id\":\"msg_")
            .map(|i| &line[i + 6..])
            .and_then(|r| r.split('"').next())
            .unwrap_or("");
        let key = format!("{id}:{}", field(&line, "\"requestId\":\"").unwrap_or(""));
        if key != ":" && !seen.insert(key) {
            continue;
        }
        let Some(u) = line
            .find("\"usage\":{")
            .and_then(|i| {
                serde_json::Deserializer::from_str(&line[i + 8..])
                    .into_iter::<Value>()
                    .next()
            })
            .and_then(Result::ok)
        else {
            continue;
        };
        let created = n(&u["cache_creation_input_tokens"]);
        let hour = n(&u["cache_creation"]["ephemeral_1h_input_tokens"]).min(created);
        let sum = Sum {
            input: n(&u["input_tokens"]),
            output: n(&u["output_tokens"]),
            cache_read: n(&u["cache_read_input_tokens"]),
            cache_write: created - hour,
            cache_write_1h: hour,
            requests: 1,
        };
        tally.add(
            at,
            "claude",
            model,
            field(&line, "\"cwd\":\"").unwrap_or(""),
            sum,
        );
    }
}

/// A string field's value, found by its key (`"name":"`) in a JSON line.
/// The first one: the record's own (its fields come before its content).
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let at = line.find(key)? + key.len();
    let rest = &line[at..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn codex_file(path: &Path, cutoff: &str, tally: &mut Tally) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let (mut cwd, mut model, mut total) = (String::new(), String::new(), 0u64);
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let p = &v["payload"];
        match v["type"].as_str().unwrap_or("") {
            "session_meta" => cwd = p["cwd"].as_str().unwrap_or("").to_string(),
            "turn_context" => {
                model = p["model"].as_str().unwrap_or(&model).to_string();
                if let Some(c) = p["cwd"].as_str() {
                    cwd = c.to_string();
                }
            }
            "event_msg" if p["type"] == "token_count" => {
                let info = &p["info"];
                if info.is_null() {
                    continue;
                }
                // Repeated with nothing new (a rate-limit update): skip.
                let now = n(&info["total_token_usage"]["total_tokens"]);
                if now == total {
                    continue;
                }
                total = now;
                let at = v["timestamp"].as_str().unwrap_or("");
                if at < cutoff {
                    continue;
                }
                let u = &info["last_token_usage"];
                let cached = n(&u["cached_input_tokens"]);
                let sum = Sum {
                    input: n(&u["input_tokens"]).saturating_sub(cached),
                    output: n(&u["output_tokens"]),
                    cache_read: cached,
                    cache_write: n(&u["cache_write_input_tokens"]),
                    cache_write_1h: 0,
                    requests: 1,
                };
                tally.add(
                    at,
                    "codex",
                    if model.is_empty() { "unknown" } else { &model },
                    &cwd,
                    sum,
                );
            }
            _ => {}
        }
    }
}

/// A time as "YYYY-MM-DDTHH" in UTC (how the logs' times start).
fn utc_hour(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, hour) = (secs / 86_400, (secs % 86_400) / 3600);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{hour:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_utc_hours() {
        assert_eq!(utc_hour(UNIX_EPOCH), "1970-01-01T00");
        assert_eq!(
            utc_hour(UNIX_EPOCH + Duration::from_secs(1_791_220_000)),
            "2026-10-05T17"
        );
        assert_eq!(
            utc_hour(UNIX_EPOCH + Duration::from_secs(951_782_400)),
            "2000-02-29T00"
        );
    }

    #[test]
    fn counts_claude_once_and_codex_by_turn() {
        let dir = std::env::temp_dir().join(format!("muxen-usage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let claude = dir.join("c.jsonl");
        let line = r#"{"type":"assistant","timestamp":"2026-10-12T17:00:01.000Z","cwd":"/w","requestId":"r1","message":{"model":"claude-opus-5-5","id":"msg_1","type":"message","role":"assistant","usage":{"input_tokens":5,"output_tokens":100,"cache_read_input_tokens":1000,"cache_creation_input_tokens":300,"cache_creation":{"ephemeral_1h_input_tokens":200}}}}"#;
        std::fs::write(&claude, format!("{line}\n{line}\n{{\"type\":\"user\"}}\n")).unwrap();
        let codex = dir.join("x.jsonl");
        std::fs::write(
            &codex,
            concat!(
                r#"{"timestamp":"2026-10-12T18:00:00Z","type":"session_meta","payload":{"cwd":"/w2"}}"#, "\n",
                r#"{"timestamp":"2026-10-12T18:00:01Z","type":"turn_context","payload":{"model":"gpt-6.1-sol"}}"#, "\n",
                r#"{"timestamp":"2026-10-12T18:00:05Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":900},"last_token_usage":{"input_tokens":800,"cached_input_tokens":600,"output_tokens":100}}}}"#, "\n",
                r#"{"timestamp":"2026-10-12T18:00:06Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":900},"last_token_usage":{"input_tokens":800,"cached_input_tokens":600,"output_tokens":100}}}}"#, "\n",
            ),
        )
        .unwrap();
        let mut tally = Tally::default();
        let mut seen = HashSet::new();
        claude_file(&claude, "2026-10-01T00", &mut seen, &mut tally);
        codex_file(&codex, "2026-10-01T00", &mut tally);
        let c = tally.hours[&(
            "2026-10-12T17".to_string(),
            "claude",
            "claude-opus-5-5".to_string(),
        )];
        assert_eq!(
            (
                c.input,
                c.output,
                c.cache_read,
                c.cache_write,
                c.cache_write_1h,
                c.requests
            ),
            (5, 100, 1000, 100, 200, 1)
        );
        let x = tally.hours[&(
            "2026-10-12T18".to_string(),
            "codex",
            "gpt-6.1-sol".to_string(),
        )];
        assert_eq!(
            (x.input, x.cache_read, x.output, x.requests),
            (200, 600, 100, 1)
        );
        assert_eq!(
            tally.folders[&("codex", "/w2".to_string())].1,
            "2026-10-12T18:00:05Z"
        );
        // Too old: not counted.
        let mut old = Tally::default();
        claude_file(&claude, "2026-10-13T00", &mut HashSet::new(), &mut old);
        assert!(old.hours.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_a_claude_directory_beside_the_default() {
        let home = std::env::temp_dir().join(format!("muxen-homes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".claude/projects")).unwrap();
        std::fs::create_dir_all(home.join(".claude-a")).unwrap();
        std::fs::write(home.join(".claude-a/.claude.json"), "{}").unwrap();
        std::fs::create_dir_all(home.join(".claude-empty")).unwrap();
        std::fs::create_dir_all(home.join("not-claude")).unwrap();
        let alias = home.join("from-env");
        std::fs::create_dir_all(&alias).unwrap();
        let homes = claude_homes(&home, Some(&alias));
        let has = |name: &str| homes.iter().any(|p| p.ends_with(name));
        assert!(has(".claude"));
        assert!(has(".claude-a"));
        assert!(has("from-env"));
        assert!(!has(".claude-empty"));
        assert!(!has("not-claude"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn parses_plan_windows_and_extra_usage() {
        let body = serde_json::json!({
            "limits": [
                {"kind": "session", "percent": 18, "resets_at": "2026-10-18T14:39:59Z", "severity": "normal"},
                {"kind": "amber_ladder", "percent": null},
                {"kind": "weekly_all", "percent": 49, "resets_at": "2026-10-20T00:59:59Z", "severity": "warning"},
                {"kind": "weekly_scoped", "percent": 0, "scope": {"model": {"display_name": "Fable"}}}
            ],
            "extra_usage": {
                "is_enabled": true,
                "used_credits": 6128,
                "monthly_limit": 40000,
                "decimal_places": 2,
                "currency": "USD"
            }
        });
        let plan = plan_json(".claude-a", &body);
        let windows = plan["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0]["label"], "Current session");
        assert_eq!(windows[0]["percent"], 18.0);
        assert_eq!(windows[1]["label"], "Current week");
        assert_eq!(windows[1]["severity"], "warning");
        assert_eq!(windows[2]["label"], "Current week, Fable");
        assert_eq!(plan["extra"]["used"], 61.28);
        assert_eq!(plan["extra"]["limit"], 400.0);
        assert_eq!(plan["extra"]["currency"], "USD");
        assert!(plan.get("error").is_none());
    }

    #[test]
    fn builds_the_refresh_request_without_calling_the_network() {
        let body = refresh_body("refresh-1");
        assert_eq!(body["grant_type"], "refresh_token");
        assert_eq!(body["refresh_token"], "refresh-1");
        assert_eq!(body["client_id"], CLAUDE_CLIENT_ID);
        assert_eq!(body.as_object().unwrap().len(), 3);
    }

    #[test]
    fn a_refused_or_expired_login_is_an_error_string() {
        let refused = usage_status_error(401).unwrap();
        assert_eq!(refused, "Claude's login was refused.");
        assert!(usage_status_error(200).is_none());
        let expired = refresh_status_error(401).unwrap();
        assert!(expired.contains("expired"));
        assert!(refresh_status_error(200).is_none());
        let shown = plan_error(".claude-a", &refused);
        assert_eq!(shown["error"], "Claude's login was refused.");
        assert!(shown["windows"].as_array().unwrap().is_empty());
        let dir = std::env::temp_dir().join(format!("muxen-oauth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cred = dir.join(".credentials.json");
        std::fs::write(
            &cred,
            r#"{"mcpOAuth":{"keep":true},"claudeAiOauth":{"accessToken":"old","refreshToken":"old-r","expiresAt":1}}"#,
        )
        .unwrap();
        let mut file = read_oauth(&cred).unwrap().unwrap();
        file.access = "new-access".into();
        file.refresh = "new-refresh".into();
        file.expires_ms = 1_800_000_000_000;
        write_oauth(&cred, &file).unwrap();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&cred).unwrap()).unwrap();
        assert_eq!(saved["mcpOAuth"]["keep"], true);
        assert_eq!(saved["claudeAiOauth"]["accessToken"], "new-access");
        assert_eq!(saved["claudeAiOauth"]["refreshToken"], "new-refresh");
        assert_eq!(saved["claudeAiOauth"]["expiresAt"], 1_800_000_000_000i64);
        let gone = dir.join("no-login");
        std::fs::create_dir_all(&gone).unwrap();
        std::fs::write(gone.join(".claude.json"), "{}").unwrap();
        let plan = claude_plan(&gone, &dir).unwrap();
        assert_eq!(plan["error"], "No Claude login here.");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
