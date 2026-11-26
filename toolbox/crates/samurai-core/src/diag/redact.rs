//! Redaction for diagnostic bundles (spec 05 "Redaction").
//!
//! Replaces identifying data with stable placeholders (`<user1>`, `<mac1>`, ...): the same input
//! always maps to the same placeholder within one [`Redactor`], so a reader can still follow
//! "this MAC talks to that address" without learning either. Std only; the scanners are
//! deliberately conservative: a false positive costs a little readability, a miss leaks.

use std::collections::HashMap;

/// Keys whose value is a credential. Matched case-insensitively as a substring of the key word.
const SECRET_KEYS: [&str; 10] = [
    "password",
    "passwd",
    "secret",
    "token",
    "apikey",
    "api_key",
    "api-key",
    "passphrase",
    "psk",
    "authorization",
];
const SERIAL_KEYS: [&str; 1] = ["serial"];
const SSID_KEYS: [&str; 2] = ["essid", "ssid"];

/// Account and host names that carry no private information and appear all over system text
/// (`[samurai]` in pacman.conf, "user" in every log). Never replaced, even if an account has one.
const PUBLIC_NAMES: [&str; 9] = [
    "root",
    "user",
    "admin",
    "guest",
    "test",
    "samurai",
    "arch",
    "archlinux",
    "localhost",
];

#[derive(Default)]
pub struct Redactor {
    users: Vec<String>,
    hostname: Option<String>,
    machine_id: Option<String>,
    seen: HashMap<(&'static str, String), usize>,
    counts: HashMap<&'static str, usize>,
}

impl Redactor {
    /// `users`: login names to hide (system accounts are not secret and should not be passed).
    pub fn new(users: Vec<String>, hostname: Option<String>) -> Self {
        let public = |n: &str| PUBLIC_NAMES.contains(&n.to_ascii_lowercase().as_str());
        let users = users
            .into_iter()
            .filter(|u| u.len() >= 2 && !public(u))
            .collect();
        let hostname = hostname
            .map(|h| h.trim().to_string())
            .filter(|h| h.len() >= 2 && !public(h));
        Redactor {
            users,
            hostname,
            ..Redactor::default()
        }
    }

    /// Hide the machine id wherever it occurs, also inside paths such as
    /// `/var/log/journal/<id>/system.journal` where no token boundary surrounds it.
    pub fn with_machine_id(mut self, id: Option<String>) -> Self {
        self.machine_id = id
            .map(|i| i.trim().to_ascii_lowercase())
            .filter(|i| i.len() == 32 && i.bytes().all(|b| b.is_ascii_hexdigit()));
        self
    }

    fn placeholder(&mut self, kind: &'static str, original: &str) -> String {
        let key = (kind, original.to_ascii_lowercase());
        if let Some(n) = self.seen.get(&key) {
            return format!("<{kind}{n}>");
        }
        let n = self.counts.entry(kind).or_insert(0);
        *n += 1;
        let n = *n;
        self.seen.insert(key, n);
        format!("<{kind}{n}>")
    }

    pub fn redact(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut in_pem = false;
        for line in text.lines() {
            if in_pem {
                if line.contains("-----END") {
                    in_pem = false;
                }
                continue;
            }
            if line.contains("-----BEGIN") && line.contains("PRIVATE KEY") {
                in_pem = !line.contains("-----END");
                out.push_str("<secret-private-key>\n");
                continue;
            }
            out.push_str(&self.redact_line(line));
            out.push('\n');
        }
        if !text.ends_with('\n') && out.ends_with('\n') {
            out.pop();
        }
        out
    }

    fn redact_line(&mut self, line: &str) -> String {
        let mut s = line.to_string();
        if let Some(id) = self.machine_id.clone() {
            if s.contains(&id) {
                let ph = self.placeholder("machine-id", &id);
                s = s.replace(&id, &ph);
            }
        }
        s = self.keyed(&s, &SECRET_KEYS, "secret");
        s = self.keyed(&s, &SERIAL_KEYS, "serial");
        s = self.keyed(&s, &SSID_KEYS, "ssid");
        s = self.bearer(&s);
        s = self.map_tokens(&s);
        s = self.names(&s);
        s
    }

