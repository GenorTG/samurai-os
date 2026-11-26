import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

Kirigami.ScrollablePage {
    id: page

    property var updates: null
    property var report: null
    property var mirrors: null
    property bool checking: false
    property int pending: 0

    readonly property var win: applicationWindow()
    readonly property bool checked: updates !== null && updates.checked !== undefined

    function read(args, done) {
        pending++
        Backend.query(args, (code, out) => {
            pending--
            done(win.parse(out))
        })
    }
    function reload() {
        read(["status", "--json"], data => updates = data)
        read(["status", "--report"], data => report = data && data.stage !== undefined ? data : null)
        read(["mirrors", "status", "--json"], data => mirrors = data)
    }
    function checkNow() {
        checking = true
        Backend.query(["status", "--check", "--json"], (code, out, err) => {
            checking = false
            const state = win.parse(out)
            if (state) {
                updates = state
            }
            if (code !== 0) {
                win.inform("Update check failed", "The update check did not finish.", err)
            }
        })
    }
    function update(acknowledged) {
        win.confirmRun("update", {
            title: acknowledged ? "Update although news ask for manual steps?" : "Update the system?",
            message: acknowledged
                ? "Only continue if you have read the news items and done what they ask."
                : "A snapshot is taken, then every package is upgraded. Keep the computer on. "
                  + "Pacman's default answer is used for its questions.",
            confirmText: "Update",
            destructive: acknowledged,
            // 2: updated, restart needed
            steps: [{ title: "System update", args: [], capture: true, okCodes: [0, 2],
                      verb: acknowledged ? "update-ack-news" : "update" }]
        })
    }

    title: "Updates"
    actions: Kirigami.Action {
        text: "Refresh"
        icon.name: "view-refresh"
        enabled: page.pending === 0
        onTriggered: page.reload()
    }
    Component.onCompleted: reload()

    Connections {
        target: Backend
        function onFinished(tag, exitCode, output) {
            page.reload()
            if (tag === "update") {
                const result = page.win.parse(output)
                if (result) {
                    page.report = result
                }
                // the old check result is stale now
                page.checkNow()
            }
        }
    }

    PageBody {
        loading: page.pending > 0

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            visible: page.report !== null && page.report.reboot_needed === true
                     && page.report.ok === true
            text: page.report && page.report.reboot_reasons
                ? "Restart to finish the update (" + page.win.escapeHtml(
                      page.report.reboot_reasons.join(", ")) + ")."
                : "Restart to finish the update."
        }

        Card {
            heading: "System update"

            Plain {
                text: !page.checked ? "No update check yet."
                    : page.updates.error === 1 ? "The last check failed (is the network up?)."
                    : (page.updates.arch + page.updates.samurai) === 0 ? "Everything is up to date."
                    : (page.updates.arch + page.updates.samurai) + " update(s) available"
                font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.2
            }
            Plain {
                visible: page.checked
                opacity: 0.7
                text: !page.checked ? "" : "Arch " + page.updates.arch + " · Samurai "
                    + page.updates.samurai + " · security fixes " + page.updates.security
                    + " · .pacnew files " + page.updates.pacnew
                    + " · days since the last update " + page.updates.stale_days
                    + "\nChecked " + page.updates.checked
            }
            RowLayout {
                QQC2.Button {
                    text: page.checking ? "Checking…" : "Check now"
                    icon.name: "view-refresh"
                    enabled: !page.checking && !Backend.busy
                    onClicked: page.checkNow()
                }
                QQC2.Button {
                    text: "Update system…"
                    icon.name: "system-software-update"
                    highlighted: true
                    enabled: !Backend.busy
                    onClicked: page.update(false)
                }
            }
        }

        Card {
            heading: "Last update"
            visible: page.report !== null

            RowLayout {
                Badge {
                    text: page.report && page.report.ok ? "succeeded" : "did not finish"
                    kind: page.report && page.report.ok ? "positive" : "negative"
                }
                Plain {
                    text: !page.report || !page.report.packages ? ""
                        : page.report.packages.upgraded + " upgraded · "
                          + page.report.packages.installed + " installed · "
                          + page.report.packages.removed + " removed"
                }
            }
            Plain {
                visible: text !== ""
                text: page.report && page.report.error
                    ? "Stopped at stage " + page.report.stage + ": " + page.report.error : ""
            }
            Plain {
                visible: text !== ""
                text: page.report && page.report.warnings
                    ? page.report.warnings.map(w => "• " + w).join("\n") : ""
            }
            Repeater {
                model: page.report && page.report.news_pending ? page.report.news_pending : []

                Plain {
                    required property var modelData
                    text: "News needing manual steps: " + modelData.title + "\n" + modelData.url
                }
            }
            QQC2.Button {
                visible: page.report !== null && page.report.exit_code === 3
                text: "I have read these: update…"
                icon.name: "dialog-warning"
                enabled: !Backend.busy
                onClicked: page.update(true)
            }
            Plain {
                visible: page.report !== null && !!page.report.log
                opacity: 0.6
                font: Kirigami.Theme.smallFont
                text: page.report && page.report.log ? "Log: " + page.report.log : ""
            }
        }

        Card {
            heading: "Arch mirrors"
            visible: page.mirrors !== null

            Plain {
                text: !page.mirrors ? "" : page.mirrors.servers.length + " server(s)"
                    + (page.mirrors.age_days === null ? ""
                       : " · list is " + page.mirrors.age_days + " day(s) old")
            }
            Plain {
                text: !page.mirrors ? "" : page.mirrors.servers.slice(0, 5).join("\n")
                font.family: "monospace"
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                wrapMode: Text.WrapAnywhere
                opacity: 0.8
            }
            QQC2.Button {
                text: "Refresh mirrors…"
                icon.name: "network-server"
                enabled: !Backend.busy
                onClicked: page.win.confirmRun("mirrors", {
                    title: "Refresh the mirror list?",
                    message: "Replaces the Arch mirror list with the fastest recently synced "
                        + "mirrors. The old list is kept as mirrorlist.bak.",
                    confirmText: "Refresh",
                    steps: [{ title: "Refresh mirrors", verb: "mirrors-refresh", args: [] }]
                })
            }
        }
    }
}
