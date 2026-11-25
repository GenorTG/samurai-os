//! Advice printed to users must name commands that exist: every `samurai ...` mention in a
//! message string has to parse.

use samurai_core::{boot, clean, diag, fix, health, kernel, mirrors, parse, Action};

const SOURCES: [&str; 5] = [
    include_str!("../src/kernel/mod.rs"),
    include_str!("../src/boot/mod.rs"),
    include_str!("../src/health/checks.rs"),
    include_str!("../src/mirrors/mod.rs"),
    include_str!("../src/fix/mod.rs"),
];

/// Words following each `samurai ` inside non-comment, non-test lines.
fn mentions(src: &str) -> Vec<Vec<String>> {
    let code = src.split("#[cfg(test)]").next().unwrap_or("");
    code.lines()
        .filter(|l| !l.trim_start().starts_with("//") && l.contains('"'))
        .flat_map(|l| {
            l.match_indices("samurai ")
                .map(move |(i, m)| &l[i + m.len()..])
        })
        .map(|rest| {
            let end = rest
                .find(['"', '(', ')', '`', ';', ':', '.', ',', '<', '['])
                .unwrap_or(rest.len());
            rest[..end].split_whitespace().map(str::to_string).collect()
        })
        .filter(|w: &Vec<String>| !w.is_empty())
        .collect()
}

#[test]
fn every_suggested_command_exists() {
    let mut seen = 0;
    for src in SOURCES {
        for words in mentions(src) {
            // `samurai update has completed`-style prose: only the command itself is checked
            let (cmd, rest) = (words[0].as_str(), &words[1..]);
            let args: Vec<String> = rest
                .iter()
                .take_while(|w| w.starts_with('-') || w.chars().all(|c| c.is_ascii_lowercase()))
                .cloned()
                .collect();
            let ok = match parse(cmd) {
                Action::Unknown(_) => false,
                // a bare command (usage lines such as `samurai fix <lock|...>`) only has to exist
                _ if args.is_empty() => true,
                Action::Clean => clean::parse_args(&args).is_ok(),
                Action::Fix => fix::parse_args(&args).is_ok(),
                Action::Mirrors => mirrors::parse_args(&args).is_ok(),
                Action::Boot => boot::parse_args(&args).is_ok(),
                Action::Kernel => kernel::parse_args(&args).is_ok(),
                Action::Health => health::parse_args(&args, health::checks::ALL).is_ok(),
                Action::Diag => diag::parse_args(&args).is_ok(),
                _ => true,
            };
            assert!(
                ok,
                "message suggests a command that does not work: samurai {}",
                words.join(" ")
            );
            seen += 1;
        }
    }
    assert!(
        seen >= 5,
        "the scanner found only {seen} mentions; it is probably broken"
    );
}
