import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

Kirigami.ScrollablePage {
    id: page

    property var targets: []
    property var selected: ({})
    property bool loading: false
    property string errorText: ""

    readonly property var win: applicationWindow()
    readonly property var chosen: targets.filter(t => selected[t.id] === true)
    readonly property real chosenBytes: chosen.reduce((sum, t) => sum + (t.bytes || 0), 0)

    readonly property var names: ({
        pkgcache: "Package cache", journal: "System journal", coredumps: "Crash dumps",
        thumbnails: "Thumbnails", orphans: "Unused packages", cache: "Your whole app cache"
    })

    function reload() {
        loading = true
        Backend.query(["clean", "--preview", "--json", "pkgcache", "journal", "coredumps",
                       "thumbnails", "orphans", "cache"], (code, out, err) => {
            loading = false
            const data = win.parse(out)
            errorText = data ? "" : "The preview failed: " + err
            targets = data ? data.targets : []
            // safe targets start ticked; risky ones never do
            const next = {}
            for (const t of targets) {
                next[t.id] = t.risk === "safe" && t.bytes !== 0
            }
            selected = next
        })
    }
    function select(id, on) {
        const next = Object.assign({}, selected)
        next[id] = on
        selected = next
    }
    function clean() {
        // user files are cleaned as the user; only system targets go through the helper
        const own = chosen.filter(t => !t.needs_root).map(t => t.id)
        const system = chosen.filter(t => t.needs_root).map(t => t.id)
        const steps = []
        if (own.length > 0) {
            steps.push({ title: "Clean your caches", cli: ["clean"].concat(own, ["--yes"]) })
        }
        if (system.length > 0) {
            steps.push({ title: "Clean system data", verb: "clean", args: system })
        }
        const lines = chosen.map(t => {
            let line = names[t.id] + ": " + (t.bytes === null ? "size unknown" : win.humanBytes(t.bytes))
            if (t.items.length > 0) {
                line += "\n  " + t.items.join("\n  ")
            }
            return line
        })
        win.confirmRun("clean", {
            title: "Delete the selected data?",
            message: "This cannot be undone.",
            details: lines.join("\n"),
            confirmText: "Delete",
            destructive: true,
            steps: steps
        })
    }

    title: "Cleanup"
    actions: [
        Kirigami.Action {
            text: "Measure again"
            icon.name: "view-refresh"
            enabled: !page.loading
            onTriggered: page.reload()
        },
        Kirigami.Action {
            text: "Clean selected…"
            icon.name: "edit-clear-history"
            enabled: page.chosen.length > 0 && !Backend.busy && !page.loading
            onTriggered: page.clean()
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

        Kirigami.Heading {
            level: 2
            visible: page.targets.length > 0
            text: page.chosen.length === 0 ? "Nothing selected"
                : "Selected: about " + page.win.humanBytes(page.chosenBytes)
        }

        Repeater {
            model: page.targets

            Card {
                id: target
                required property var modelData

                RowLayout {
                    spacing: Kirigami.Units.largeSpacing

                    QQC2.CheckBox {
                        checked: page.selected[target.modelData.id] === true
                        Layout.alignment: Qt.AlignTop
                        onToggled: page.select(target.modelData.id, checked)
                    }
                    ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Plain {
                            text: page.names[target.modelData.id] || target.modelData.id
                            font.bold: true
                        }
                        Plain {
                            text: target.modelData.description
                            opacity: 0.8
                        }
                        Plain {
                            text: target.modelData.note
                            visible: text !== ""
                            opacity: 0.6
                            font: Kirigami.Theme.smallFont
                        }
                        Plain {
                            text: target.modelData.items.slice(0, 12).join(", ")
                                + (target.modelData.items.length > 12
                                   ? " … (" + target.modelData.items.length + " in total)" : "")
                            visible: target.modelData.items.length > 0
                            font.family: "monospace"
                            font.pointSize: Kirigami.Theme.smallFont.pointSize
                        }
                    }
                    Badge {
                        visible: target.modelData.risk !== "safe"
                        text: "check first"
                        kind: "warning"
                        Layout.alignment: Qt.AlignTop
                    }
                    Plain {
                        Layout.fillWidth: false
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                        Layout.alignment: Qt.AlignTop
                        horizontalAlignment: Text.AlignRight
                        text: target.modelData.bytes !== null
                            ? page.win.humanBytes(target.modelData.bytes)
                            : target.modelData.id === "orphans"
                              ? target.modelData.items.length + " package(s)" : "—"
                    }
                }
            }
        }
    }
}
