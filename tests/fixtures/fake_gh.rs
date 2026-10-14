//! Compile with rustc. Strict offline fixture; never delegates to another executable.
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

fn json_string(input: &str) -> String {
    let mut out = String::from("\"");
    for c in input.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn bytes(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_default()
}
fn number(path: &Path, default: u64) -> u64 {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}
fn main() {
    let fixture = PathBuf::from(
        std::env::var_os("GH_FIXTURE_DIR").expect("GH_FIXTURE_DIR required; network is forbidden"),
    );
    let lock = fixture.join(".fixture-lock");
    let started = Instant::now();
    while fs::create_dir(&lock).is_err() {
        if started.elapsed() > Duration::from_secs(5) {
            eprintln!("fixture lock deadline exceeded");
            std::process::exit(87);
        }
        thread::sleep(Duration::from_millis(2));
    }
    let step = number(&fixture.join("next-step"), 0);
    fs::write(fixture.join("next-step"), (step + 1).to_string()).expect("write step");
    let prefix = fixture.join(format!("{step:03}"));
    let file = |suffix: &str| PathBuf::from(format!("{}.{}", prefix.display(), suffix));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let expected = fs::read_to_string(file("args"))
        .ok()
        .map(|s| s.lines().map(str::to_owned).collect::<Vec<_>>());
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).expect("stdin");
    let matched = expected.as_ref() == Some(&args) && input == bytes(&file("stdin"));
    // Secret fixtures assert exact input bytes but never journal their plaintext or encoding.
    let hex = if file("sensitive-stdin").exists()
        || (args.first().is_some_and(|s| s == "secret")
            && args.get(1).is_some_and(|s| s == "set"))
    {
        "[REDACTED]".to_owned()
    } else {
        input.iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    let record = format!(
        "{{\"step\":{step},\"args\":[{}],\"stdin_hex\":{},\"pid\":{},\"matched\":{matched}}}\n",
        args.iter()
            .map(|s| json_string(s))
            .collect::<Vec<_>>()
            .join(","),
        json_string(&hex),
        std::process::id()
    );
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(fixture.join("commands.jsonl"))
        .expect("journal")
        .write_all(record.as_bytes())
        .expect("journal write");
    fs::remove_dir(&lock).expect("unlock");
    if !matched {
        eprintln!("Unexpected fixture command at step {step}: {args:?}; expected {expected:?}; stdin bytes {}",input.len());
        std::process::exit(86);
    }
    thread::sleep(Duration::from_millis(number(&file("delay-ms"), 0)));
    std::io::stdout()
        .write_all(&bytes(&file("stdout")))
        .expect("stdout");
    std::io::stderr()
        .write_all(&bytes(&file("stderr")))
        .expect("stderr");
    std::process::exit(number(&file("status"), 0) as i32);
}
