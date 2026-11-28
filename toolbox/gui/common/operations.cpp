#include "operations.hpp"

#include <algorithm>
#include <array>
#include <cstddef>
#include <string_view>

namespace samurai::ops {
namespace {

using Args = std::vector<std::string>;
using Validator = bool (*)(const std::string &);

constexpr std::size_t kMaxApps = 32;

bool lower(char c) { return c >= 'a' && c <= 'z'; }
bool digit(char c) { return c >= '0' && c <= '9'; }
bool alnum(char c) { return lower(c) || digit(c) || (c >= 'A' && c <= 'Z'); }

/// `word(-word)*` of lower-case letters.
bool dashed_words(std::string_view s)
{
    if (s.empty() || s.front() == '-' || s.back() == '-') {
        return false;
    }
    for (std::size_t i = 0; i < s.size(); ++i) {
        if (!(lower(s[i]) || (s[i] == '-' && s[i - 1] != '-'))) {
            return false;
        }
    }
    return true;
}

bool check_id(const std::string &s)
{
    const auto dot = s.find('.');
    if (s.size() > 64 || dot == std::string::npos) {
        return false;
    }
    const std::string_view v{s};
    return dashed_words(v.substr(0, dot)) && dashed_words(v.substr(dot + 1));
}

/// Catalog app id or `@bundle`. The catalog itself is the allowlist; this keeps options out.
bool app_id(const std::string &s)
{
    std::string_view v{s};
    if (!v.empty() && v.front() == '@') {
        v.remove_prefix(1);
    }
    return !v.empty() && v.size() <= 64 && alnum(v.front())
        && std::all_of(v.begin(), v.end(), [](char c) { return alnum(c) || c == '-'; });
}

/// Kernel package name; `samurai kernel` accepts only its own catalog.
bool kernel_name(const std::string &s)
{
    return s.size() <= 40 && (s == "linux" || (s.rfind("linux-", 0) == 0 && dashed_words(s)));
}

bool timeout_seconds(const std::string &s)
{
    if (s.empty() || s.size() > 2 || !std::all_of(s.begin(), s.end(), digit)) {
        return false;
    }
    return std::stoi(s) <= 60;
}

bool parameter_text(const std::string &s)
{
    constexpr std::string_view extra = "._-=,:/+@";
    return !s.empty() && s.size() <= 200 && s.front() != '-' && s.front() != '='
        && std::all_of(s.begin(), s.end(), [&](char c) {
               return alnum(c) || extra.find(c) != std::string_view::npos;
           });
}

/// Parameters that run code or hand out a shell at boot. `samurai boot set` leaves them to a
/// root shell's user; a GUI button must not add them. The kernel and systemd treat `-` and `_`
/// alike, so names are compared with `-` folded to `_`.
bool added_parameter(const std::string &s)
{
    if (!parameter_text(s)) {
        return false;
    }
    std::string key = s.substr(0, s.find('='));
    std::replace(key.begin(), key.end(), '-', '_');
    constexpr std::array<std::string_view, 3> harmless{"systemd.show_status", "systemd.log_level",
                                                       "rd.udev.log_level"};
    if (std::find(harmless.begin(), harmless.end(), key) != harmless.end()) {
        return true;
    }
    if (key.rfind("systemd.", 0) == 0 || key.rfind("rd.", 0) == 0) {
        return false;
    }
    constexpr std::array<std::string_view, 12> boot_modes{
        "init", "rdinit", "single", "emergency", "rescue", "s", "S", "1", "2", "3", "4", "5"};
    return std::find(boot_modes.begin(), boot_modes.end(), key) == boot_modes.end();
}

/// One line of printable text; UTF-8 bytes pass, control characters do not.
bool snapshot_label(const std::string &s)
{
    return !s.empty() && s.size() <= 100 && s.front() != '-'
        && std::all_of(s.begin(), s.end(), [](char c) {
               const auto u = static_cast<unsigned char>(c);
               return u >= 0x20 && u != 0x7f;
           });
}

bool snapshot_id(const std::string &s)
{
    return !s.empty() && s.size() <= 9 && s.front() != '0'
        && std::all_of(s.begin(), s.end(), digit);
}

bool clean_target(const std::string &s)
{
    // only the targets that need root; user caches are cleaned without privileges
    return s == "pkgcache" || s == "journal" || s == "coredumps" || s == "orphans";
}

enum class Arity { None, One, Many };

struct Operation {
    std::string_view verb;
    Arity arity;
    Validator valid;
    std::vector<std::string_view> before;  // fixed arguments before the user values
    std::vector<std::string_view> after;   // fixed arguments after them
};

const std::vector<Operation> &table()
{
    static const std::vector<Operation> t{
        // read-only
        {"health-read", Arity::None, nullptr, {"health", "--json"}, {}},
        {"kernel-list", Arity::None, nullptr, {"kernel", "list", "--json"}, {}},
        {"boot-get", Arity::None, nullptr, {"boot", "get", "--json"}, {}},
        {"snap-list", Arity::None, nullptr, {"snap", "list", "--json"}, {}},
        {"diag-bundle", Arity::None, nullptr, {"diag", "bundle", "--out", "-"}, {}},
        // health
        {"health-fix", Arity::One, check_id, {"health", "--fix", "--yes", "--id"}, {}},
        {"health-fix-safe", Arity::None, nullptr, {"health", "--fix"}, {}},
        // updates and apps (`--` ends option parsing; vendor scripts never run from here)
        {"update", Arity::None, nullptr, {"update", "--json", "--noconfirm"}, {}},
        {"update-ack-news", Arity::None, nullptr,
         {"update", "--json", "--noconfirm", "--ack-news"}, {}},
        {"apps-install", Arity::Many, app_id,
         {"apps", "install", "--yes", "--no-user-scripts", "--"}, {}},
        {"apps-install-upgrade", Arity::Many, app_id,
         {"apps", "install", "--yes", "--no-user-scripts", "--with-upgrade", "--"}, {}},
        {"apps-remove", Arity::Many, app_id, {"apps", "remove", "--yes", "--"}, {}},
        // cleanup
        {"clean", Arity::Many, clean_target, {"clean"}, {"--yes"}},
        // kernels and boot
        {"kernel-install", Arity::One, kernel_name, {"kernel", "install"}, {}},
        {"kernel-install-anyway", Arity::One, kernel_name, {"kernel", "install"}, {"--yes"}},
        {"kernel-remove", Arity::One, kernel_name, {"kernel", "remove"}, {}},
        {"kernel-default", Arity::One, kernel_name, {"kernel", "default"}, {}},
        {"boot-timeout", Arity::One, timeout_seconds, {"boot", "set", "timeout"}, {}},
        {"boot-param-add", Arity::One, added_parameter, {"boot", "set", "param", "add"}, {}},
        {"boot-param-remove", Arity::One, parameter_text,
         {"boot", "set", "param", "remove"}, {}},
        // snapshots
        {"snap-create", Arity::One, snapshot_label, {"snap", "create"}, {}},
        {"snap-delete", Arity::One, snapshot_id, {"snap", "delete"}, {}},
        // repairs
        {"fix-lock", Arity::None, nullptr, {"fix", "lock"}, {}},
        {"fix-keyring", Arity::None, nullptr, {"fix", "keyring"}, {}},
        {"fix-keyring-reset", Arity::None, nullptr, {"fix", "keyring", "--reset", "--yes"}, {}},
        {"fix-dkms", Arity::None, nullptr, {"fix", "dkms"}, {}},
        {"mirrors-refresh", Arity::None, nullptr, {"mirrors", "refresh"}, {}},
        // hardware: fixed commands only, never a profile name from the GUI
        {"hw-apply", Arity::None, nullptr, {"hw", "apply", "--yes"}, {}},
        {"hw-apply-switch", Arity::None, nullptr, {"hw", "apply", "--yes", "--switch-driver"}, {}},
        {"hw-accept", Arity::None, nullptr, {"hw", "fingerprint", "--accept"}, {}},
    };
    return t;
}

}  // namespace

std::vector<std::string> verbs()
{
    std::vector<std::string> out;
    for (const auto &op : table()) {
        out.emplace_back(op.verb);
    }
    return out;
}

std::optional<std::vector<std::string>> build(const std::string &verb, const Args &args,
                                              std::string &error)
{
    const auto &t = table();
    const auto op = std::find_if(t.begin(), t.end(),
                                 [&](const Operation &o) { return o.verb == verb; });
    if (op == t.end()) {
        error = "unknown operation";
        return std::nullopt;
    }
    const bool count_ok = (op->arity == Arity::None && args.empty())
        || (op->arity == Arity::One && args.size() == 1)
        || (op->arity == Arity::Many && !args.empty() && args.size() <= kMaxApps);
    if (!count_ok) {
        error = verb + ": wrong number of arguments";
        return std::nullopt;
    }
    for (const auto &a : args) {
        // a NUL would truncate the argument at exec time
        if (a.find('\0') != std::string::npos || !op->valid(a)) {
            error = verb + ": argument not allowed";
            return std::nullopt;
        }
    }
    Args out(op->before.begin(), op->before.end());
    out.insert(out.end(), args.begin(), args.end());
    out.insert(out.end(), op->after.begin(), op->after.end());
    return out;
}

}  // namespace samurai::ops
