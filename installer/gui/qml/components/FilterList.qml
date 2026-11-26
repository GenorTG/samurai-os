// A searchable single-choice list (time zones, locales, keyboard layouts).
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: picker
    property string label: ""
    property var values: []
    property string value: ""
    signal picked(string value)

    readonly property var shown: {
        const needle = search.text.trim().toLowerCase().replace(/ /g, "_")
        if (needle === "")
            return values
        return values.filter(v => v.toLowerCase().indexOf(needle) >= 0)
    }
    function reveal() {
        const at = shown.indexOf(value)
        if (at >= 0)
            list.positionViewAtIndex(at, ListView.Center)
    }
    function focusSearch() { search.forceActiveFocus() }

    spacing: 6
    onShownChanged: Qt.callLater(reveal)

    PlainLabel { text: picker.label; font.bold: true }
    TextField {
        id: search
        Layout.fillWidth: true
        placeholderText: "Search…"
        // Enter takes the first match
        onAccepted: if (picker.shown.length > 0) picker.picked(picker.shown[0])
    }
    Frame {
        Layout.fillWidth: true
        Layout.fillHeight: true
        padding: 1
        ListView {
            id: list
            anchors.fill: parent
            clip: true
            model: picker.shown
            ScrollBar.vertical: ScrollBar {}
            delegate: ItemDelegate {
                required property string modelData
                width: list.width - 12
                text: modelData
                highlighted: modelData === picker.value
                onClicked: picker.picked(modelData)
            }
            Component.onCompleted: Qt.callLater(picker.reveal)
        }
    }
    PlainLabel { text: "Selected: " + picker.value; opacity: 0.75 }
}
