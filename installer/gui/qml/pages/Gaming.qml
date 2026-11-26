import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

PageBody {
    Notice {
        visible: installer.choices.desktop === "none"
        kind: "warning"
        text: "Gaming needs a desktop: nothing from this page will be installed."
    }
    CheckBox {
        enabled: installer.choices.desktop !== "none"
        text: "Set this computer up for gaming"
        checked: installer.choices.gaming_enabled
        onClicked: {
            installer.choices.gaming_enabled = checked
            checked = Qt.binding(() => installer.choices.gaming_enabled)
        }
    }
    GridLayout {
        Layout.fillWidth: true
        columns: 2
        columnSpacing: 16
        enabled: installer.choices.gaming_enabled && installer.choices.desktop !== "none"
        Repeater {
            model: window.options.gaming
            delegate: Choice {
                required property var modelData
                Layout.preferredWidth: 10
                Layout.alignment: Qt.AlignTop
                label: modelData.name
                text: modelData.text
                checked: installer.choices[modelData.id]
                onToggled: on => installer.choices[modelData.id] = on
            }
        }
    }
}
