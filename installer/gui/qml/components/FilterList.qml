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
        // exact name first, then names that start with it, then the rest ("pl" before
        // "apple-…")
        const exact = [], prefix = [], other = []
        for (const v of values) {
            const low = v.toLowerCase()
            const at = low.indexOf(needle)
            if (at < 0)
                continue
            if (low === needle)
                exact.push(v)
            else if (at === 0 || low[at - 1] === "/")
                prefix.push(v)
            else
                other.push(v)
        }
        return exact.concat(prefix, other)
    }
    function reveal() {
        const at = shown.indexOf(value)
        if (at >= 0)
            list.positionViewAtIndex(at, ListView.Center)
    }
    function focusSearch() { search.forceActiveFocus() }

    spacing: 6
    // a new search shows its best matches; without one the selection is centred
    onShownChanged: Qt.callLater(search.text === "" ? reveal : list.positionViewAtBeginning)

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
            Label {
                anchors.centerIn: parent
                visible: list.count === 0
                text: "Nothing matches."
            }
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