    /// `key=value`, `key: value`, `key = "quoted value"` where the key word contains one of `keys`.
    fn keyed(&mut self, line: &str, keys: &[&str], kind: &'static str) -> String {
        let lower = line.to_ascii_lowercase();
        let bytes = line.as_bytes();
        let mut out = String::new();
        let mut i = 0;
        while i < line.len() {
            let hit = keys.iter().find(|k| lower[i..].starts_with(*k));
            let Some(k) = hit else {
                let ch = line[i..].chars().next().unwrap_or(' ');
                out.push(ch);
                i += ch.len_utf8();
                continue;
            };
            // extend over the rest of the key word (and "number"/"no" after it: "Serial Number: X"),
            // then optional quote/space, then a separator
            let mut j = i + k.len();
            while j < line.len() && is_word(bytes[j]) {
                j += 1;
            }
            let after = line[j..].trim_start_matches([' ', '\t']);
            let gap = line.len() - j - after.len();
            for w in ["number", "num", "no"] {
                let is_word_end = after.len() == w.len()
                    || after.as_bytes().get(w.len()).is_some_and(|b| !is_word(*b));
                if gap > 0 && lower[j + gap..].starts_with(w) && is_word_end {
                    j += gap + w.len();
                    break;
                }
            }
            let mut sep_at = j;
            while sep_at < line.len() && matches!(bytes[sep_at], b' ' | b'\t' | b'"' | b'\'') {
                sep_at += 1;
            }
            if sep_at >= line.len() || !matches!(bytes[sep_at], b'=' | b':') {
                out.push_str(&line[i..j]);
                i = j;
                continue;
            }
            let is_colon = bytes[sep_at] == b':';
            let mut v = sep_at + 1;
            while v < line.len() && matches!(bytes[v], b' ' | b'\t') {
                v += 1;
            }
            let (value, end, quote) = value_at(line, v, is_colon);
            out.push_str(&line[i..v]);
            if value.is_empty() {
                i = v;
                continue;
            }
            let ph = self.placeholder(kind, &value);
            match quote {
                Some(q) => out.push_str(&format!("{q}{ph}{q}")),
                None => out.push_str(&ph),
            }
            i = end;
        }
        out
    }

    fn bearer(&mut self, line: &str) -> String {
        let lower = line.to_ascii_lowercase();
        let Some(p) = lower.find("bearer ") else {
            return line.to_string();
        };
        let start = p + "bearer ".len();
        let end = line[start..]
            .find(|c: char| c.is_whitespace())
            .map_or(line.len(), |e| start + e);
        if end == start {
            return line.to_string();
        }
        let ph = self.placeholder("secret", &line[start..end]);
        format!("{}{}{}", &line[..start], ph, &line[end..])
    }

    /// Whitespace/punctuation separated tokens: tokens, emails, MACs, IPs.
    fn map_tokens(&mut self, line: &str) -> String {
        let mut out = String::new();
        let mut rest = line;
        while !rest.is_empty() {
            let (tok_len, is_tok) = token_span(rest);
            let piece = &rest[..tok_len];
            if is_tok {
                out.push_str(&self.token(piece));
            } else {
                out.push_str(piece);
            }
            rest = &rest[tok_len..];
        }
        out
    }

    fn token(&mut self, t: &str) -> String {
        if let Some(r) = self.email(t) {
            return r;
        }
        if is_mac(t) {
            // one MAC, one placeholder, whatever the case or separator
            return self.placeholder("mac", &t.trim_matches([',', ';']).replace('-', ":"));
        }
        if let Some(r) = self.ipv4_in(t) {
            return r;
        }
        if is_ipv6(t) {
            // keep brackets, the prefix length and the scope id (`/64`, `%eth0`): not identifying
            let core = t.trim_matches(['[', ']', ',', ';']);
            let addr_end = core.find(['/', '%']).unwrap_or(core.len());
            let (addr, tail) = core.split_at(addr_end);
            let lead = &t[..t.find(core).unwrap_or(0)];
            let trail = &t[lead.len() + core.len()..];
            return format!("{lead}{}{tail}{trail}", self.placeholder("ip6-", addr));
        }
        if is_secretish(t) {
            return self.placeholder("secret", t);
        }
        t.to_string()
    }

    fn email(&mut self, t: &str) -> Option<String> {
        let at = t.find('@')?;
        let (local, domain) = (&t[..at], &t[at + 1..]);
        let domain = domain.trim_end_matches(['.', ',', ';', '>', ')']);
        let tail = &t[at + 1 + domain.len()..];
        let ok = !local.is_empty() && domain.contains('.') && !domain.starts_with('.');
        let local_start = local.trim_start_matches(['<', '(', '"']);
        let lead = &local[..local.len() - local_start.len()];
        ok.then(|| {
            format!(
                "{lead}{}{tail}",
                self.placeholder("email", &format!("{local_start}@{domain}"))
            )
        })
    }

