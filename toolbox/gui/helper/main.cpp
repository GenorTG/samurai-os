// samurai-gui-helper: the only thing the Samurai Toolbox runs as root (through pkexec).
//
//   pkexec /usr/lib/samurai/gui-helper <verb> [args]
//
// It accepts the verbs in common/operations.cpp, validates every argument, and executes
// /usr/local/bin/samurai with a fixed argument vector and a clean environment. No shell, no paths
// from the caller. It stays alive to relay output, so a toolbox that crashes or is closed cannot
// break the pipe under a running pacman transaction.
//
// Exit codes: the CLI's own; 64 refused request; 70 cannot start the CLI.
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <syslog.h>
#include <unistd.h>

#include <array>
#include <cerrno>
#include <climits>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include "../common/operations.hpp"

// Overridden only by the unit tests (tests/unit/test_gui_helper.py).
#ifndef SAMURAI_CLI
#define SAMURAI_CLI "/usr/local/bin/samurai"
#endif
#ifndef SAMURAI_REQUIRE_ROOT
#define SAMURAI_REQUIRE_ROOT 1
#endif
#ifndef SAMURAI_STALL_MS
#define SAMURAI_STALL_MS 30000
#endif

namespace {

constexpr int kRefused = 64;
constexpr int kCannotRun = 70;

/// The numeric uid pkexec reports for the caller, or empty.
std::string caller_uid()
{
    const char *v = std::getenv("PKEXEC_UID");
    const std::string s = v ? v : "";
    if (s.empty() || s.size() > 10 || s.find_first_not_of("0123456789") != std::string::npos) {
        return {};
    }
    return s;
}

bool same_file(int a, int b)
{
    struct stat sa{};
    struct stat sb{};
    return fstat(a, &sa) == 0 && fstat(b, &sb) == 0 && sa.st_dev == sb.st_dev
        && sa.st_ino == sb.st_ino;
}

/// In the child: drop everything inherited, then exec the CLI.
[[noreturn]] void exec_cli(const std::vector<std::string> &args, int out, int err,
                           const std::string &uid)
{
    setsid();  // signals aimed at the caller's process group never reach the transaction
    const int null = open("/dev/null", O_RDONLY);
    if (null < 0 || dup2(null, 0) < 0 || dup2(out, 1) < 0 || dup2(err, 2) < 0) {
        _exit(kCannotRun);
    }
#ifdef SYS_close_range
    syscall(SYS_close_range, 3U, ~0U, 0U);
#endif
    for (const int sig : {SIGPIPE, SIGHUP, SIGINT, SIGQUIT, SIGTERM}) {
        signal(sig, SIG_DFL);
    }
    clearenv();
    setenv("PATH", "/usr/local/sbin:/usr/local/bin:/usr/bin", 1);
    setenv("LANG", "C.UTF-8", 1);
    setenv("LC_ALL", "C.UTF-8", 1);
    setenv("HOME", "/root", 1);
    if (!uid.empty()) {
        setenv("PKEXEC_UID", uid.c_str(), 1);
    }
    if (chdir("/") != 0) {
        _exit(kCannotRun);
    }
    umask(022);

    std::vector<char *> argv;
    static char program[] = SAMURAI_CLI;
    argv.push_back(program);
    for (const auto &a : args) {
        argv.push_back(const_cast<char *>(a.c_str()));
    }
    argv.push_back(nullptr);
    execv(program, argv.data());
    std::perror("samurai-gui-helper: cannot run " SAMURAI_CLI);
    _exit(kCannotRun);
}

/// Forward `n` bytes. Returns false once the reader is gone or stuck: output is then discarded.
bool forward(int fd, const char *data, ssize_t n)
{
    while (n > 0) {
        pollfd p{fd, POLLOUT, 0};
        const int ready = poll(&p, 1, SAMURAI_STALL_MS);
        if (ready < 0 && errno == EINTR) {
            continue;
        }
        if (ready <= 0 || (p.revents & (POLLERR | POLLHUP | POLLNVAL))) {
            return false;
        }
        // at most PIPE_BUF after POLLOUT: this write cannot block
        const ssize_t chunk = n < PIPE_BUF ? n : PIPE_BUF;
        const ssize_t w = write(fd, data, static_cast<size_t>(chunk));
        if (w < 0) {
            if (errno == EINTR || errno == EAGAIN) {
                continue;
            }
            return false;
        }
        data += w;
        n -= w;
    }
    return true;
}

/// Copy the child's output until it exits. Daemons it started may keep the pipes open, so after
/// the exit only what is already buffered is read.
int relay(pid_t child, int out, int err)
{
    std::array<pollfd, 2> fds{{{out, POLLIN, 0}, {err, POLLIN, 0}}};
    std::array<bool, 2> alive{true, true};
    const std::array<int, 2> sink{1, 2};
    std::array<char, 8192> buf{};
    int status = 0;
    bool exited = false;
    int idle_after_exit = 0;
    int reads_after_exit = 0;
    while (fds[0].fd >= 0 || fds[1].fd >= 0) {
        const int ready = poll(fds.data(), fds.size(), 200);
        if (ready < 0 && errno != EINTR) {
            break;
        }
        for (size_t i = 0; i < fds.size() && ready > 0; ++i) {
            if (fds[i].fd < 0 || !(fds[i].revents & (POLLIN | POLLHUP | POLLERR))) {
                continue;
            }
            const ssize_t n = read(fds[i].fd, buf.data(), buf.size());
            if (n > 0) {
                alive[i] = alive[i] && forward(sink[i], buf.data(), n);
                reads_after_exit += exited ? 1 : 0;
            } else if (n == 0 || errno != EINTR) {
                close(fds[i].fd);
                fds[i].fd = -1;
            }
        }
        if (!exited && waitpid(child, &status, WNOHANG) == child) {
            exited = true;
        } else if (exited) {
            idle_after_exit = ready > 0 ? 0 : idle_after_exit + 1;
            if (idle_after_exit >= 2 || reads_after_exit > 2000) {
                break;
            }
        }
    }
    while (!exited) {
        const pid_t r = waitpid(child, &status, 0);
        if (r == child) {
            exited = true;
        } else if (r < 0 && errno != EINTR) {
            return kCannotRun;
        }
    }
    if (WIFEXITED(status)) {
        return WEXITSTATUS(status);
    }
    return WIFSIGNALED(status) ? 128 + WTERMSIG(status) : kCannotRun;
}

}  // namespace

