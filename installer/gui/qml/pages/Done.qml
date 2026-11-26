import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

PageBody {
    id: page
    property string message: ""
    function activate() { restart.forceActiveFocus(Qt.TabFocusReason) }
    readonly property var steps: {
        installer.revision
        installer.state
        const c = installer.choices
        const out = ["Remove the installation medium, then restart."]
        if (c.encryption)
            out.push("Type your disk passphrase at start (keyboard layout “" + c.keyboard + "”).")
        out.push("Log in as “" + c.username + "”.")
        if (c.tailscale)
            out.push("Connect Tailscale:  sudo tailscale up")
        const deferred = installer.summary.deferred ?? []
        if (deferred.length > 0)
            out.push("Connect to a network and install what waited (also listed in /var/lib/samurai/firstboot-notes):\n"
                     + deferred.map(d => "    " + d.command).join("\n"))
        out.push("Optional: set up Secure Boot and TPM unlock later.")
        return out
    }

    Notice { kind: "ok"; title: "Samurai OS is installed."; text: "The new system is ready on " + installer.choices.disk + "." }
    Heading { text: "First steps" }
    Repeater {
        model: page.steps
        delegate: PlainLabel {
            required property string modelData
            required property int index
            Layout.fillWidth: true
            text: (index + 1) + ".  " + modelData
        }
    }
    RowLayout {
        Layout.topMargin: 16
        spacing: 10
        Button {
            id: restart
            text: "Restart now"
            icon.name: "system-reboot"
            highlighted: true
            onClicked: installer.reboot()
        }
        Button { text: "Save log"; icon.name: "document-save"; onClicked: page.message = installer.saveLog() }
        Button { text: "Close"; icon.name: "window-close"; onClicked: window.close() }
    }
    PlainLabel { Layout.fillWidth: true; text: page.message }
}
