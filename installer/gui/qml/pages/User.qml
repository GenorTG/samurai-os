import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

PageBody {
    function activate() { name.forceActiveFocus() }

    GridLayout {
        columns: 2
        columnSpacing: 12
        Label { text: "User name" }
        TextField {
            id: name
            Layout.preferredWidth: 360
            maximumLength: 32
            inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
            placeholderText: "lowercase, e.g. alex"
            text: installer.choices.username
            onTextEdited: installer.choices.username = text
        }
        Label { text: "Password" }
        TextField {
            id: pass
            Layout.preferredWidth: 360
            echoMode: reveal.checked ? TextInput.Normal : TextInput.Password
            inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText
            placeholderText: "At least 6 characters"
            onTextChanged: installer.setPassword(text, repeat.text)
        }
        Label { text: "Password again" }
        TextField {
            id: repeat
            Layout.preferredWidth: 360
            echoMode: pass.echoMode
            inputMethodHints: pass.inputMethodHints
            onTextChanged: installer.setPassword(pass.text, text)
        }
        Item { width: 1 }
        CheckBox { id: reveal; text: "Show password" }
        Label { text: "Computer name" }
        TextField {
            Layout.preferredWidth: 360
            maximumLength: 63
            inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
            text: installer.choices.hostname
            onTextEdited: installer.choices.hostname = text
        }
    }
    Notice {
        text: "This account is the administrator (sudo). The root account stays locked."
    }

    Heading { text: "Terminal shell" }
    Repeater {
        model: window.options.shells
        delegate: Choice {
            required property var modelData
            exclusive: true
            label: modelData.name
            text: modelData.text
            checked: installer.choices.shell === modelData.id
            onToggled: installer.choices.shell = modelData.id
        }
    }
}
