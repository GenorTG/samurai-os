import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

Kirigami.ScrollablePage {
    id: page

    property var apps: []
    property var bundles: []
    property var categories: []
    property string category: ""
    property bool installedOnly: false
    property bool loading: false
    property bool planning: false
    property string errorText: ""
    // a fresh offline install has no package databases: nothing resolves until an update
    property bool noDatabases: false
    // answers to older requests are dropped
    property int request: 0

    readonly property var win: applicationWindow()
    readonly property var shown: apps.filter(a => (category === "" || a.category_name === category)
                                                  && (!installedOnly || a.installed))

    readonly property var statusLabels: ({
        installed: "installed", available: "available", partial: "partly available",
        unavailable: "unavailable", "not-yet": "not packaged yet", "needs-flatpak": "needs Flatpak",
        script: "vendor script", hidden: "not offered"
    })

    function reload() {
        const id = ++request
        const term = search.text.trim()
        const args = term === "" ? ["apps", "list", "--json"]
                                 : ["apps", "search", "--json", "--", term]
        loading = true
        Backend.query(args, (code, out, err) => {
            if (id !== request) {
                return
            }
            loading = false
            const data = win.parse(out)
            errorText = data ? "" : "Cannot read the application catalog: " + err
            noDatabases = data !== null && data.db_age_days === null
            const list = data ? data.apps : []
            // search results are ranked; the full list is grouped by category
            if (term === "") {
                list.sort((a, b) => a.category_name.localeCompare(b.category_name)
                                    || a.name.localeCompare(b.name))
                categories = [...new Set(list.map(a => a.category_name))]
            }
            apps = list
        })
    }
    function loadBundles() {
        Backend.query(["apps", "bundles", "--json"], (code, out) => {
            const data = win.parse(out)
            bundles = data ? data.bundles : []
        })
    }

    // preview with the tool's own dry run, then confirm
    function install(id, name, upgrade) {
        planning = true
        const args = ["apps", "install", "--dry-run", "--no-user-scripts"]
            .concat(upgrade ? ["--with-upgrade"] : [], ["--", id])
        Backend.query(args, (code, out, err) => {
            planning = false
            if (code === 4 && !upgrade) {
                // pending upgrades can be applied in the same transaction; a stale database cannot
                win.ask({
                    title: "The system needs an update first",
                    message: "Installing now would be a partial upgrade.",
                    details: err,
                    confirmText: "Update and install…",
                    action: () => Qt.callLater(page.install, id, name, true)
                })
            } else if (code === 4) {
                win.ask({
                    title: "Run a system update first",
                    message: "The package databases are too old to install safely.",
                    details: err,
                    confirmText: "Open Updates",
                    action: () => page.win.currentPage = "updates"
                })
            } else if (code !== 0) {
                win.inform("Cannot install " + name, "Nothing was changed.", (err + "\n" + out).trim())
            } else if (out.indexOf("(dry run: nothing changed)") < 0) {
                win.inform(name, "There is nothing to install.", out.trim())
            } else {
                win.confirmRun("apps", {
                    title: (upgrade ? "Update and install " : "Install ") + name,
                    details: out.replace("(dry run: nothing changed)", "").trim(),
                    confirmText: "Install",
                    steps: [{ title: "Install " + name,
                              verb: upgrade ? "apps-install-upgrade" : "apps-install", args: [id] }]
                })
            }
        })
    }
    function remove(id, name) {
        planning = true
        Backend.query(["apps", "remove", "--dry-run", "--", id], (code, out, err) => {
            planning = false
            if (code !== 0) {
                win.inform("Cannot remove " + name, "Nothing was changed.", (err + "\n" + out).trim())
            } else if (out.indexOf("(dry run: nothing changed)") < 0) {
                win.inform(name, "There is nothing to remove.", out.trim())
            } else {
                win.confirmRun("apps", {
                    title: "Remove " + name,
                    details: out.replace("(dry run: nothing changed)", "").trim(),
                    confirmText: "Remove",
                    destructive: true,
                    steps: [{ title: "Remove " + name, verb: "apps-remove", args: [id] }]
                })
            }
        })
    }

    title: "Applications"
    actions: Kirigami.Action {
        text: "Refresh"
        icon.name: "view-refresh"
        enabled: !page.loading
        onTriggered: {
            page.reload()
            page.loadBundles()
        }
    }
    Component.onCompleted: {
        reload()
        loadBundles()
    }

    Connections {
        target: Backend
        function onFinished(tag, exitCode, output) {
            page.reload()
            page.loadBundles()
        }
    }

    Timer {
        id: debounce
        interval: 300
        onTriggered: page.reload()
    }

    header: QQC2.ToolBar {
        contentItem: RowLayout {
            spacing: Kirigami.Units.largeSpacing

            Kirigami.SearchField {
                id: search
                Layout.fillWidth: true
                onTextChanged: debounce.restart()
            }
            QQC2.ComboBox {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                model: ["All categories"].concat(page.categories)
                onActivated: index => page.category = index === 0 ? "" : page.categories[index - 1]
            }
            QQC2.CheckBox {
                text: "Installed only"
                onToggled: page.installedOnly = checked
            }
            QQC2.BusyIndicator {
                running: visible
                visible: page.loading || page.planning
                Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                Layout.preferredHeight: Kirigami.Units.iconSizes.medium
            }
        }
    }

    ListView {
        id: list
        model: page.shown
        spacing: 0
        reuseItems: false

        header: ColumnLayout {
            width: list.width
            spacing: Kirigami.Units.smallSpacing

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.smallSpacing
                type: Kirigami.MessageType.Error
                text: page.win.escapeHtml(page.errorText)
                visible: page.errorText !== ""
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.smallSpacing
                type: Kirigami.MessageType.Warning
                text: "There are no package databases yet, so applications show as unavailable. "
                    + "Run a system update first."
                visible: page.noDatabases
                actions: Kirigami.Action {
                    text: "Open Updates"
                    icon.name: "system-software-update"
                    onTriggered: page.win.currentPage = "updates"
                }
            }
            Kirigami.Heading {
                level: 3
                text: "Bundles"
                visible: bundleFlow.visible
                Layout.margins: Kirigami.Units.largeSpacing
                Layout.bottomMargin: 0
            }
            Flow {
                id: bundleFlow
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                spacing: Kirigami.Units.smallSpacing
                visible: search.text === "" && page.category === "" && !page.installedOnly
                    && page.bundles.length > 0

                Repeater {
                    model: page.bundles

                    QQC2.Button {
                        required property var modelData
                        readonly property int done: modelData.members.filter(m => m.installed).length
                        text: modelData.name + "  " + done + "/" + modelData.members.length
                        icon.name: done === modelData.members.length ? "dialog-ok-apply" : "list-add"
                        enabled: !Backend.busy && !page.planning
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.text: modelData.description || ""
                        onClicked: page.install("@" + modelData.id, modelData.name, false)
                    }
                }
            }
        }

        delegate: QQC2.Control {
            id: row
            required property var modelData
            readonly property string note: modelData.reason && modelData.status !== "available"
                ? modelData.reason : ""

            width: list.width
            padding: Kirigami.Units.largeSpacing
            topPadding: Kirigami.Units.smallSpacing
            bottomPadding: Kirigami.Units.smallSpacing

            contentItem: RowLayout {
                spacing: Kirigami.Units.largeSpacing

                ColumnLayout {
                    spacing: 0
                    Plain {
                        text: row.modelData.name
                        font.bold: true
                    }
                    Plain {
                        text: [row.modelData.category_name,
                               row.modelData.packages.map(p => p.name).join(" ")
                               || row.modelData.flatpak || ""].filter(t => t !== "").join(" · ")
                        opacity: 0.7
                        font: Kirigami.Theme.smallFont
                        elide: Text.ElideRight
                        wrapMode: Text.NoWrap
                    }
                    Plain {
                        text: row.modelData.description || row.note
                        visible: text !== ""
                        opacity: 0.8
                        font: Kirigami.Theme.smallFont
                    }
                }
                Badge {
                    text: page.statusLabels[row.modelData.status] || row.modelData.status
                    kind: row.modelData.installed ? "positive"
                        : row.modelData.status === "available" ? "info"
                        : row.modelData.status === "script" ? "neutral" : "warning"
                }
                QQC2.Button {
                    readonly property bool script: row.modelData.status === "script"
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 9
                    text: row.modelData.installed ? "Remove…" : script ? "Copy command" : "Install…"
                    icon.name: row.modelData.installed ? "edit-delete"
                        : script ? "edit-copy" : "list-add"
                    enabled: script || (!Backend.busy && !page.planning
                        && (row.modelData.installed || row.modelData.installable))
                    onClicked: {
                        if (row.modelData.installed) {
                            page.remove(row.modelData.id, row.modelData.name)
                        } else if (script) {
                            Backend.copy("samurai apps install " + row.modelData.id)
                            page.win.inform(row.modelData.name,
                                "This app comes from a vendor script that runs as your user. "
                                + "The command is on the clipboard; run it in a terminal.",
                                (row.modelData.user_commands || []).join("\n"))
                        } else {
                            page.install(row.modelData.id, row.modelData.name, false)
                        }
                    }
                }
            }
        }

        QQC2.BusyIndicator {
            anchors.centerIn: parent
            running: visible
            visible: page.loading && page.shown.length === 0
        }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 4
            visible: !page.loading && page.shown.length === 0 && page.errorText === ""
            text: "No applications match"
        }
    }
}
