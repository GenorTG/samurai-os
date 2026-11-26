// Shown before every change: what happens, and the exact commands that will run.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

QQC2.Dialog {
    id: dialog

    property string message
    property string details
    property var steps: []
    property string confirmText: "Continue"
    property bool destructive: false
    property var action: null

    // {title, message, details, steps, confirmText, destructive, action}; no action: close only
    function ask(options) {
        title = options.title
        message = options.message || ""
        details = options.details || ""
        steps = options.steps || []
        confirmText = options.confirmText || "Continue"
        destructive = options.destructive || false
        action = options.action || null
        open()
    }

    parent: QQC2.Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 36)
    modal: true
    padding: Kirigami.Units.largeSpacing

    onAccepted: {
        const run = action
        action = null
        if (run) {
            run()
        }
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Plain {
            text: dialog.message
            visible: text !== ""
        }

        QQC2.ScrollView {
            id: detailScroll
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(detailText.implicitHeight,
                                             Kirigami.Units.gridUnit * 13)
            visible: dialog.details !== ""

            QQC2.TextArea {
                id: detailText
                text: dialog.details
                textFormat: TextEdit.PlainText
                readOnly: true
                wrapMode: TextEdit.Wrap
                font.family: "monospace"
                font.pointSize: Kirigami.Theme.smallFont.pointSize
            }
        }

        ColumnLayout {
            visible: dialog.steps.length > 0
            spacing: Kirigami.Units.smallSpacing

            Plain {
                text: dialog.steps.some(s => !!s.verb)
                    ? "This runs (you may be asked for your password):" : "This runs:"
                opacity: 0.7
            }

            Repeater {
                model: dialog.steps

                Plain {
                    required property var modelData
                    text: Backend.describe(modelData)
                    font.family: "monospace"
                    wrapMode: Text.WrapAnywhere
                }
            }
        }
    }

    footer: QQC2.DialogButtonBox {
        QQC2.Button {
            text: dialog.action ? "Cancel" : "Close"
            icon.name: "dialog-cancel"
            QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.RejectRole
        }
        QQC2.Button {
            text: dialog.confirmText
            icon.name: dialog.destructive ? "dialog-warning" : "dialog-ok-apply"
            visible: dialog.action !== null
            QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.AcceptRole
        }
    }
}
