// The column every page puts its content in, with the shared loading and error states on top.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: body

    property bool loading: false
    property string errorText: ""

    spacing: Kirigami.Units.largeSpacing

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        type: Kirigami.MessageType.Error
        text: applicationWindow().escapeHtml(body.errorText)
        visible: body.errorText !== ""
    }

    QQC2.BusyIndicator {
        Layout.alignment: Qt.AlignHCenter
        running: visible
        visible: body.loading
    }
}
