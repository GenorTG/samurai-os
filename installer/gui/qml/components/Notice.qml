// A framed message. kind: "info" | "warning" | "danger" | "ok".
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: notice
    property string kind: "info"
    property string title: ""
    property string text: ""
    readonly property color accent: kind === "danger" ? "#da4453"
                                  : kind === "warning" ? "#f67400"
                                  : kind === "ok" ? "#27ae60" : "#3daee9"

    Layout.fillWidth: true
    implicitHeight: body.implicitHeight + 20
    radius: 6
    color: Qt.rgba(accent.r, accent.g, accent.b, 0.14)
    border.color: accent
    border.width: kind === "danger" ? 2 : 1

    ColumnLayout {
        id: body
        anchors.fill: parent
        anchors.margins: 10
        spacing: 4
        PlainLabel {
            Layout.fillWidth: true
            visible: notice.title !== ""
            text: notice.title
            font.bold: true
            color: notice.kind === "danger" ? notice.accent : palette.windowText
        }
        PlainLabel {
            Layout.fillWidth: true
            visible: notice.text !== ""
            text: notice.text
        }
    }
}
