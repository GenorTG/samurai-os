import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

ColumnLayout {
    id: page
    property string message: ""
    readonly property bool failed: installer.state === "failed"
    spacing: 10

    PlainLabel {
        Layout.fillWidth: true
        text: page.failed ? "The installation stopped" : installer.stage + "…"
        font.pointSize: 12
    }
    RowLayout {
        Layout.fillWidth: true
        Layout.rightMargin: 12
        ProgressBar {
            Layout.fillWidth: true
            from: 0
            to: 100
            value: installer.percent
        }
        Label { text: installer.percent + " %" }
    }
    Notice {
        Layout.rightMargin: 12
        visible: page.failed
        kind: "danger"
        title: "Installation failed"
        text: installer.failure + "\nThe live system was cleaned up. The log below says what happened."
    }
    LogView {
        id: log
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.rightMargin: 12
    }
    RowLayout {
        Layout.fillWidth: true
        Layout.rightMargin: 12
        Layout.bottomMargin: 10
        Button { text: "Copy log"; icon.name: "edit-copy"; onClicked: { log.copyAll(); page.message = "Log copied." } }
        Button { text: "Save log"; icon.name: "document-save"; onClicked: page.message = installer.saveLog() }
        PlainLabel { Layout.fillWidth: true; text: page.message }
        Button {
            visible: page.failed
            text: "Open rescue shell"
            icon.name: "utilities-terminal"
            onClicked: installer.openTerminal()
        }
        Button {
            visible: page.failed
            text: "Back to the summary"
            icon.name: "go-previous"
            onClicked: window.go(window.summaryIndex)
        }
    }
}
