import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

PageBody {
    readonly property var aiClis: window.facts.apps.filter(a => a.category === "ai-cli")

    Heading { text: "Extra Python versions" }
    PlainLabel {
        Layout.fillWidth: true
        text: "Python 3.14 is the system Python. Other versions can also be fetched later with uv."
        opacity: 0.75
    }
    RowLayout {
        spacing: 18
        Repeater {
            model: window.options.pythons
            delegate: CheckBox {
                required property string modelData
                text: "Python " + modelData
                checked: installer.choices.python.indexOf(modelData) >= 0
                onClicked: {
                    installer.toggle("python", modelData, checked)
                    checked = Qt.binding(() => installer.choices.python.indexOf(modelData) >= 0)
                }
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 24
        ColumnLayout {
            Layout.alignment: Qt.AlignTop
            Layout.fillWidth: true
            Layout.preferredWidth: 10
            Heading { text: "Containers" }
            Repeater {
                model: window.options.docker
                delegate: Choice {
                    required property var modelData
                    exclusive: true
                    label: modelData.name
                    text: modelData.text
                    checked: installer.choices.docker === modelData.id
                    onToggled: installer.choices.docker = modelData.id
                }
            }
        }
        ColumnLayout {
            Layout.alignment: Qt.AlignTop
            Layout.fillWidth: true
            Layout.preferredWidth: 10
            Heading { text: "Conda" }
            Repeater {
                model: window.options.conda
                delegate: Choice {
                    required property var modelData
                    exclusive: true
                    label: modelData.name
                    text: modelData.text
                    checked: installer.choices.conda === modelData.id
                    onToggled: installer.choices.conda = modelData.id
                }
            }
            Heading { text: "Local AI" }
            Choice {
                label: "Ollama"
                text: installer.offlineInstall ? "Installs after the first boot (needs network)."
                                               : "Run language models on this computer."
                checked: installer.choices.ollama
                onToggled: on => installer.choices.ollama = on
            }
        }
    }

    Heading { text: "AI command-line tools" }
    PlainLabel {
        visible: installer.offlineInstall
        text: "These need a network: they install after the first boot."
        opacity: 0.75
    }
    GridLayout {
        Layout.fillWidth: true
        columns: 3
        Repeater {
            model: aiClis
            delegate: CheckBox {
                required property var modelData
                Layout.fillWidth: true
                Layout.preferredWidth: 10
                text: modelData.name
                checked: installer.choices.ai_clis.indexOf(modelData.id) >= 0
                onClicked: {
                    installer.toggle("ai_clis", modelData.id, checked)
                    checked = Qt.binding(() => installer.choices.ai_clis.indexOf(modelData.id) >= 0)
                }
            }
        }
    }
}
