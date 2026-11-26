// The install log: appends lines, follows the end unless the user scrolled up.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Frame {
    id: frame
    padding: 1
    function copyAll() {
        clipboard.text = installer.logText()
        clipboard.selectAll()
        clipboard.copy()
        clipboard.text = ""
    }
    TextEdit { id: clipboard; visible: false }
    ListModel { id: lines }
    Connections {
        target: installer
        function onLogAppended(line) {
            const follow = list.atYEnd
            lines.append({ line: line })
            if (lines.count > 4000)
                lines.remove(0, 500)
            if (follow)
                Qt.callLater(list.positionViewAtEnd)
        }
        function onStateChanged() {
            if (installer.state === "running")
                lines.clear()
        }
    }
    ListView {
        id: list
        anchors.fill: parent
        clip: true
        model: lines
        ScrollBar.vertical: ScrollBar {}
        delegate: Label {
            required property string line
            width: list.width - 14
            leftPadding: 6
            textFormat: Text.PlainText
            wrapMode: Text.WrapAnywhere
            font.family: "monospace"
            font.pointSize: 9
            text: line
        }
    }
}
