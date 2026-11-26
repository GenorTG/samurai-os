// The GUI's only way to the system: run the `samurai` CLI, or ask pkexec to run it through
// `samurai-gui-helper`. No system logic lives in the toolbox.
#pragma once

#include <QByteArray>
#include <QJSValue>
#include <QObject>
#include <QProcess>
#include <QString>
#include <QStringList>
#include <QVariant>

#include "logmodel.hpp"

class Backend : public QObject
{
    Q_OBJECT
    Q_PROPERTY(LogModel *log READ log CONSTANT)
    // the one long-running task; reads (`query`) are independent of it
    Q_PROPERTY(bool busy READ busy NOTIFY taskChanged)
    Q_PROPERTY(bool privileged READ privileged NOTIFY taskChanged)
    Q_PROPERTY(QString title READ title NOTIFY taskChanged)
    Q_PROPERTY(QString command READ command NOTIFY taskChanged)
    // idle | running | ok | failed | cancelled | denied
    Q_PROPERTY(QString state READ state NOTIFY taskChanged)
    Q_PROPERTY(int exitCode READ exitCode NOTIFY taskChanged)

public:
    struct Programs {
        QString cli = QStringLiteral("/usr/local/bin/samurai");
        QString pkexec = QStringLiteral("/usr/bin/pkexec");
        QString helper = QStringLiteral("/usr/lib/samurai/gui-helper");
    };

    explicit Backend(Programs programs, QObject *parent = nullptr);
    ~Backend() override;

    LogModel *log() { return &m_log; }
    bool busy() const { return m_state == QLatin1String("running"); }
    bool privileged() const { return m_privileged; }
    QString title() const { return m_title; }
    QString command() const { return m_command; }
    QString state() const { return m_state; }
    int exitCode() const { return m_exitCode; }

    /// `samurai <args>` as the user; `callback(exitCode, stdout, stderr)`.
    Q_INVOKABLE void query(const QStringList &args, const QJSValue &callback);
    /// A helper operation whose output the page consumes; same callback.
    Q_INVOKABLE void privilegedQuery(const QString &verb, const QStringList &args,
                                     const QJSValue &callback);
    /// Run steps one after another, streaming into `log`; false when a task is already running.
    /// A step is `{title, verb, args}` (privileged) or `{title, cli: [..]}` (as the user), with
    /// optional `capture` (keep stdout out of the log and return it), `saveTo` (write captured
    /// stdout to a new private file) and `okCodes` (exit codes that count as success).
    Q_INVOKABLE bool start(const QString &tag, const QVariantList &steps);
    /// Stop a task that runs as the user. Privileged operations cannot be interrupted.
    Q_INVOKABLE void cancel();
    /// Why the helper would refuse a step; empty when it is allowed.
    Q_INVOKABLE QString refusal(const QVariantMap &step) const;
    /// The terminal command equivalent to a step.
    Q_INVOKABLE QString describe(const QVariantMap &step) const;
    Q_INVOKABLE void copy(const QString &text) const;
    /// A new file name in the home directory for a diagnostics bundle.
    Q_INVOKABLE QString newBundlePath() const;
    Q_INVOKABLE QString folderUrl(const QString &path) const;

signals:
    void taskChanged();
    /// `output` is the captured stdout of the last step that ran.
    void finished(const QString &tag, int exitCode, const QString &output);

private:
    struct Invocation {
        QString program;
        QStringList arguments;
        QString error;  // non-empty: the request was refused before running anything
    };

    Invocation privilegedInvocation(const QString &verb, const QStringList &args) const;
    void capture(const Invocation &invocation, const QJSValue &callback, int timeoutMs);
    void nextStep();
    void stepDone(int code);
    void end(const QString &state, int code);

    Programs m_programs;
    LogModel m_log;
    QProcess *m_process = nullptr;
    QVariantList m_steps;
    QVariantMap m_step;
    QByteArray m_captured;
    QString m_tag;
    QString m_title;
    QString m_command;
    QString m_state = QStringLiteral("idle");
    int m_exitCode = 0;
    bool m_privileged = false;
    bool m_cancelled = false;
};
