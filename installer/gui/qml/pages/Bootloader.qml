import QtQuick
import QtQuick.Layouts
import "../components"

PageBody {
    Repeater {
        model: window.options.bootloaders
        delegate: Choice {
            required property var modelData
            exclusive: true
            label: modelData.name
            text: modelData.text
            checked: installer.choices.bootloader === modelData.id
            onToggled: installer.choices.bootloader = modelData.id
        }
    }
    Notice {
        text: "Secure Boot stays off for the installation. It can be set up afterwards."
    }
}
