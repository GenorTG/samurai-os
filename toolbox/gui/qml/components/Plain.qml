// Every string that comes from the system is shown as plain text: package descriptions, journal
// lines and snapshot labels must never be interpreted as rich text.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

QQC2.Label {
    textFormat: Text.PlainText
    wrapMode: Text.Wrap
    Layout.fillWidth: true
}
