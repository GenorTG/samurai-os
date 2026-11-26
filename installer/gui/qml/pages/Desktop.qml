import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

PageBody {
    Repeater {
        model: window.options.desktops
        delegate: Choice {
            required property var modelData
            exclusive: true
            label: modelData.name
            text: modelData.text
            checked: installer.choices.desktop === modelData.id
            onToggled: installer.choices.desktop = modelData.id
        }
    }
    ColumnLayout {
        Layout.fillWidth: true
        visible: installer.choices.desktop === "kde"
        spacing: 8
        Heading { text: "Theme" }
        Choice {
            exclusive: true
            label: "Breeze Dark"
            text: "KDE's own dark theme. More themes (Otto, Scratchy) are not packaged yet."
            checked: true
        }
        Heading { text: "Layout and effects" }
        Choice {
            label: "Monitor widgets in the top panel"
            text: "CPU and GPU usage, clock and temperature."
            checked: installer.choices.monitor_widgets
            onToggled: on => installer.choices.monitor_widgets = on
        }
        Choice {
            label: "Reduced effects"
            text: "No animations or blur: a faster desktop. Change it any time in System Settings."
            checked: installer.choices.reduced_effects
            onToggled: on => installer.choices.reduced_effects = on
        }
    }
    Notice {
        visible: installer.offlineInstall === false && window.facts.flavor === "offline"
        kind: "warning"
        text: "This desktop is not on the installation medium: the whole install needs a network."
    }
}