int main(int argc, char **argv)
{
    if (SAMURAI_REQUIRE_ROOT && geteuid() != 0) {
        std::fputs("samurai-gui-helper: run through pkexec\n", stderr);
        return kRefused;
    }
    if (argc < 2) {
        std::fputs("usage: samurai-gui-helper <operation> [arguments]\n", stderr);
        return kRefused;
    }
    const std::string verb = argv[1];
    const std::vector<std::string> args(argv + 2, argv + argc);
    const std::string uid = caller_uid();
    openlog("samurai-gui-helper", LOG_PID, LOG_AUTHPRIV);

    std::string error;
    const auto command = samurai::ops::build(verb, args, error);
    if (!command) {
        // the rejected text is not logged: it is attacker-controlled
        syslog(LOG_WARNING, "refused request from uid %s: %s", uid.c_str(),
               error == "unknown operation" ? "unknown operation" : "bad arguments");
        std::fprintf(stderr, "samurai-gui-helper: %s\n", error.c_str());
        return kRefused;
    }
    std::string line;
    for (const auto &a : *command) {
        line += ' ';
        line += a;
    }
    syslog(LOG_NOTICE, "uid %s runs: samurai%s", uid.c_str(), line.c_str());

    // one shared pipe when the caller gave us one for both streams: keeps their order
    const bool merged = same_file(1, 2);
    int out[2] = {-1, -1};
    int err[2] = {-1, -1};
    if (pipe2(out, O_CLOEXEC) != 0 || (!merged && pipe2(err, O_CLOEXEC) != 0)) {
        std::perror("samurai-gui-helper: pipe");
        return kCannotRun;
    }
    const pid_t child = fork();
    if (child < 0) {
        std::perror("samurai-gui-helper: fork");
        return kCannotRun;
    }
    if (child == 0) {
        exec_cli(*command, out[1], merged ? out[1] : err[1], uid);
    }
    close(out[1]);
    if (!merged) {
        close(err[1]);
    }
    // neither a vanished reader nor a closing session may interrupt the running operation
    for (const int sig : {SIGPIPE, SIGHUP, SIGINT, SIGQUIT, SIGTERM}) {
        signal(sig, SIG_IGN);
    }
    const int code = relay(child, out[0], err[0]);
    syslog(LOG_NOTICE, "samurai %s finished with exit code %d", verb.c_str(), code);
    return code;
}
