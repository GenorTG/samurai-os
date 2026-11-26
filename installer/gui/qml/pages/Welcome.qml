import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import "../components"

PageBody {
    PlainLabel {
        Layout.fillWidth: true
        text: "This guide asks a few questions, shows you exactly what will happen, and only then "
              + "touches your disk. The recommended answers are already filled in."
    }
    Heading { text: "This computer" }
    Notice {
        kind: window.facts.uefi ? "ok" : "danger"
        title: window.facts.uefi ? "UEFI firmware" : "Legacy BIOS boot: not supported"
        text: window.facts.uefi ? "" : "Switch the firmware to UEFI mode and boot this medium again."
    }
    Notice {
        kind: window.facts.memory >= 7500 ? "ok" : window.facts.memory >= 3500 ? "warning" : "danger"
        title: "Memory: " + (window.facts.memory / 1024).toFixed(1) + " GiB"
        text: window.facts.memory >= 7500 ? ""
              : "4 GiB is the minimum; 8 GiB is recommended for KDE with gaming."
    }
    Notice {
        kind: installer.online ? "ok" : "warning"
        title: installer.online ? "Network connected" : "No network connection"
        text: installer.online ? ""
              : window.facts.flavor === "offline"
                ? "The default KDE system installs without network. Extra apps wait for the first boot."
                : "This is the online edition: connect with the network icon in the panel."
    }
    Notice {
        kind: "info"
        title: "Before you continue"
        text: "Plug a laptop into AC power. The chosen disk is erased completely (installing beside "
              + "another system is not supported yet): back up anything you want to keep. "
              + "At least 40 GiB of disk space is recommended."
    }

    Heading { text: "Reuse an earlier setup" }
    RowLayout {
        spacing: 10
        Button {
            text: "Load a saved profile…"
            icon.name: "document-open"
            onClicked: dialog.open()
        }
        PlainLabel { Layout.fillWidth: true; text: window.welcomeMessage }
    }
    FileDialog {
        id: dialog
        title: "Load install profile"
        nameFilters: ["Install profiles (*.toml)"]
        onAccepted: window.welcomeMessage = installer.loadProfile(decodeURIComponent(selectedFile.toString()))
    }
}
