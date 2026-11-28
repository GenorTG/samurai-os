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
    Heading { visible: installer.choices.nvidia !== "no"; text: "CUDA" }
    Choice {
        visible: installer.choices.nvidia !== "no"
        label: "CUDA toolkit"
        text: window.facts.gpus.some(g => g.cuda)
              ? "cuda, cudnn and nvtop: about 4 GB more to download."
              : "Needs an NVIDIA Turing (RTX 20) or newer card. None was found, so nothing is added."
        checked: installer.choices.cuda
        onToggled: on => installer.choices.cuda = on
    }
}
