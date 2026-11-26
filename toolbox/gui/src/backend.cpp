#include "backend.hpp"

#include <QClipboard>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QQmlEngine>
#include <QRegularExpression>
#include <QTimer>
#include <QUrl>

#include "../common/operations.hpp"

namespace {

constexpr int kQueryTimeoutMs = 11 * 60 * 1000;  // `status --check` may take ten minutes
constexpr qsizetype kMaxCaptureBytes = 64 * 1024 * 1024;
constexpr int kRefused = 64;
constexpr int kAuthDismissed = 126;  // pkexec(1)
constexpr int kNotAuthorized = 127;

std::vector<std::string> toStd(const QStringList &list)
{
    std::vector<std::string> out;
    out.reserve(list.size());
    for (const auto &s : list) {
        out.push_back(s.toStdString());
    }
    return out;
}

/// Shell-style quoting, for display only: nothing is ever run through a shell.
QString quoted(const QString &arg)
{
    static const QRegularExpression plain(QStringLiteral("^[A-Za-z0-9@._:/=,+-]+$"));
    if (plain.match(arg).hasMatch()) {
        return arg;
    }
    QString inner = arg;
    inner.replace(QLatin1Char('\''), QLatin1String("'\\''"));
    return QLatin1Char('\'') + inner + QLatin1Char('\'');
}

}  // namespace

Backend::Backend(Programs programs, QObject *parent)
    : QObject(parent), m_programs(std::move(programs))
{
}

Backend::~Backend()
{
    if (m_process && busy() && m_privileged) {
        // A root process cannot be killed from here, and QProcess would wait for it forever.
        // Leave it: the helper keeps the operation running after our pipes close.
        m_process->disconnect(this);
        m_process->setParent(nullptr);
        m_process = nullptr;
    }
}

Backend::Invocation Backend::privilegedInvocation(const QString &verb,
                                                  const QStringList &args) const
{
    std::string error;
    if (!samurai::ops::build(verb.toStdString(), toStd(args), error)) {
        return {{}, {}, QString::fromStdString(error)};
    }
    QStringList argv{QStringLiteral("--disable-internal-agent"), m_programs.helper, verb};
    argv += args;
    return {m_programs.pkexec, argv, {}};
}

void Backend::query(const QStringList &args, const QJSValue &callback)
{
    capture({m_programs.cli, args, {}}, callback, kQueryTimeoutMs);
}

void Backend::privilegedQuery(const QString &verb, const QStringList &args,
                              const QJSValue &callback)
{
    // no timeout: an authentication dialog may be open, and the root process could not be killed
    capture(privilegedInvocation(verb, args), callback, 0);
}

void Backend::capture(const Invocation &invocation, const QJSValue &callback, int timeoutMs)
{
    const auto reply = [callback](int code, const QString &out, const QString &err) {
        QJSValue function = callback;
        if (function.isCallable()) {
            function.call({code, out, err});
        }
    };
    if (!invocation.error.isEmpty()) {
        QTimer::singleShot(0, this, [reply, error = invocation.error] {
            reply(kRefused, {}, error);
        });
        return;
    }
    auto *process = new QProcess(this);
    process->setStandardInputFile(QProcess::nullDevice());
    connect(process, &QProcess::finished, this,
            [process, reply](int code, QProcess::ExitStatus status) {
                reply(status == QProcess::NormalExit ? code : -1,
                      QString::fromUtf8(process->readAllStandardOutput()),
                      QString::fromUtf8(process->readAllStandardError()));
                process->deleteLater();
            });
    connect(process, &QProcess::errorOccurred, this,
            [process, reply](QProcess::ProcessError error) {
                if (error == QProcess::FailedToStart) {
                    reply(-1, {}, process->program() + QLatin1String(": ") + process->errorString());
                    process->deleteLater();
                }
            });
    if (timeoutMs > 0) {
        QTimer::singleShot(timeoutMs, process, &QProcess::kill);
    }
    process->start(invocation.program, invocation.arguments);
}

QString Backend::refusal(const QVariantMap &step) const
{
    const QString verb = step.value(QStringLiteral("verb")).toString();
    if (verb.isEmpty()) {
        return {};
    }
    return privilegedInvocation(verb, step.value(QStringLiteral("args")).toStringList()).error;
}

QString Backend::describe(const QVariantMap &step) const
{
    QStringList words;
    const QString verb = step.value(QStringLiteral("verb")).toString();
    if (verb.isEmpty()) {
        words << QStringLiteral("samurai") << step.value(QStringLiteral("cli")).toStringList();
    } else {
        std::string error;
        const auto argv = samurai::ops::build(
            verb.toStdString(), toStd(step.value(QStringLiteral("args")).toStringList()), error);
        if (!argv) {
            return QString::fromStdString(error);
        }
        words << QStringLiteral("sudo") << QStringLiteral("samurai");
        for (const auto &a : *argv) {
            words << QString::fromStdString(a);
        }
    }
    QStringList out;
    for (const auto &w : std::as_const(words)) {
        out << quoted(w);
    }
    return out.join(QLatin1Char(' '));
}

