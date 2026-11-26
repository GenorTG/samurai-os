import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

Kirigami.ScrollablePage {
    id: page

    property var kernels: null
    property var boot: null
    property string bootError: ""
    property string errorText: ""
    property int pending: 0

    readonly property var win: applicationWindow()

    function reload() {
        pending += 2
        win.readSystem("kernel-list", ["kernel", "list", "--json"], (code, out, err) => {
            pending--
            kernels = win.parse(out)
            errorText = kernels ? "" : "Cannot list kernels: " + err
        })
        win.readSystem("boot-get", null, (code, out, err) => {
            pending--
            boot = win.parse(out)
            bootError = boot ? "" : "Boot settings are not available: " + err.trim()
        })
    }
    function change(title, message, verb, args, destructive) {
        win.confirmRun("kernels", {
            title: title,
            message: message,
            confirmText: "Apply",
            destructive: destructive,
            steps: [{ title: title, verb: verb, args: args }]
        })
    }
    function install(kernel) {
        const warnings = kernel.install_warnings
        win.confirmRun("kernels", {
            title: "Install " + kernel.name,
            message: "Downloads the kernel and adds it to the boot menu."
                + (warnings.length > 0 ? "\n\nWarning: " + warnings.join("\n") : ""),
            confirmText: warnings.length > 0 ? "Install anyway" : "Install",
            destructive: warnings.length > 0,
            steps: [{ title: "Install " + kernel.name, args: [kernel.name],
                      verb: warnings.length > 0 ? "kernel-install-anyway" : "kernel-install" }]
        })
    }

    title: "Kernels & boot"
    actions: Kirigami.Action {
        text: "Refresh"
        icon.name: "view-refresh"
        enabled: page.pending === 0
        onTriggered: page.reload()
    }
    Component.onCompleted: reload()

    Connections {
        target: Backend
        function onFinished(tag, exitCode, output) {
            page.reload()
        }
    }

    PageBody {
        loading: page.pending > 0
        errorText: page.errorText

        Card {
            heading: "Kernels"
            visible: page.kernels !== null

            Repeater {
                model: page.kernels ? page.kernels.kernels : []

                ColumnLayout {
                    id: kernel
                    required property var modelData
                    required property int index

                    Kirigami.Separator {
                        visible: kernel.index > 0
                        Layout.fillWidth: true
                    }
                    RowLayout {
                        spacing: Kirigami.Units.smallSpacing

                        ColumnLayout {
                            spacing: 0
                            Plain {
                                text: kernel.modelData.name
                                font.bold: true
                            }
                            Plain {
                                text: kernel.modelData.description
                                opacity: 0.7
                                font: Kirigami.Theme.smallFont
                            }
                        }
                        Badge {
                            visible: kernel.modelData.running
                            text: "running"
                            kind: "positive"
                        }
                        Badge {
                            visible: kernel.modelData.default
                            text: "default"
                            kind: "info"
                        }
                        Badge {
                            visible: kernel.modelData.installed && !kernel.modelData.running
                            text: "installed"
                        }
                        Badge {
                            visible: kernel.modelData.installed && kernel.modelData.bootable === false
                            text: "not bootable"
                            kind: "negative"
                        }
                        QQC2.Button {
                            text: "Install…"
                            icon.name: "list-add"
                            visible: !kernel.modelData.installed
                            enabled: !Backend.busy
                            onClicked: page.install(kernel.modelData)
                        }
                        QQC2.Button {
                            text: "Make default…"
                            icon.name: "favorite"
                            visible: kernel.modelData.installed && !kernel.modelData.default
                                && kernel.modelData.bootable !== false
                                && page.boot !== null && page.boot.editable
                            enabled: !Backend.busy
                            onClicked: page.change("Boot " + kernel.modelData.name + " by default",
                                "Takes effect at the next restart.", "kernel-default",
                                [kernel.modelData.name], false)
                        }
                        QQC2.Button {
                            text: "Remove…"
                            icon.name: "edit-delete"
                            visible: kernel.modelData.installed
                            enabled: !Backend.busy && kernel.modelData.remove_blocker === null
                            hoverEnabled: true
                            onClicked: page.change("Remove " + kernel.modelData.name,
                                "The kernel and its boot entries are removed.", "kernel-remove",
                                [kernel.modelData.name], true)
                        }
                    }
                    Plain {
                        // why Remove is disabled
                        visible: kernel.modelData.installed && kernel.modelData.remove_blocker !== null
                        text: "Cannot remove: " + (kernel.modelData.remove_blocker || "")
                        opacity: 0.6
                        font: Kirigami.Theme.smallFont
                    }
                }
            }
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            visible: page.bootError !== ""
            text: page.win.escapeHtml(page.bootError)
        }

        Card {
            heading: "Boot settings"
            visible: page.boot !== null

            Plain {
                text: page.boot ? "Boot loader: " + page.boot.bootloader + " · default kernel: "
                    + (page.boot.default_kernel || "unknown") : ""
            }
            Plain {
                visible: page.boot !== null && !page.boot.editable
                text: "These settings are read-only for " + (page.boot ? page.boot.bootloader : "")
                    + ": edit /etc/default/grub and regenerate its configuration."
            }
            RowLayout {
                visible: page.boot !== null && page.boot.editable
                Plain {
                    Layout.fillWidth: false
                    text: "Menu timeout (seconds)"
                }
                QQC2.SpinBox {
                    id: timeout
                    from: 0
                    to: page.boot ? page.boot.max_timeout : 60
                    value: page.boot && page.boot.timeout !== null ? page.boot.timeout : 0
                    editable: true
                }
                QQC2.Button {
                    text: "Apply…"
                    icon.name: "dialog-ok-apply"
                    enabled: !Backend.busy && page.boot !== null && timeout.value !== page.boot.timeout
                    onClicked: page.change("Set the boot menu timeout",
                        "The boot menu waits " + timeout.value + " second(s).", "boot-timeout",
                        [String(timeout.value)], false)
                }
            }
            Kirigami.Heading {
                level: 4
                text: "Kernel parameters"
                visible: page.boot !== null && page.boot.editable
            }
            Flow {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                visible: page.boot !== null && page.boot.editable

                Repeater {
                    model: page.boot ? page.boot.parameters : []

                    QQC2.Button {
                        required property var modelData
                        text: modelData.value
                        font.family: "monospace"
                        icon.name: modelData.protected ? "object-locked" : "edit-delete-remove"
                        enabled: !modelData.protected && !Backend.busy
                        onClicked: page.change("Remove a kernel parameter",
                            "Removes " + modelData.value + " from the Samurai OS boot entries. "
                            + "Takes effect at the next restart.",
                            "boot-param-remove", [modelData.value], true)
                    }
                }
            }
            RowLayout {
                visible: page.boot !== null && page.boot.editable
                QQC2.TextField {
                    id: parameter
                    Layout.fillWidth: true
                    placeholderText: "New parameter, for example nowatchdog"
                    maximumLength: 200
                    validator: RegularExpressionValidator {
                        regularExpression: /[A-Za-z0-9._=,:\/+@][A-Za-z0-9._\-=,:\/+@]*/
                    }
                    onAccepted: if (add.enabled) {
                        add.clicked()
                    }
                }
                QQC2.Button {
                    id: add
                    text: "Add…"
                    icon.name: "list-add"
                    enabled: parameter.text !== "" && !Backend.busy
                    onClicked: page.change("Add a kernel parameter",
                        "Adds " + parameter.text + " to the Samurai OS boot entries. A wrong "
                        + "parameter can stop the system from booting; the previous menu is kept "
                        + "as a .bak file.", "boot-param-add", [parameter.text], true)
                }
            }
            Plain {
                visible: page.boot !== null && page.boot.editable
                opacity: 0.6
                font: Kirigami.Theme.smallFont
                text: "Locked parameters find or unlock the system disk. Parameters that start "
                    + "programs or a shell at boot cannot be added here; use a terminal."
            }
        }
    }
}
