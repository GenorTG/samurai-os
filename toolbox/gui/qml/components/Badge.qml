import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

Rectangle {
    id: badge

    property string text
    // positive | warning | negative | info | neutral
    property string kind: "neutral"
    readonly property color tone: kind === "positive" ? Kirigami.Theme.positiveTextColor
        : kind === "warning" ? Kirigami.Theme.neutralTextColor
        : kind === "negative" ? Kirigami.Theme.negativeTextColor
        : kind === "info" ? Kirigami.Theme.highlightColor
        : Kirigami.Theme.disabledTextColor

    implicitWidth: label.implicitWidth + Kirigami.Units.largeSpacing * 2
    implicitHeight: label.implicitHeight + Kirigami.Units.smallSpacing
    radius: height / 2
    color: Qt.alpha(tone, 0.15)
    border.color: Qt.alpha(tone, 0.7)

    QQC2.Label {
        id: label
        anchors.centerIn: parent
        text: badge.text
        textFormat: Text.PlainText
        font: Kirigami.Theme.smallFont
    }
}
