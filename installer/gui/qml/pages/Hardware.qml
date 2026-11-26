import QtQuick
import QtQuick.Layouts
import "../components"

PageBody {
    Heading { text: "Detected graphics" }
    Repeater {
        model: window.facts.gpus
        delegate: Notice {
            required property var modelData
            title: modelData.device
            text: modelData.vendor + "  ·  PCI " + modelData.address
        }
    }
    PlainLabel {
        visible: window.facts.gpus.length === 0
        text: "No graphics controller was identified."
    }
    PlainLabel {
        Layout.fillWidth: true
        text: "Open drivers for AMD, Intel and virtual machines are always installed."
    }
    Heading { text: "NVIDIA driver" }
    Repeater {
        model: window.options.nvidia
        delegate: Choice {
            required property var modelData
            exclusive: true
            label: modelData.name
            text: modelData.text
            checked: installer.choices.nvidia === modelData.id
            onToggled: installer.choices.nvidia = modelData.id
        }
    }
}
