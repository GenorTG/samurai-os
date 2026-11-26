// A scrollable page: children stack vertically at a readable width.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: view
    default property alias content: column.data
    clip: true
    contentWidth: availableWidth
    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

    ColumnLayout {
        id: column
        width: Math.min(view.availableWidth - 16, 860)
        spacing: 10
    }
}
