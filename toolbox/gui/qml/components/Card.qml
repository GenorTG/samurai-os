import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.AbstractCard {
    id: card

    default property alias content: body.data
    property string heading

    Layout.fillWidth: true

    contentItem: ColumnLayout {
        id: body
        spacing: Kirigami.Units.smallSpacing

        Kirigami.Heading {
            level: 3
            text: card.heading
            textFormat: Text.PlainText
            visible: card.heading !== ""
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
    }
}
