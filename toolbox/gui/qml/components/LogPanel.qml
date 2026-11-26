// The console under every action: the running task's command, output and result.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

QQC2.Pane {
    id: panel

    property bool expanded: false
    readonly property var stateText: ({
        running: "Running", ok: "Finished", failed: "Failed",
        cancelled: "Cancelled", denied: "Not authorised", idle: ""
    })

    visible: Backend.state !== "idle"
    padding: Kirigami.Units.smallSpacing

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Kirigami.Separator {
            Layout.fillWidth: true
        }

        RowLayout {
            spacing: Kirigami.Units.smallSpacing

            QQC2.BusyIndicator {
                running: visible
                visible: Backend.busy
                Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                Layout.preferredHeight: Kirigami.Units.iconSizes.medium
            }
            Kirigami.Icon {
                visible: !Backend.busy
                source: Backend.state === "ok" ? "dialog-ok-apply"
                    : Backend.state === "failed" ? "dialog-error" : "dialog-cancel"
                Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium
            }
            Plain {
                text: Backend.title + " · " + panel.stateText[Backend.state]
                font.bold: true
                wrapMode: Text.NoWrap
                elide: Text.ElideRight
            }
            QQC2.Button {
                text: "Copy command"
                icon.name: "edit-copy"
                flat: true
                onClicked: Backend.copy(Backend.command)
            }
            QQC2.Button {
                text: "Copy log"
                icon.name: "edit-copy"
                flat: true
                onClicked: Backend.copy(Backend.log.text())
            }
            QQC2.Button {
                text: "Cancel"
                icon.name: "process-stop"
                visible: Backend.busy && !Backend.privileged
                onClicked: Backend.cancel()
            }
            QQC2.Button {
                text: panel.expanded ? "Hide log" : "Show log"
                icon.name: panel.expanded ? "arrow-down" : "arrow-up"
                flat: true
                onClicked: panel.expanded = !panel.expanded
            }
        }

        QQC2.Frame {
            visible: panel.expanded
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit * 11

            ListView {
                id: lines
                anchors.fill: parent
                clip: true
                model: Backend.log
                boundsBehavior: Flickable.StopAtBounds
                QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                onCountChanged: Qt.callLater(lines.positionViewAtEnd)
                onVisibleChanged: Qt.callLater(lines.positionViewAtEnd)

                delegate: QQC2.Label {
                    required property string line
                    width: lines.width - Kirigami.Units.gridUnit
                    text: line
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                    font.family: "monospace"
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                }
            }
        }
    }
}
