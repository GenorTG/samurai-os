// Samurai Toolbox: the Qt/QML client of the `samurai` CLI (spec 01, ADR-0004).
#include <QApplication>
#include <QCommandLineParser>
#include <QIcon>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QQmlApplicationEngine>
#include <QQmlNetworkAccessManagerFactory>
#include <QQuickStyle>

#include <cstdio>
#include <unistd.h>

#include "backend.hpp"

#ifdef SAMURAI_GUI_DEVTOOLS
#include <QDir>
#include <QQuickWindow>
#include <QTimer>
#endif

namespace {

/// The toolbox has nothing to fetch. Text shown here comes from package metadata, journals and
/// snapshot labels, so the QML engine is not allowed to open any network request.
class NoNetwork : public QNetworkAccessManager
{
public:
    using QNetworkAccessManager::QNetworkAccessManager;

protected:
    QNetworkReply *createRequest(Operation op, const QNetworkRequest &, QIODevice *) override
    {
        return QNetworkAccessManager::createRequest(op, QNetworkRequest(QUrl()), nullptr);
    }
};

class NoNetworkFactory : public QQmlNetworkAccessManagerFactory
{
public:
    QNetworkAccessManager *create(QObject *parent) override { return new NoNetwork(parent); }
};

}  // namespace

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("samurai-toolbox"));
    QApplication::setApplicationDisplayName(QStringLiteral("Samurai Toolbox"));
    QApplication::setDesktopFileName(QStringLiteral("samurai-toolbox"));
    QApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("preferences-system")));
    if (qEnvironmentVariableIsEmpty("QT_QUICK_CONTROLS_STYLE")) {
        QQuickStyle::setStyle(QStringLiteral("org.kde.desktop"));
    }

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Graphical front end of the samurai command"));
    parser.addHelpOption();
    const QCommandLineOption page(
        QStringLiteral("page"),
        QStringLiteral("Open on a page: overview, health, apps, cleanup, kernels, updates, "
                       "snapshots, diagnostics, welcome."),
        QStringLiteral("name"), QStringLiteral("overview"));
    parser.addOption(page);
    Backend::Programs programs;
#ifdef SAMURAI_GUI_DEVTOOLS
    // test builds only (ci/test-gui.sh): a fake CLI and offscreen page screenshots
    const QCommandLineOption cli(QStringLiteral("cli"), {}, QStringLiteral("path"));
    const QCommandLineOption pkexec(QStringLiteral("pkexec"), {}, QStringLiteral("path"));
    const QCommandLineOption grab(QStringLiteral("grab"), {}, QStringLiteral("dir"));
    // switch to another page while the first one is still waiting for its answers
    const QCommandLineOption then(QStringLiteral("then"), {}, QStringLiteral("name"));
    parser.addOptions({cli, pkexec, grab, then});
#endif
    parser.process(app);

    if (geteuid() == 0) {
        std::fputs("samurai-toolbox: run it as your user; it asks for a password when needed\n",
                   stderr);
        return 1;
    }
#ifdef SAMURAI_GUI_DEVTOOLS
    if (parser.isSet(cli)) {
        programs.cli = parser.value(cli);
    }
    if (parser.isSet(pkexec)) {
        programs.pkexec = parser.value(pkexec);
    }
#endif

    Backend backend(programs);
    qmlRegisterSingletonInstance("org.samurai.backend", 1, 0, "Backend", &backend);

    NoNetworkFactory noNetwork;
    QQmlApplicationEngine engine;
    engine.setNetworkAccessManagerFactory(&noNetwork);
    engine.setInitialProperties({{QStringLiteral("currentPage"), parser.value(page)}});
    engine.loadFromModule("org.samurai.toolbox", "Main");
    if (engine.rootObjects().isEmpty()) {
        return 1;
    }

#ifdef SAMURAI_GUI_DEVTOOLS
    if (parser.isSet(grab)) {
        auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().constFirst());
        const QDir dir(parser.value(grab));
        QString name = parser.value(page);
        if (parser.isSet(then)) {
            const QString next = parser.value(then);
            name += QLatin1Char('-') + next;
            QTimer::singleShot(300, window, [window, next] {
                window->setProperty("currentPage", next);
            });
        }
        QTimer::singleShot(3500, &app, [window, dir, name] {
            const bool ok = window && window->grabWindow().save(dir.filePath(name + ".png"));
            QCoreApplication::exit(ok ? 0 : 1);
        });
    }
#endif
    return QApplication::exec();
}
