//! One fzf pick: start a new agent, or resume a claude/codex/pi chat started in this folder.
//! Session sources follow agf (github.com/subinium/agf) src/scanner/{claude,codex,pi}.rs.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const PI: &str = if cfg!(windows) { "pi.cmd" } else { "pi" };

struct Chat {
    agent: &'static str,
    id: String,
    title: String,
    ms: u128,
}

fn main() {
    let home = std::env::home_dir().expect("no home directory");
    let here = norm(&std::env::current_dir().unwrap().to_string_lossy());
    let mut chats = claude(&home, &here);
    chats.extend(codex(&home, &here));
    chats.extend(pi(&home, &here));
    chats.sort_by_key(|c| std::cmp::Reverse(c.ms));

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let mut rows = String::from("new\tclaude\t+ new claude\nnew\tcodex\t+ new codex\nnew\tpi\t+ new pi\n");
    for c in &chats {
        let title: String = c.title.split_whitespace().collect::<Vec<_>>().join(" ");
        rows += &format!("{}\t{}\t{:<6} {:>4}  {title}\n", c.agent, c.id, c.agent, age(now.saturating_sub(c.ms)));
    }

    let mut fzf = Command::new("fzf")
        .args([
            "--delimiter=\t", "--with-nth=3", "--no-sort", "--reverse", "--prompt=ai> ",
            "--margin=20%,15%", "--border=rounded", "--border-label= ai ", "--padding=0,1", "--info=inline-right",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("fzf not found on PATH");
    fzf.stdin.take().unwrap().write_all(rows.as_bytes()).unwrap();
    let out = fzf.wait_with_output().unwrap();
    let pick = String::from_utf8_lossy(&out.stdout);
    let mut parts = pick.trim_end().split('\t');
    let (Some(kind), Some(id)) = (parts.next(), parts.next()) else { return };

    let (program, args): (&str, Vec<&str>) = match kind {
        "new" if id == "pi" => (PI, vec![]),
        "new" => (id, vec![]),
        "claude" => ("claude", vec!["--resume", id]),
        "codex" => ("codex", vec!["resume", id]),
        _ => (PI, vec!["--session", id]),
    };
    let mut cmd = Command::new(program);
    cmd.args(args);
    #[cfg(unix)]
    {
        let err = std::os::unix::process::CommandExt::exec(&mut cmd);
        eprintln!("ai: {program}: {err}");
        std::process::exit(1);
    }
    #[cfg(windows)]
    {
        ignore_ctrl_c();
        std::process::exit(cmd.status().map_or(1, |s| s.code().unwrap_or(1)));
    }
}

/// Claude: every typed prompt is a line in history.jsonl, tagged with project and sessionId.
fn claude(home: &Path, here: &str) -> Vec<Chat> {
    let mut by_id: HashMap<String, Chat> = HashMap::new();
    for line in lines(&home.join(".claude/history.jsonl")) {
        let (Some(project), Some(id), Some(text), Some(ts)) =
            (field(&line, "project"), field(&line, "sessionId"), field(&line, "display"), field(&line, "timestamp"))
        else { continue };
        if norm(&project) != here || text.starts_with('/') {
            continue;
        }
        let ms = ts.parse().unwrap_or(0);
        let chat = by_id.entry(id.clone()).or_insert(Chat { agent: "claude", id, title: text, ms });
        chat.ms = chat.ms.max(ms);
    }
    by_id.into_values().collect()
}

/// Codex: history.jsonl holds typed prompts (never subagents) but no cwd; the rollout's first line has it.
fn codex(home: &Path, here: &str) -> Vec<Chat> {
    let mut in_here = HashSet::new();
    walk(&home.join(".codex/sessions"), &mut |p| {
        let stem = p.file_stem().unwrap().to_string_lossy();
        if stem.starts_with("rollout-")
            && stem.len() > 36
            && lines(p).next().and_then(|l| field(&l, "cwd")).is_some_and(|cwd| norm(&cwd) == here)
        {
            in_here.insert(stem[stem.len() - 36..].to_string());
        }
    });
    let mut by_id: HashMap<String, Chat> = HashMap::new();
    for line in lines(&home.join(".codex/history.jsonl")) {
        let (Some(id), Some(text), Some(ts)) = (field(&line, "session_id"), field(&line, "text"), field(&line, "ts"))
        else { continue };
        if !in_here.contains(&id) {
            continue;
        }
        let ms = ts.parse::<u128>().unwrap_or(0) * 1000;
        let chat = by_id.entry(id.clone()).or_insert(Chat { agent: "codex", id, title: text, ms });
        chat.ms = chat.ms.max(ms);
    }
    by_id.into_values().collect()
}

/// pi: one file per session; its first line is a header with the cwd, the first user message is the title.
fn pi(home: &Path, here: &str) -> Vec<Chat> {
    let mut chats = Vec::new();
    walk(&home.join(".pi/agent/sessions"), &mut |p| {
        let mut it = lines(p);
        if !it.next().and_then(|l| field(&l, "cwd")).is_some_and(|cwd| norm(&cwd) == here) {
            return;
        }
        let Some(title) = it.find(|l| l.contains(r#""role":"user""#)).and_then(|l| field(&l, "text")) else { return };
        let ms = fs::metadata(p).and_then(|m| m.modified()).map_or(0, |t| t.duration_since(UNIX_EPOCH).unwrap().as_millis());
        chats.push(Chat { agent: "pi", id: p.to_string_lossy().into_owned(), title, ms });
    });
    chats
}

fn lines(path: &Path) -> impl Iterator<Item = String> {
    File::open(path).into_iter().flat_map(|f| BufReader::new(f).lines().map_while(Result::ok))
}

fn walk(dir: &Path, f: &mut dyn FnMut(&Path)) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            f(&path);
        }
    }
}

/// Paths from the three agents differ in `\\?\` prefix, trailing slash, and case.
fn norm(path: &str) -> String {
    path.trim_start_matches(r"\\?\").trim_end_matches(['\\', '/']).to_lowercase()
}

/// The first `"key":` value on a JSON line, as text. Sound because a JSON string can't hold an unescaped `"`.
fn field(line: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":");
    let rest = line[line.find(&pat)? + pat.len()..].trim_start();
    let Some(body) = rest.strip_prefix('"') else {
        return Some(rest.chars().take_while(char::is_ascii_digit).collect());
    };
    let mut out: Vec<u16> = Vec::new();
    let mut chars = body.chars();
    loop {
        match chars.next()? {
            '"' => return Some(String::from_utf16_lossy(&out)),
            '\\' => match chars.next()? {
                'u' => out.push(u16::from_str_radix(&chars.by_ref().take(4).collect::<String>(), 16).ok()?),
                'n' | 't' | 'r' | 'b' | 'f' => out.push(b' '.into()),
                c => out.extend(c.encode_utf16(&mut [0; 2]).iter()),
            },
            c => out.extend(c.encode_utf16(&mut [0; 2]).iter()),
        }
    }
}

fn age(ms: u128) -> String {
    match ms / 60_000 {
        m if m < 60 => format!("{m}m"),
        m if m < 60 * 24 => format!("{}h", m / 60),
        m => format!("{}d", m / (60 * 24)),
    }
}

/// Windows has no exec, so `ai` stays the agent's parent. Ctrl+C reaches every process on the console;
/// without this, `ai` would exit and hand the prompt back while the agent is still running. A handler (not a NULL ignore) so the agent doesn't inherit it.
#[cfg(windows)]
fn ignore_ctrl_c() {
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(handler: Option<extern "system" fn(u32) -> i32>, add: i32) -> i32;
    }
    extern "system" fn swallow(_: u32) -> i32 {
        1
    }
    unsafe { SetConsoleCtrlHandler(Some(swallow), 1) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_reads_strings_escapes_and_numbers() {
        let line = r#"{"display":"a \"q\"\nb \u00e9 \ud83d\ude00","project":"C:\\Users\\x","timestamp":1790206839835}"#;
        assert_eq!(field(line, "display").unwrap(), "a \"q\" b é 😀");
        assert_eq!(field(line, "project").unwrap(), r"C:\Users\x");
        assert_eq!(field(line, "timestamp").unwrap(), "1790206839835");
        assert_eq!(field(line, "missing"), None);
        assert_eq!(norm(r"\\?\C:\Users\X\"), norm(r"c:\users\x"));
    }
}
