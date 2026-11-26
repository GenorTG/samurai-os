import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

Kirigami.ApplicationWindow {
    id: app

    property string currentPage: "overview"
    property bool closeAnyway: false
    // `currentPage` may be set before the page stack exists
    property bool ready: false

    readonly property var pages: [
        { id: "overview", title: "Overview", icon: "go-home", file: "OverviewPage.qml" },
        { id: "health", title: "Health", icon: "security-high", file: "HealthPage.qml" },
        { id: "apps", title: "Applications", icon: "applications-all", file: "AppsPage.qml" },
        { id: "cleanup", title: "Cleanup", icon: "edit-clear-history", file: "CleanupPage.qml" },
        { id: "kernels", title: "Kernels & boot", icon: "cpu", file: "KernelsPage.qml" },
        { id: "updates", title: "Updates", icon: "system-software-update", file: "UpdatesPage.qml" },
        { id: "snapshots", title: "Snapshots", icon: "document-save-all", file: "SnapshotsPage.qml" },
        { id: "diagnostics", title: "Diagnostics", icon: "tools-report-bug", file: "DiagnosticsPage.qml" }
    ]

    // ------------------------------------------------------------ helpers shared by the pages
    function parse(text) {
        try {
            return JSON.parse(text)
        } catch (e) {
            return null
        }
    }
    function escapeHtml(text) {
        return String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
            .replace(/\n/g, "<br>")
    }
    function humanBytes(bytes) {
        if (bytes === null || bytes === undefined) {
            return "unknown size"
        }
        const units = ["B", "KiB", "MiB", "GiB", "TiB"]
        let value = bytes
        let unit = 0
        while (value >= 1024 && unit < units.length - 1) {
            value /= 1024
            unit++
        }
        return (unit === 0 || value >= 10 ? Math.round(value) : value.toFixed(1)) + " " + units[unit]
    }
    function severityRank(severity) {
        return ["info", "warning", "error", "critical"].indexOf(severity)
    }
    function severityKind(severity) {
        return severity === "info" ? "info" : severity === "warning" ? "warning" : "negative"
    }
    // health findings that are issues, worst first
    function healthIssues(report) {
        return report ? report.results.filter(r => r.status === "issue")
            .sort((a, b) => severityRank(b.severity) - severityRank(a.severity)) : []
    }
    // A read that is better as root and needs no password in the local session; when it is not
    // allowed (remote or inactive session) the unprivileged command answers instead.
    function readSystem(verb, fallback, callback) {
        Backend.privilegedQuery(verb, [], (code, out, err) => {
            if (fallback && (code === 126 || code === 127 || code === -1)) {
                Backend.query(fallback, (c, o, e) => callback(c, o, e, false))
            } else {
                callback(code, out, err, true)
            }
        })
    }
    function ask(options) {
        confirm.ask(options)
    }
    function inform(title, message, details) {
        confirm.ask({ title: title, message: message, details: details })
    }
    function run(tag, steps) {
        if (Backend.start(tag, steps)) {
            logPanel.expanded = true
        } else {
            showPassiveNotification("Another task is still running.")
        }
    }
    // confirm first, then run
    function confirmRun(tag, options) {
        const refused = options.steps.map(s => Backend.refusal(s)).filter(r => r !== "")
        if (refused.length > 0) {
            inform("Not possible from the toolbox",
                   "This value is not accepted here. If you are sure, use a terminal.",
                   refused.join("\n"))
            return
        }
        options.action = () => run(tag, options.steps)
        confirm.ask(options)
    }

    function showPage() {
        const wanted = pages.find(p => p.id === currentPage) || pages[0]
        pageStack.clear()
        pageStack.push(Qt.resolvedUrl(wanted.file))
    }

    title: "Samurai Toolbox"
    width: Kirigami.Units.gridUnit * 60
    height: Kirigami.Units.gridUnit * 38
    minimumWidth: Kirigami.Units.gridUnit * 40
    minimumHeight: Kirigami.Units.gridUnit * 28
    visible: true

    pageStack.globalToolBar.style: Kirigami.ApplicationHeaderStyle.ToolBar
    pageStack.globalToolBar.showNavigationButtons: Kirigami.ApplicationHeaderStyle.NoNavigationButtons

    onCurrentPageChanged: {
        if (ready) {
            showPage()
        }
    }
    Component.onCompleted: {
        ready = true
        showPage()
    }
    onClosing: close => {
        if (Backend.busy && !closeAnyway) {
            close.accepted = false
            confirm.ask({
                title: "A task is still running",
                message: Backend.privileged
                    ? "\"" + Backend.title + "\" runs as administrator. If you close the toolbox it "
                      + "keeps running in the background until it is done; do not power off."
                    : "Closing the toolbox stops \"" + Backend.title + "\".",
                confirmText: "Close anyway",
                destructive: true,
                action: () => {
                    Backend.cancel()
                    app.closeAnyway = true
                    app.close()
                }
            })
        }
    }

    globalDrawer: Kirigami.OverlayDrawer {
        edge: Qt.LeftEdge
        modal: false
        drawerOpen: true
        handleVisible: false
        interactive: false
        width: Kirigami.Units.gridUnit * 12
        leftPadding: 0
        rightPadding: 0

        contentItem: ListView {
            model: app.pages
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            delegate: QQC2.ItemDelegate {
                required property var modelData
                width: ListView.view.width
                text: modelData.title
                icon.name: modelData.icon
                highlighted: app.currentPage === modelData.id
                onClicked: app.currentPage = modelData.id
            }
        }
    }

    // Kirigami moves the footer left under the sidebar but keeps the page width: the wrapper
    // lets the panel span the whole window
    footer: Item {
        visible: logPanel.active
        implicitHeight: logPanel.active ? logPanel.implicitHeight : 0

        LogPanel {
            id: logPanel
            x: -parent.x
            width: app.width
        }
    }

    ConfirmDialog {
        id: confirm
    }
}
