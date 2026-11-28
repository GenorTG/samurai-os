// First steps on a new system: the list `samurai welcome --json` reports. This page only reads;
// anything that changes the disk or the network is a command the user runs in a terminal.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

Kirigami.ScrollablePage {
    id: page

    property var report: null
    property bool loading: true
    property string errorText: ""

    readonly property var win: applicationWindow()

    function reload() {
        loading = true
        Backend.query(["welcome", "--json"], (code, out, err) => {
            loading = false
            report = win.parse(out)
            errorText = report ? "" : "Could not read the first steps. " + err
        })
    }

    title: "Welcome"
    Component.onCompleted: reload()

    actions: [
        Kirigami.Action {
            text: "Release notes"
            icon.name: "help-about"
            enabled: page.report !== null
            onTriggered: Qt.openUrlExternally(page.report.release_notes)
        },
        Kirigami.Action {
            text: "Refresh"
            icon.name: "view-refresh"
            onTriggered: page.reload()
        }
    ]

    PageBody {
        loading: page.loading
        errorText: page.errorText

        Plain {
            text: "Welcome to Samurai OS. These are the first things worth doing; none of them is "
                + "required, and you can open this page again at any time."
            visible: page.report !== null
        }

        Repeater {
            model: page.report ? page.report.steps : []

            delegate: Card {
                id: card

                required property var modelData

                heading: (modelData.done ? "✓ " : "") + modelData.title

                Plain {
                    text: card.modelData.summary
                    visible: text !== ""
                }
                Plain {
                    text: card.modelData.command
                    visible: text !== ""
                    font.family: "monospace"
                    wrapMode: Text.WrapAnywhere
                }
                RowLayout {
                    spacing: Kirigami.Units.smallSpacing
                    visible: card.modelData.command !== "" || card.modelData.page !== ""

                    QQC2.Button {
                        text: "Open"
                        icon.name: "go-next"
                        visible: card.modelData.page !== ""
                        onClicked: page.win.currentPage = card.modelData.page
                    }
                    QQC2.Button {
                        text: "Copy command"
                        icon.name: "edit-copy"
                        visible: card.modelData.command !== ""
                        onClicked: {
                            Backend.copy(card.modelData.command)
                            page.win.showPassiveNotification("Copied. Paste it into a terminal.")
                        }
                    }
                }
            }
        }
    }
}
