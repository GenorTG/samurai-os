// One selectable row: a radio button or check box with an explanation under it.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: row
    property string label: ""
    property string text: ""
    property bool exclusive: false
    property bool checked: false
    signal toggled(bool on)

    Layout.fillWidth: true
    spacing: 0

    Loader {
        id: loader
        sourceComponent: row.exclusive ? radio : check
    }
    PlainLabel {
        Layout.fillWidth: true
        Layout.leftMargin: 30
        visible: row.text !== ""
        text: row.text
        opacity: 0.72
    }

    Component {
        id: radio
        RadioButton {
            text: row.label
            autoExclusive: false
            checked: row.checked
            // stays bound to the choice: a click only reports, the choice decides
            onClicked: {
                row.toggled(true)
                checked = Qt.binding(() => row.checked)
            }
        }
    }
    Component {
        id: check
        CheckBox {
            text: row.label
            checked: row.checked
            onClicked: {
                row.toggled(checked)
                checked = Qt.binding(() => row.checked)
            }
        }
    }
}
