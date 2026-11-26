import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

Kirigami.ScrollablePage {
    id: page

    property var snapshots: []
    property bool loading: false
    property bool loaded: false
    property string errorText: ""

    readonly property var win: applicationWindow()

    function reload() {
        loading = true
        win.readSystem("snap-list", null, (code, out, err) => {
            loading = false
            const data = win.parse(out)
            loaded = data !== null
            errorText = data ? "" : "Cannot list snapshots: " + err.trim()
            // newest first
            snapshots = data ? data.snapshots.slice().reverse() : []
        })
    }

    title: "Snapshots"
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
            if (tag === "snap-create" && exitCode === 0) {
                label.clear()
            }
            page.reload()
        }
    }

    PageBody {
        loading: page.loading
        errorText: page.errorText

        Card {
            heading: "New snapshot"

            RowLayout {
                QQC2.TextField {
                    id: label
                    Layout.fillWidth: true
                    placeholderText: "Description, for example: before changing drivers"
                    maximumLength: 60
                    onAccepted: if (create.enabled) {
                        create.clicked()
                    }
                }
                QQC2.Button {
                    id: create
                    text: "Create"
                    icon.name: "document-save"
                    enabled: label.text.trim() !== "" && !label.text.trim().startsWith("-")
                             && !Backend.busy
                    onClicked: page.win.run("snap-create", [{
                        title: "Create a snapshot", verb: "snap-create", args: [label.text.trim()]
                    }])
                }
            }
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: page.loaded
            text: "To roll the system back, start the Samurai OS live USB and run: "
                + "samurai rescue rollback &lt;number&gt;"
        }

        Card {
            heading: page.snapshots.length + " snapshot(s)"
            visible: page.loaded

            Repeater {
                model: page.snapshots

                ColumnLayout {
                    id: snapshot
                    required property var modelData
                    required property int index

                    Kirigami.Separator {
                        visible: snapshot.index > 0
                        Layout.fillWidth: true
                    }
                    RowLayout {
                        spacing: Kirigami.Units.largeSpacing

                        Plain {
                            Layout.fillWidth: false
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 3
                            horizontalAlignment: Text.AlignRight
                            text: snapshot.modelData.id
                            font.bold: true
                        }
                        ColumnLayout {
                            spacing: 0
                            Plain {
                                text: snapshot.modelData.description || "(no description)"
                            }
                            Plain {
                                text: snapshot.modelData.date
                                opacity: 0.7
                                font: Kirigami.Theme.smallFont
                            }
                        }
                        Badge {
                            text: snapshot.modelData.type
                        }
                        QQC2.Button {
                            text: "Delete…"
                            icon.name: "edit-delete"
                            enabled: !Backend.busy
                            onClicked: page.win.confirmRun("snap-delete", {
                                title: "Delete snapshot " + snapshot.modelData.id + "?",
                                message: "You can no longer roll back to it.",
                                details: snapshot.modelData.date + "  " + snapshot.modelData.description,
                                confirmText: "Delete",
                                destructive: true,
                                steps: [{ title: "Delete snapshot " + snapshot.modelData.id,
                                          verb: "snap-delete",
                                          args: [String(snapshot.modelData.id)] }]
                            })
                        }
                    }
                }
            }
        }
    }
}
