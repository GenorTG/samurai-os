import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

PageBody {
    id: page
    readonly property var chosen: {
        const wanted = installer.choices.disk
        return installer.disks.find(d => d.path === wanted) ?? null
    }

    RowLayout {
        Layout.fillWidth: true
        PlainLabel { Layout.fillWidth: true; text: "Pick the disk to erase and install on." }
        Button { text: "Rescan"; icon.name: "view-refresh"; onClicked: installer.refreshDisks() }
    }

    Repeater {
        model: installer.disks
        delegate: Frame {
            id: card
            required property var modelData
            readonly property bool selected: installer.choices.disk === modelData.path
            Layout.fillWidth: true
            background: Rectangle {
                radius: 6
                color: card.selected ? Qt.rgba(0.85, 0.27, 0.33, 0.12) : "transparent"
                border.color: card.selected ? "#da4453" : palette.mid
                border.width: card.selected ? 2 : 1
            }
            ColumnLayout {
                anchors.fill: parent
                spacing: 2
                RadioButton {
                    autoExclusive: false
                    enabled: card.modelData.blocked === ""
                    checked: card.selected
                    text: card.modelData.path + "   —   " + card.modelData.size + "   —   "
                          + card.modelData.model + (card.modelData.removable ? "  (removable)" : "")
                    onClicked: {
                        installer.choices.disk = card.modelData.path
                        checked = Qt.binding(() => card.selected)
                    }
                }
                PlainLabel {
                    Layout.fillWidth: true
                    Layout.leftMargin: 30
                    text: card.modelData.partitions.length > 0
                          ? "On it now:\n" + card.modelData.partitions.join("\n")
                          : "No partitions or file systems found."
                    opacity: 0.8
                }
                PlainLabel {
                    Layout.fillWidth: true
                    Layout.leftMargin: 30
                    visible: text !== ""
                    text: card.modelData.blocked !== "" ? card.modelData.blocked
                          : card.modelData.small ? "Smaller than the recommended 40 GiB." : ""
                    color: "#f67400"
                }
            }
        }
    }
    Notice {
        visible: installer.disks.length === 0
        kind: "danger"
        title: "No disks found"
        text: "Attach a disk, then press Rescan."
    }

    Notice {
        visible: page.chosen !== null
        kind: "danger"
        title: page.chosen ? "Everything on " + page.chosen.path + " (" + page.chosen.size + ", "
                             + page.chosen.model + ") will be permanently erased." : ""
        text: "New layout: a 2 GiB boot (EFI) partition and one "
              + (installer.choices.encryption ? "encrypted " : "")
              + "btrfs partition with separate subvolumes for snapshots. "
              + "Nothing is written until you confirm on the Summary page."
    }

    Heading { text: "Encryption" }
    CheckBox {
        text: "Encrypt the system (LUKS2, recommended)"
        checked: installer.choices.encryption
        onClicked: {
            installer.choices.encryption = checked
            checked = Qt.binding(() => installer.choices.encryption)
        }
    }
    GridLayout {
        columns: 2
        columnSpacing: 12
        visible: installer.choices.encryption
        Label { text: "Passphrase" }
        TextField {
            id: pass
            Layout.preferredWidth: 360
            echoMode: reveal.checked ? TextInput.Normal : TextInput.Password
            inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText
            placeholderText: "At least 8 characters"
            onTextChanged: installer.setPassphrase(text, repeat.text)
        }
        Label { text: "Again" }
        TextField {
            id: repeat
            Layout.preferredWidth: 360
            echoMode: pass.echoMode
            inputMethodHints: pass.inputMethodHints
            onTextChanged: installer.setPassphrase(pass.text, text)
        }
        Item { width: 1 }
        CheckBox { id: reveal; text: "Show passphrase" }
    }
    Notice {
        visible: installer.choices.encryption
        kind: "warning"
        text: "You type this passphrase at every start, with the “" + installer.choices.keyboard
              + "” keyboard layout. If you lose it, your data cannot be recovered."
    }
    Notice {
        visible: !installer.choices.encryption
        kind: "warning"
        text: "Without encryption anyone with physical access to the disk can read your files."
    }
}
