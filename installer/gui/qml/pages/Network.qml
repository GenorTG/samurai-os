import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

PageBody {
    RowLayout {
        Layout.fillWidth: true
        spacing: 24
        ColumnLayout {
            Layout.alignment: Qt.AlignTop
            Layout.fillWidth: true
            Layout.preferredWidth: 10
            Heading { text: "Firewall" }
            Repeater {
                model: window.options.firewalls
                delegate: Choice {
                    required property var modelData
                    exclusive: true
                    label: modelData.name
                    text: modelData.text
                    checked: installer.choices.firewall === modelData.id
                    onToggled: installer.choices.firewall = modelData.id
                }
            }
            Heading { text: "Remote access" }
            Choice {
                label: "Tailscale"
                text: "Private network between your devices. You sign in after the first boot."
                checked: installer.choices.tailscale
                onToggled: on => installer.choices.tailscale = on
            }
            Choice {
                label: "SSH server"
                text: "Key-only login. Off unless you need to reach this computer remotely."
                checked: installer.choices.ssh_server
                onToggled: on => installer.choices.ssh_server = on
            }
        }
        ColumnLayout {
            Layout.alignment: Qt.AlignTop
            Layout.fillWidth: true
            Layout.preferredWidth: 10
            Heading { text: "Reverse proxy (optional)" }
            Repeater {
                model: window.options.proxies
                delegate: Choice {
                    required property var modelData
                    exclusive: true
                    label: modelData.name
                    text: modelData.text
                    checked: installer.choices.reverse_proxy === modelData.id
                    onToggled: installer.choices.reverse_proxy = modelData.id
                }
            }
            Notice {
                visible: installer.choices.reverse_proxy !== "none"
                text: "Installed switched off, with no port opened. Serving the internet also needs "
                      + "ports 80 and 443 forwarded on your router."
            }
        }
    }
}
