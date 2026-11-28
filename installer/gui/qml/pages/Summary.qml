import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

PageBody {
    id: page
    property string saveMessage: ""
    readonly property var disk: installer.summary.disk ?? ({})
    readonly property var rows: {
        installer.revision
        installer.summary
        const c = installer.choices
        const on = list => list.length ? list.join(", ") : "none"
        const gaming = c.gaming_enabled && c.desktop !== "none"
                       ? window.options.gaming.filter(g => c[g.id]).map(g => g.name).join(", ") : "off"
        return [
            ["Computer", c.hostname + "  ·  " + c.timezone + "  ·  " + c.locale + "  ·  keyboard " + c.keyboard],
            ["User", c.username + "  ·  shell " + c.shell + "  ·  root locked"],
            ["Kernels", c.kernels.join(", ") + "  (default: " + c.default_kernel + ")"],
            ["Bootloader", c.bootloader],
            ["Desktop", c.desktop === "kde" ? "KDE Plasma, Breeze Dark, widgets "
                        + (c.monitor_widgets ? "on" : "off") + ", reduced effects "
                        + (c.reduced_effects ? "on" : "off") : c.desktop],
            ["Graphics", "NVIDIA driver: " + c.nvidia + "  ·  CUDA "
                        + (c.cuda && window.facts.gpus.some(g => g.cuda)
                           && (c.nvidia === "yes" || (c.nvidia === "auto" && !window.facts.gpus.some(g => g.legacy)))
                           ? "on (about 4 GB)" : "off")],
            ["Gaming", gaming],
            ["Development", "Python: " + on(c.python) + "  ·  conda: " + c.conda + "  ·  Docker: " + c.docker
                            + "  ·  AI tools: " + on(c.ai_clis) + (c.ollama ? "  ·  Ollama" : "")],
            ["Browsers", c.desktop === "none" ? "none" : on(c.browsers)],
            ["Applications", on(c.apps)],
            ["Network", "Firewall: " + c.firewall + "  ·  Tailscale " + (c.tailscale ? "on" : "off")
                        + "  ·  SSH " + (c.ssh_server ? "on" : "off") + "  ·  proxy " + c.reverse_proxy],
        ].concat(c.arch_mirror === "auto" ? [] : [["Package mirror", c.arch_mirror + "  (from the loaded profile)"]])
    }

    Notice {
        kind: "danger"
        title: "ERASE " + (page.disk.path ?? "?") + "  —  " + (page.disk.size ?? "") + "  "
               + (page.disk.model ?? "")
        text: ((page.disk.partitions ?? []).length > 0
                   ? "These will be destroyed:\n" + page.disk.partitions.join("\n") + "\n\n" : "")
              + "New: 2 GiB EFI partition + " + (installer.choices.encryption ? "LUKS2-encrypted" : "UNENCRYPTED")
              + " btrfs system."
    }
    GridLayout {
        Layout.fillWidth: true
        columns: 2
        columnSpacing: 16
        rowSpacing: 4
        Repeater {
            model: page.rows.length * 2
            delegate: PlainLabel {
                required property int index
                readonly property bool isName: index % 2 === 0
                Layout.fillWidth: !isName
                Layout.alignment: Qt.AlignTop
                font.bold: isName
                text: page.rows[Math.floor(index / 2)][index % 2]
            }
        }
    }
    Notice {
        visible: (installer.summary.error ?? "") !== ""
        kind: "danger"
        title: "This setup cannot be installed"
        text: installer.summary.error ?? ""
    }
    Notice {
        visible: (installer.summary.deferred ?? []).length > 0
        kind: "warning"
        title: "Installs after the first boot (needs network)"
        text: (installer.summary.deferred ?? []).map(d => d.id + ":  " + d.command).join("\n")
    }
    CheckBox {
        id: details
        text: "Show all " + (installer.summary.count ?? 0) + " selected packages (plus the base system)"
    }
    Repeater {
        model: details.checked ? (installer.summary.groups ?? []) : []
        delegate: PlainLabel {
            required property var modelData
            Layout.fillWidth: true
            text: modelData.name + ": " + modelData.packages
            opacity: 0.8
        }
    }

    Heading { text: "Save this setup" }
    RowLayout {
        Layout.fillWidth: true
        TextField { id: path; Layout.fillWidth: true; text: installer.defaultProfilePath }
        Button {
            text: "Save profile"
            icon.name: "document-save"
            onClicked: page.saveMessage = installer.saveProfile(path.text)
        }
    }
    PlainLabel { Layout.fillWidth: true; visible: text !== ""; text: page.saveMessage }
    Item { Layout.preferredHeight: 8 }
}