    /// Dotted quads inside a token (`10.0.2.15`, `10.0.2.15/24`, `[10.0.2.15]:22`).
    fn ipv4_in(&mut self, t: &str) -> Option<String> {
        let b = t.as_bytes();
        let mut out = String::new();
        let mut i = 0;
        let mut changed = false;
        while i < b.len() {
            if b[i].is_ascii_digit() && (i == 0 || !is_ip_char(b[i - 1])) {
                if let Some(len) = quad_len(&t[i..]) {
                    let q = &t[i..i + len];
                    let after_ok = i + len >= b.len() || !is_ip_char(b[i + len]);
                    if after_ok && !q.starts_with("127.") && q != "0.0.0.0" {
                        out.push_str(&self.placeholder("ip4-", q));
                        i += len;
                        changed = true;
                        continue;
                    }
                }
            }
            let ch = t[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
        changed.then_some(out)
    }

    /// User names and the hostname as whole words.
    fn names(&mut self, line: &str) -> String {
        let mut s = line.to_string();
        let users = self.users.clone();
        for u in &users {
            s = self.whole_word(&s, u, "user");
        }
        if let Some(h) = self.hostname.clone() {
            s = self.whole_word(&s, &h, "host");
        }
        s
    }

    fn whole_word(&mut self, s: &str, word: &str, kind: &'static str) -> String {
        let (lower, w) = (s.to_ascii_lowercase(), word.to_ascii_lowercase());
        let b = s.as_bytes();
        let mut out = String::new();
        let mut i = 0;
        while let Some(p) = lower[i..].find(&w) {
            let (a, z) = (i + p, i + p + w.len());
            let before_ok = a == 0 || !is_word(b[a - 1]);
            let after_ok = z >= b.len() || !is_word(b[z]);
            out.push_str(&s[i..a]);
            if before_ok && after_ok {
                out.push_str(&self.placeholder(kind, word));
            } else {
                out.push_str(&s[a..z]);
            }
            i = z;
        }
        out.push_str(&s[i..]);
        out
    }
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn is_ip_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'.' || b == b'-'
}

/// Value after `key=` / `key:` starting at `v`: a quoted string, else a word (`=`) or the line (`:`).
fn value_at(line: &str, v: usize, whole_line: bool) -> (String, usize, Option<char>) {
    let b = line.as_bytes();
    if v < b.len() && matches!(b[v], b'"' | b'\'') {
        let q = b[v] as char;
        let end = line[v + 1..].find(q).map_or(line.len(), |e| v + 1 + e);
        return (
            line[v + 1..end].to_string(),
            (end + 1).min(line.len()),
            Some(q),
        );
    }
    let end = if whole_line {
        line.len()
    } else {
        line[v..]
            .find(|c: char| c.is_whitespace())
            .map_or(line.len(), |e| v + e)
    };
    (line[v..end].trim_end().to_string(), end, None)
}

/// Splits `s` into the next run of token characters or of separators. Returns (len, is_token).
fn token_span(s: &str) -> (usize, bool) {
    let tok = |c: char| {
        c.is_ascii_alphanumeric()
            || matches!(c, '@' | '.' | ':' | '-' | '_' | '%' | '/' | '+' | '=')
    };
    let first = s.chars().next().is_some_and(tok);
    let len = s.find(|c: char| tok(c) != first).unwrap_or(s.len());
    (len, first)
}

fn quad_len(s: &str) -> Option<usize> {
    let mut len = 0;
    for part in 0..4 {
        let digits: usize = s[len..].bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || digits > 3 || s[len..len + digits].parse::<u16>().ok()? > 255 {
            return None;
        }
        len += digits;
        if part < 3 {
            if s.as_bytes().get(len) != Some(&b'.') {
                return None;
            }
            len += 1;
        }
    }
    Some(len)
}

fn is_mac(t: &str) -> bool {
    let t = t.trim_matches(|c| c == ',' || c == ';');
    let sep = if t.contains(':') { ':' } else { '-' };
    let parts: Vec<&str> = t.split(sep).collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn is_ipv6(t: &str) -> bool {
    let t = t.trim_matches(|c| matches!(c, '[' | ']' | ',' | ';'));
    let t = t.split('%').next().unwrap_or(t);
    let t = t.split('/').next().unwrap_or(t);
    if t == "::1" || t == "::" || !t.contains(':') {
        return false;
    }
    let groups: Vec<&str> = t.split(':').collect();
    let hex = |g: &str| g.len() <= 4 && g.bytes().all(|b| b.is_ascii_hexdigit());
    let doubled = t.contains("::");
    groups.iter().all(|g| g.is_empty() || hex(g))
        && (doubled || groups.len() == 8)
        && t.matches("::").count() <= 1
}

/// 32+ chars of base64/hex alphabet with a digit and a letter, or a JWT.
fn is_secretish(t: &str) -> bool {
    let t = t.trim_matches(|c| matches!(c, ',' | ';' | '.' | '"' | '\''));
    if t.starts_with("eyJ") && t.matches('.').count() == 2 && t.len() >= 30 {
        return true;
    }
    let ok = |c: char| c.is_ascii_alphanumeric() || c == '_';
    t.len() >= 32
        && t.chars().all(ok)
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r() -> Redactor {
        Redactor::new(vec!["alice".into()], Some("archbox".into()))
    }

    #[test]
    fn machine_id_is_hidden_even_inside_paths() {
        let id = "3f97a028e94648b0b133cd79f23c5347";
        let mut rd = r().with_machine_id(Some(format!("{id}\n")));
        let out = rd.redact(&format!(
            "File /var/log/journal/{id}/system.journal corrupted\nid {id}\n"
        ));
        assert_eq!(
            out,
            "File /var/log/journal/<machine-id1>/system.journal corrupted\nid <machine-id1>\n"
        );
        // an unreadable or odd value changes nothing
        let mut none = r().with_machine_id(Some("uninitialized".into()));
        assert_eq!(none.redact("uninitialized\n"), "uninitialized\n");
    }

    #[test]
    fn users_and_hostname_are_whole_words_and_stable() {
        let out =
            r().redact("alice@archbox:/home/alice$ ls\nuser alice on archbox; alicewonder stays\n");
        assert!(!out.contains("alice@") || out.contains("<email"), "{out}");
        assert!(out.contains("/home/<user1>"), "{out}");
        assert!(out.contains("alicewonder"), "word boundaries: {out}");
        assert!(!out.contains("archbox"), "{out}");
        let mut rd = r();
        assert_eq!(rd.redact("alice\n"), "<user1>\n");
        assert_eq!(
            rd.redact("alice bob alice\n"),
            "<user1> bob <user1>\n",
            "stable per Redactor"
        );
    }

    #[test]
    fn public_names_are_never_redacted() {
        let mut rd = Redactor::new(
            vec!["samurai".into(), "admin".into(), "alice".into()],
            Some("archlinux".into()),
        );
        let out = rd.redact("[samurai] admin alice archlinux\n");
        assert_eq!(out, "[samurai] admin <user1> archlinux\n");
    }

    #[test]
    fn distro_words_inside_names_survive() {
        let out =
            Redactor::new(vec!["alice".into()], None).redact("[samurai]\nsamurai-tools 0.4.0-8\n");
        assert_eq!(out, "[samurai]\nsamurai-tools 0.4.0-8\n");
    }

    #[test]
    fn macs_both_separators_and_case() {
        let mut rd = r();
        let out =
            rd.redact("link/ether AA:BB:CC:11:22:33 brd ff:ff:ff:ff:ff:ff\nid aa-bb-cc-11-22-33\n");
        assert_eq!(out, "link/ether <mac1> brd <mac2>\nid <mac1>\n");
    }

    #[test]
    fn ipv4_outside_loopback_only() {
        let out = r().redact(
            "inet 10.0.2.15/24 gw 192.168.1.1 lo 127.0.0.1 any 0.0.0.0 conn [203.0.113.7]:443\n",
        );
        assert!(out.contains("<ip4-1>/24"), "{out}");
        assert!(out.contains("gw <ip4-2>"), "{out}");
        assert!(
            out.contains("lo 127.0.0.1") && out.contains("any 0.0.0.0"),
            "{out}"
        );
        assert!(out.contains("[<ip4-3>]:443"), "{out}");
        for leaked in ["10.0.2.15", "192.168.1.1", "203.0.113.7"] {
            assert!(!out.contains(leaked), "{leaked} leaked: {out}");
        }
    }

    #[test]
    fn version_numbers_with_non_octets_are_left_alone() {
        let out = r().redact("linux 6.12.1-arch1-1 build 2024.300.1.9999 pkg 1.2.3\n");
        assert_eq!(
            out,
            "linux 6.12.1-arch1-1 build 2024.300.1.9999 pkg 1.2.3\n"
        );
    }

    #[test]
    fn ipv6_non_loopback() {
        let out =
            r().redact("inet6 2001:db8::1234/64 fe80::a00:27ff:fe4e:66a1%eth0 ::1 time 12:34:56\n");
        assert!(out.contains("<ip6-1>/64"), "{out}");
        assert!(out.contains("<ip6-2>"), "{out}");
        assert!(out.contains(" ::1 ") && out.contains("12:34:56"), "{out}");
        assert!(
            !out.contains("2001:db8") && !out.contains("fe80::"),
            "{out}"
        );
    }

    #[test]
    fn emails() {
        let out =
            r().redact("Maintainer: Bob Builder <bob@example.org>, cc carol@mail.example.com.\n");
        assert_eq!(out, "Maintainer: Bob Builder <<email1>>, cc <email2>.\n");
    }

    #[test]
    fn keyed_secrets_quoted_unquoted_and_colon() {
        let mut rd = r();
        let out = rd.redact(
            "password=hunter2 next\nToken: abc def\napi_key = \"k e y\"\nWIFI_PSK=\"correct horse\"\nusername=bob\n",
        );
        assert_eq!(out, "password=<secret1> next\nToken: <secret2>\napi_key = \"<secret3>\"\nWIFI_PSK=\"<secret4>\"\nusername=bob\n");
        assert!(!out.contains("hunter2") && !out.contains("correct horse"));
    }

    #[test]
    fn bearer_tokens_and_jwts_and_long_tokens() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let hex = "0123456789abcdef0123456789abcdef";
        let out = r().redact(&format!(
            "Authorization: Bearer abc.def\ncookie {jwt}\nmachine-id {hex}\nfile.txt short 12345\n"
        ));
        assert!(out.contains("Authorization: <secret1>"), "{out}");
        assert!(!out.contains("eyJhbGci") && !out.contains(hex), "{out}");
        assert!(out.contains("file.txt short 12345"), "{out}");
    }

    #[test]
    fn long_plain_words_without_digits_are_kept() {
        let word = "supercalifragilisticexpialidocious_and_more";
        assert_eq!(r().redact(&format!("{word}\n")), format!("{word}\n"));
    }

    #[test]
    fn pem_private_keys_vanish_entirely() {
        let out = r().redact("before\n-----BEGIN OPENSSH PRIVATE KEY-----\nAAAAB3Nza\nC1lZDI1\n-----END OPENSSH PRIVATE KEY-----\nafter\n");
        assert_eq!(out, "before\n<secret-private-key>\nafter\n");
    }

    #[test]
    fn serials_and_ssids() {
        let out = r().redact("Serial Number: S3Z1NX0K123456 \nID_SERIAL_SHORT=ABC123\nESSID:\"HomeWifi\"\nssid: Cafe Net\n");
        assert_eq!(out, "Serial Number: <serial1>\nID_SERIAL_SHORT=<serial2>\nESSID:\"<ssid1>\"\nssid: <ssid2>\n");
    }

    #[test]
    fn mixed_realistic_block_leaks_nothing() {
        let input = "host archbox user alice mac 08:00:27:4e:66:a1 ip 10.0.2.15 mail alice@example.org serial=XY99 psk=\"abc\"\n";
        let out = r().redact(input);
        for leaked in ["archbox", "alice", "08:00:27", "10.0.2.15", "XY99", "abc"] {
            assert!(!out.contains(leaked), "{leaked} leaked: {out}");
        }
    }

    #[test]
    fn non_ascii_text_survives_every_scanner() {
        let line =
            "Zażółć gęślą jaźń: użytkownik alice, hasło=sekret ssid=Żabka 10.0.2.15 → ok ✓\n";
        let out = r().redact(line);
        assert!(
            out.contains("Zażółć gęślą jaźń") && out.contains("→ ok ✓"),
            "{out}"
        );
        assert!(
            !out.contains("alice") && !out.contains("10.0.2.15") && !out.contains("Żabka"),
            "{out}"
        );
        let kb = "hasło=x ssid=Łódź\n";
        assert_eq!(r().redact(kb), "hasło=x ssid=<ssid1>\n");
    }

    #[test]
    fn serial_number_phrasings() {
        let out = r().redact("Serial No: A1\nserial number = B2\nSerialNumber: C3\nserialized data\nserial is fine\n");
        assert_eq!(out, "Serial No: <serial1>\nserial number = <serial2>\nSerialNumber: <serial3>\nserialized data\nserial is fine\n");
    }

    #[test]
    fn trailing_newline_preserved_or_absent_as_given() {
        assert_eq!(r().redact("a\nb"), "a\nb");
        assert_eq!(r().redact("a\nb\n"), "a\nb\n");
        assert_eq!(r().redact(""), "");
    }
}
