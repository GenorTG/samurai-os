import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.samurai.backend

Kirigami.ScrollablePage {
    id: page

    property string output: ""
    property string section: ""
    property string bundlePath: ""
    property string savedBundle: ""
    property bool loading: false

    readonly property var win: applicationWindow()
    readonly property var reports: [
        { id: "info", title: "System info", args: ["diag", "info"] },
        { id: "services", title: "Services", args: ["diag", "services"] },
        { id: "logs", title: "Error log", args: ["diag", "logs"] },
        { id: "pacman", title: "Package log", args: ["diag", "logs", "--pacman"] },
        { id: "disks", title: "Disks", args: ["diag", "disks"] },
        { id: "gpu", title: "Graphics", args: ["diag", "gpu"] }
    ]

    function show(report) {
        section = report.id
        loading = true
        Backend.query(report.args, (code, out, err) => {
            if (section !== report.id) {
                return
            }
            loading = false
            output = out + (code === 0 ? "" : "\n" + err)
        })
    }
    function bundle() {
        const path = Backend.newBundlePath()
        win.confirmRun("bundle", {
            title: "Create a support bundle",
            message: "Collects system information, logs and boot settings into one text file. User "
                + "names, addresses, serial numbers and secrets are replaced with placeholders. "
                + "Nothing is uploaded. Read the file before you share it.",
            details: path,
            confirmText: "Create",
            steps: [{ title: "Support bundle", verb: "diag-bundle", args: [], capture: true,
                      saveTo: path }]
        })
        bundlePath = path
    }

    title: "Diagnostics"
    Component.onCompleted: show(reports[0])

    Connections {
        target: Backend
        function onFinished(tag, exitCode, text) {
            if (tag === "bundle" && exitCode === 0) {
                page.savedBundle = page.bundlePath
            }
        }
    }

    PageBody {
        Card {
            heading: "Support bundle"

            Plain {
                text: "One redacted text file to attach to a bug report."
            }
            RowLayout {
                QQC2.Button {
                    text: "Create bundle…"
                    icon.name: "document-export"
                    enabled: !Backend.busy
                    onClicked: page.bundle()
                }
                QQC2.Button {
                    text: "Open folder"
                    icon.name: "document-open-folder"
                    visible: page.savedBundle !== ""
                    onClicked: Qt.openUrlExternally(Backend.folderUrl(page.savedBundle))
                }
            }
            Plain {
                visible: page.savedBundle !== ""
                text: "Saved: " + page.savedBundle
            }
        }

        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Repeater {
                model: page.reports

                QQC2.Button {
                    required property var modelData
                    text: modelData.title
                    checkable: true
                    autoExclusive: false
                    checked: page.section === modelData.id
                    onClicked: {
                        checked = Qt.binding(() => page.section === modelData.id)
                        page.show(modelData)
                    }
                }
            }
            QQC2.Button {
                text: "Copy"
                icon.name: "edit-copy"
                flat: true
                enabled: page.output !== "" && !page.loading
                onClicked: Backend.copy(page.output)
            }
        }

        QQC2.BusyIndicator {
            Layout.alignment: Qt.AlignHCenter
            running: visible
            visible: page.loading
        }

        QQC2.TextArea {
            Layout.fillWidth: true
            visible: !page.loading
            text: page.output
            textFormat: TextEdit.PlainText
            readOnly: true
            wrapMode: TextEdit.WrapAnywhere
            font.family: "monospace"
            font.pointSize: Kirigami.Theme.smallFont.pointSize
        }
    }
}
