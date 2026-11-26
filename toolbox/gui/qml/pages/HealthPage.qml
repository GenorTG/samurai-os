import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

Kirigami.ScrollablePage {
    id: page

    property var report: null
    property bool loading: false
    property bool complete: true
    property bool showPassed: false
    property string errorText: ""

    readonly property var win: applicationWindow()
    readonly property var issues: win.healthIssues(report)
    readonly property var passed: report ? report.results.filter(r => r.status === "ok") : []
    readonly property var skipped: report ? report.results.filter(r => r.status === "skipped") : []
    readonly property var safeFixes: issues.filter(r => r.fixable && r.fix_risk === "safe")

    readonly property var repairs: [
        { verb: "fix-lock", title: "Remove a stale package lock", destructive: false,
          text: "Use when pacman reports a locked database after a crash. Refused while a package manager is running." },
        { verb: "fix-keyring", title: "Refresh signing keys", destructive: false,
          text: "Use when packages fail signature checks." },
        { verb: "fix-keyring-reset", title: "Reset the keyring", destructive: true,
          text: "Deletes the pacman keyring and builds it again. A snapshot is taken first." },
        { verb: "fix-dkms", title: "Rebuild kernel modules", destructive: false,
          text: "Rebuilds DKMS modules (for example NVIDIA) for the installed kernels." }
    ]

    function reload() {
        loading = true
        win.readSystem("health-read", ["health", "--json"], (code, out, err, asRoot) => {
            loading = false
            const parsed = win.parse(out)
            errorText = parsed ? "" : "The health check failed: " + err
            report = parsed
            complete = asRoot
        })
    }
    function repair(finding) {
        win.confirmRun("health", {
            title: "Repair " + finding.id,
            message: finding.summary + "\n\nA snapshot is taken before the repair"
                + (finding.fix_risk === "needs-confirm" ? ". This repair changes more than usual; read the details." : "."),
            details: [finding.details].concat(finding.evidence).filter(t => t !== "").join("\n"),
            confirmText: "Repair",
            destructive: finding.fix_risk !== "safe",
            steps: [{ title: "Repair " + finding.id, verb: "health-fix", args: [finding.id] }]
        })
    }

    title: "Health"
    actions: [
        Kirigami.Action {
            text: "Check again"
            icon.name: "view-refresh"
            enabled: !page.loading
            onTriggered: page.reload()
        },
        Kirigami.Action {
            text: "Apply safe repairs (" + page.safeFixes.length + ")"
            icon.name: "tools-wizard"
            enabled: page.safeFixes.length > 0 && !Backend.busy
            onTriggered: page.win.confirmRun("health", {
                title: "Apply the safe repairs",
                message: "A snapshot is taken first. Repairs that need a decision are left alone.",
                details: page.safeFixes.map(r => r.id + ": " + r.summary).join("\n"),
                confirmText: "Repair",
                steps: [{ title: "Safe repairs", verb: "health-fix-safe", args: [], okCodes: [0, 1] }]
            })
        }
    ]
    Component.onCompleted: reload()

    Connections {
        target: Backend
        function onFinished(tag, exitCode, output) {
            page.reload()
        }
    }

    PageBody {
        loading: page.loading
        errorText: page.errorText

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: !page.complete
            text: "Checked without administrator rights: boot checks were skipped."
        }

        Kirigami.Heading {
            level: 2
            visible: page.report !== null
            text: page.issues.length === 0 ? "No problems found"
                : page.issues.length + " finding(s)"
        }

        Repeater {
            model: page.issues

            Card {
                id: finding
                required property var modelData

                RowLayout {
                    spacing: Kirigami.Units.largeSpacing

                    Badge {
                        text: finding.modelData.severity
                        kind: page.win.severityKind(finding.modelData.severity)
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                        Layout.alignment: Qt.AlignTop
                    }
                    ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Plain {
                            text: finding.modelData.summary
                            font.bold: true
                        }
                        Plain {
                            text: finding.modelData.details
                            visible: text !== ""
                        }
                        Plain {
                            text: finding.modelData.evidence.slice(0, 8).join("\n")
                                + (finding.modelData.evidence.length > 8 ? "\n…" : "")
                            visible: finding.modelData.evidence.length > 0
                            font.family: "monospace"
                            font.pointSize: Kirigami.Theme.smallFont.pointSize
                            wrapMode: Text.WrapAnywhere
                            opacity: 0.8
                        }
                        Plain {
                            text: finding.modelData.id + (finding.modelData.fixable ? ""
                                : " · manual: follow the advice above")
                            opacity: 0.6
                            font: Kirigami.Theme.smallFont
                        }
                    }
                    QQC2.Button {
                        text: "Repair…"
                        icon.name: "tools-wizard"
                        visible: finding.modelData.fixable
                        enabled: !Backend.busy
                        Layout.alignment: Qt.AlignTop
                        onClicked: page.repair(finding.modelData)
                    }
                }
            }
        }

        Card {
            visible: page.report !== null
            heading: "Passed " + page.passed.length + " · skipped " + page.skipped.length

            QQC2.Button {
                text: page.showPassed ? "Hide" : "Show"
                icon.name: page.showPassed ? "arrow-up" : "arrow-down"
                onClicked: page.showPassed = !page.showPassed
            }
            Repeater {
                model: page.showPassed ? page.passed.concat(page.skipped) : []

                RowLayout {
                    required property var modelData
                    Badge {
                        text: modelData.status === "ok" ? "ok" : "skipped"
                        kind: modelData.status === "ok" ? "positive" : "neutral"
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                    }
                    Plain {
                        text: modelData.id + (modelData.summary ? " (" + modelData.summary + ")" : "")
                    }
                }
            }
        }

        Card {
            heading: "Repairs"

            Repeater {
                model: page.repairs

                RowLayout {
                    id: row
                    required property var modelData
                    spacing: Kirigami.Units.largeSpacing

                    ColumnLayout {
                        spacing: 0
                        Plain {
                            text: row.modelData.title
                            font.bold: true
                        }
                        Plain {
                            text: row.modelData.text
                            opacity: 0.7
                        }
                    }
                    QQC2.Button {
                        text: "Run…"
                        icon.name: row.modelData.destructive ? "dialog-warning" : "system-run"
                        enabled: !Backend.busy
                        onClicked: page.win.confirmRun("repair", {
                            title: row.modelData.title,
                            message: row.modelData.text,
                            confirmText: "Run",
                            destructive: row.modelData.destructive,
                            steps: [{ title: row.modelData.title, verb: row.modelData.verb, args: [] }]
                        })
                    }
                }
            }
        }
    }
}
