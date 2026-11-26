import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

Kirigami.ScrollablePage {
    id: page

    property string version: ""
    property var updates: null
    property var report: null
    property var kernels: null
    property var health: null
    property int pending: 0
    property bool checking: false

    readonly property var win: applicationWindow()
    readonly property var issues: win.healthIssues(health)
    readonly property bool rebootNeeded: updates !== null && updates.reboot === 1
    readonly property bool checked: updates !== null && updates.checked !== undefined

    function read(args, done) {
        pending++
        Backend.query(args, (code, out, err) => {
            pending--
            done(out)
        })
    }
    function reload() {
        read(["version"], out => version = out.trim().replace(/^samurai /, ""))
        read(["status", "--json"], out => updates = win.parse(out))
        read(["status", "--report"], out => {
            const r = win.parse(out)
            report = r && r.stage !== undefined ? r : null
        })
        pending++
        win.readSystem("kernel-list", ["kernel", "list", "--json"], (code, out) => {
            pending--
            kernels = win.parse(out)
        })
        pending++
        win.readSystem("health-read", ["health", "--json"], (code, out) => {
            pending--
            health = win.parse(out)
        })
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

    title: "Overview"
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
        }
    }

    component Fact: RowLayout {
        property string name
        property string value
        Plain {
            text: parent.name
            opacity: 0.7
            Layout.fillWidth: false
            Layout.preferredWidth: Kirigami.Units.gridUnit * 8
            Layout.alignment: Qt.AlignTop
        }
        Plain {
            text: parent.value === "" ? "…" : parent.value
        }
    }

    PageBody {
        loading: page.pending > 0

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            text: "A restart is needed to finish the last update."
            visible: page.rebootNeeded
        }

        GridLayout {
            columns: page.width > Kirigami.Units.gridUnit * 36 ? 2 : 1
            columnSpacing: Kirigami.Units.largeSpacing
            rowSpacing: Kirigami.Units.largeSpacing
            uniformCellWidths: true

            Card {
                heading: "This system"
                Layout.fillHeight: true

                Fact {
                    name: "Samurai tools"
                    value: page.version.replace(" (samurai-tools)", "")
                }
                Fact {
                    name: "Running kernel"
                    value: page.kernels ? (page.kernels.running || "unknown (restart pending)") : ""
                }
                Fact {
                    name: "Default kernel"
                    value: page.kernels ? (page.kernels.default || "unknown") : ""
                }
                Fact {
                    name: "Last update"
                    value: page.report === null ? "not run yet"
                        : (page.report.ok ? "succeeded" : "failed at " + page.report.stage)
                }
                Item { Layout.fillHeight: true }
            }

            Card {
                heading: "Updates"
                Layout.fillHeight: true

                Plain {
                    text: !page.checked ? "No update check yet."
                        : page.updates.error === 1 ? "The last check failed."
                        : (page.updates.arch + page.updates.samurai) === 0 ? "Everything is up to date."
                        : (page.updates.arch + page.updates.samurai) + " update(s) available"
                    font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.2
                }
                Plain {
                    visible: page.checked
                    opacity: 0.7
                    text: !page.checked ? "" : "Arch " + page.updates.arch + " · Samurai "
                        + page.updates.samurai + " · security " + page.updates.security
                        + "\nChecked " + page.updates.checked
                }
                Item { Layout.fillHeight: true }
                RowLayout {
                    QQC2.Button {
                        text: page.checking ? "Checking…" : "Check now"
                        icon.name: "view-refresh"
                        enabled: !page.checking
                        onClicked: page.checkNow()
                    }
                    QQC2.Button {
                        text: "Open Updates"
                        icon.name: "system-software-update"
                        onClicked: page.win.currentPage = "updates"
                    }
                }
            }

            Card {
                heading: "Health"
                Layout.columnSpan: parent.columns

                Plain {
                    text: page.health === null ? "Checking…"
                        : page.issues.length === 0 ? "No problems found."
                        : page.issues.length + " finding(s) need a look"
                    font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.2
                }
                Repeater {
                    model: page.issues.slice(0, 5)

                    RowLayout {
                        required property var modelData
                        Badge {
                            text: modelData.severity
                            kind: page.win.severityKind(modelData.severity)
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                        }
                        Plain {
                            text: modelData.summary
                        }
                    }
                }
                Plain {
                    visible: page.issues.length > 5
                    opacity: 0.7
                    text: "… and " + (page.issues.length - 5) + " more"
                }
                QQC2.Button {
                    text: "Open Health"
                    icon.name: "security-high"
                    onClicked: page.win.currentPage = "health"
                }
            }
        }
    }
}
