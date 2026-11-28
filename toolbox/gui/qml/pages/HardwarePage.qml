import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

Kirigami.ScrollablePage {
    id: page

    property var report: null
    property string errorText: ""
    property bool loading: false

    readonly property var win: applicationWindow()
    readonly property var pending: report ? report.steps.filter(s => s.status !== "ok") : []
    readonly property bool switching: pending.some(s => s.status === "switch")
    // a branch change is the one refusal the "Switch driver" button answers
    readonly property var blockers: report
        ? report.refusals.filter(r => !(switching && r.indexOf("--switch-driver") !== -1)) : []

    function reload() {
        loading = true
        // detection only reads the machine: no administrator rights
        Backend.query(["hw", "detect", "--json"], (code, out, err) => {
            loading = false
            report = win.parse(out)
            errorText = report ? "" : "Cannot detect the hardware: " + err.trim()
        })
    }
    function statusText(status) {
        return { ok: "installed", extras: "optional packages missing", missing: "driver missing",
                 switch: "different driver installed", reinstall: "reinstall" }[status] || status
    }
    function statusKind(status) {
        return status === "ok" ? "positive" : status === "missing" ? "negative" : "warning"
    }
    function apply() {
        const lines = pending.map(s => s.profile + ": "
            + (s.remove.length > 0 ? "remove " + s.remove.join(" ") + "; " : "")
            + "install " + (s.install.join(" ") || "nothing"))
        win.confirmRun("hardware", {
            title: switching ? "Replace the graphics driver" : "Apply the driver profiles",
            message: "A snapshot is taken and every package is downloaded before anything changes. "
                + "If a step fails, the previous driver is put back. Restart afterwards."
                + "\n\n" + lines.join("\n"),
            confirmText: switching ? "Replace driver" : "Apply",
            destructive: switching,
            steps: [{ title: "Apply driver profiles",
                      verb: switching ? "hw-apply-switch" : "hw-apply", args: [] }]
        })
    }

    title: "Hardware"
    actions: Kirigami.Action {
        text: "Refresh"
        icon.name: "view-refresh"
        enabled: !page.loading
        onTriggered: page.reload()
    }
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
            type: Kirigami.MessageType.Warning
            visible: page.report !== null && page.report.hardware_changed
            text: page.report ? page.win.escapeHtml(page.report.hardware_message) : ""
            actions: Kirigami.Action {
                text: "Keep the current drivers"
                enabled: !Backend.busy
                onTriggered: page.win.confirmRun("hardware", {
                    title: "Accept the new hardware",
                    message: "The notice is cleared. No driver changes.",
                    confirmText: "Accept",
                    steps: [{ title: "Accept the new hardware", verb: "hw-accept", args: [] }]
                })
            }
        }

        Card {
            heading: "Devices"
            visible: page.report !== null

            Repeater {
                model: page.report ? page.report.devices : []

                ColumnLayout {
                    id: device
                    required property var modelData
                    spacing: 0

                    RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Plain {
                            text: device.modelData.name
                            font.bold: true
                        }
                        Badge {
                            visible: device.modelData.boot_display
                            text: "boot display"
                            kind: "info"
                        }
                        Badge {
                            visible: device.modelData.generation !== ""
                            text: device.modelData.cuda ? "CUDA capable" : "no CUDA toolkit"
                        }
                    }
                    Plain {
                        text: "PCI " + device.modelData.addr + "  ·  driver in use: "
                            + (device.modelData.driver || "none")
                            + (device.modelData.generation !== ""
                               ? "  ·  " + device.modelData.generation : "")
                        opacity: 0.7
                        font: Kirigami.Theme.smallFont
                    }
                }
            }
        }

        Card {
            heading: "Driver profiles"
            visible: page.report !== null

            Plain {
                visible: page.report !== null && page.report.steps.length === 0
                text: "No driver profile applies to this hardware."
            }
            Repeater {
                model: page.report ? page.report.steps : []

                ColumnLayout {
                    id: step
                    required property var modelData
                    spacing: 0

                    RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Plain {
                            text: step.modelData.profile
                            font.bold: true
                        }
                        Badge {
                            text: page.statusText(step.modelData.status)
                            kind: page.statusKind(step.modelData.status)
                        }
                    }
                    Plain {
                        text: step.modelData.description
                        opacity: 0.7
                        font: Kirigami.Theme.smallFont
                    }
                    Plain {
                        visible: step.modelData.status !== "ok" && step.modelData.install.length > 0
                        text: "Installs: " + step.modelData.install.join(" ")
                        font: Kirigami.Theme.smallFont
                    }
                    Plain {
                        visible: step.modelData.remove.length > 0
                        text: "Removes: " + step.modelData.remove.join(" ")
                        font: Kirigami.Theme.smallFont
                    }
                }
            }
            Repeater {
                model: page.report ? page.report.warnings : []
                Plain {
                    required property var modelData
                    text: "Warning: " + modelData
                }
            }
            Repeater {
                model: page.blockers
                Plain {
                    required property var modelData
                    text: "Not possible: " + modelData
                    color: Kirigami.Theme.negativeTextColor
                }
            }
            QQC2.Button {
                text: page.switching ? "Replace driver…" : "Apply…"
                icon.name: "dialog-ok-apply"
                visible: page.pending.length > 0
                enabled: !Backend.busy && page.blockers.length === 0
                onClicked: page.apply()
            }
            Plain {
                visible: page.report !== null && page.pending.length === 0
                    && page.report.steps.length > 0
                text: "The installed drivers match the hardware."
            }
        }
    }
}