bool Backend::start(const QString &tag, const QVariantList &steps)
{
    if (busy() || steps.isEmpty()) {
        return false;
    }
    m_tag = tag;
    m_steps = steps;
    m_captured.clear();
    m_cancelled = false;
    m_log.clear();
    m_state = QStringLiteral("running");
    nextStep();
    return true;
}

void Backend::nextStep()
{
    m_step = m_steps.takeFirst().toMap();
    m_captured.clear();
    const QString verb = m_step.value(QStringLiteral("verb")).toString();
    m_privileged = !verb.isEmpty();
    m_title = m_step.value(QStringLiteral("title")).toString();
    m_command = describe(m_step);
    m_exitCode = 0;

    const Invocation invocation = m_privileged
        ? privilegedInvocation(verb, m_step.value(QStringLiteral("args")).toStringList())
        : Invocation{m_programs.cli, m_step.value(QStringLiteral("cli")).toStringList(), {}};
    m_log.append(QStringLiteral("$ ") + m_command);
    emit taskChanged();
    if (!invocation.error.isEmpty()) {
        m_log.append(invocation.error);
        end(QStringLiteral("failed"), kRefused);
        return;
    }

    const bool captured = m_step.value(QStringLiteral("capture")).toBool();
    m_process = new QProcess(this);
    m_process->setStandardInputFile(QProcess::nullDevice());
    m_process->setProcessChannelMode(captured ? QProcess::SeparateChannels
                                              : QProcess::MergedChannels);
    if (captured) {
        connect(m_process, &QProcess::readyReadStandardOutput, this, [this] {
            m_captured += m_process->readAllStandardOutput();
            if (m_captured.size() > kMaxCaptureBytes) {
                m_captured.truncate(kMaxCaptureBytes);
            }
        });
        connect(m_process, &QProcess::readyReadStandardError, this,
                [this] { m_log.feed(m_process->readAllStandardError()); });
    } else {
        connect(m_process, &QProcess::readyReadStandardOutput, this,
                [this] { m_log.feed(m_process->readAllStandardOutput()); });
    }
    connect(m_process, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        if (m_step.value(QStringLiteral("capture")).toBool()) {
            m_captured += m_process->readAllStandardOutput();
            m_log.feed(m_process->readAllStandardError());
        } else {
            m_log.feed(m_process->readAll());
        }
        m_log.flush();
        stepDone(status == QProcess::NormalExit ? code : -1);
    });
    connect(m_process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) {
            m_log.append(m_process->program() + QLatin1String(": ") + m_process->errorString());
            stepDone(-1);
        }
    });
    m_process->start(invocation.program, invocation.arguments);
}

void Backend::stepDone(int code)
{
    m_process->deleteLater();
    m_process = nullptr;
    if (m_cancelled) {
        end(QStringLiteral("cancelled"), code);
        return;
    }
    if (m_privileged && (code == kAuthDismissed || code == kNotAuthorized)) {
        m_log.append(code == kAuthDismissed
                         ? QStringLiteral("Authentication cancelled; nothing was changed.")
                         : QStringLiteral("Not authorised; nothing was changed."));
        end(QStringLiteral("denied"), code);
        return;
    }
    QVariantList ok = m_step.value(QStringLiteral("okCodes")).toList();
    if (ok.isEmpty()) {
        ok << 0;
    }
    bool success = false;
    for (const auto &c : std::as_const(ok)) {
        success = success || c.toInt() == code;
    }
    const QString saveTo = m_step.value(QStringLiteral("saveTo")).toString();
    if (success && !saveTo.isEmpty()) {
        QFile file(saveTo);
        if (file.open(QIODevice::WriteOnly | QIODevice::NewOnly,
                      QFileDevice::ReadOwner | QFileDevice::WriteOwner)
            && file.write(m_captured) == m_captured.size() && file.flush()) {
            m_log.append(QStringLiteral("Saved to ") + saveTo);
        } else {
            m_log.append(QStringLiteral("Cannot write ") + saveTo + QLatin1String(": ")
                         + file.errorString());
            success = false;
        }
    }
    if (!success) {
        m_log.append(QStringLiteral("Failed (exit code %1).").arg(code));
        end(QStringLiteral("failed"), code);
        return;
    }
    if (!m_steps.isEmpty()) {
        nextStep();
        return;
    }
    end(QStringLiteral("ok"), code);
}

void Backend::end(const QString &state, int code)
{
    m_steps.clear();
    m_state = state;
    m_exitCode = code;
    const QString tag = m_tag;
    const QString output = QString::fromUtf8(m_captured);
    emit taskChanged();
    emit finished(tag, code, output);
}

void Backend::cancel()
{
    if (!busy() || m_privileged || !m_process) {
        return;
    }
    m_cancelled = true;
    m_log.append(QStringLiteral("Cancelling..."));
    m_process->terminate();
    QTimer::singleShot(3000, m_process, &QProcess::kill);
}

void Backend::copy(const QString &text) const
{
    QGuiApplication::clipboard()->setText(text);
}

QString Backend::newBundlePath() const
{
    return QDir::home().filePath(
        QStringLiteral("samurai-diag-%1.txt")
            .arg(QDateTime::currentDateTime().toString(QStringLiteral("yyyyMMdd-HHmmss"))));
}

QString Backend::folderUrl(const QString &path) const
{
    return QUrl::fromLocalFile(QFileInfo(path).absolutePath()).toString();
}
