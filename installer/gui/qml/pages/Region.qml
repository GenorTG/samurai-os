import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

ColumnLayout {
    spacing: 8
    function activate() { zones.focusSearch() }

    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.rightMargin: 12
        spacing: 16
        FilterList {
            id: zones
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 10
            label: "Time zone"
            values: window.facts.timezones
            value: installer.choices.timezone
            onPicked: picked => installer.choices.timezone = picked
        }
        FilterList {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 10
            label: "Language and formats"
            values: window.facts.locales
            value: installer.choices.locale
            onPicked: picked => installer.choices.locale = picked
        }
        FilterList {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 10
            label: "Keyboard layout"
            values: window.facts.keymaps
            value: installer.choices.keyboard
            onPicked: picked => {
                installer.choices.keyboard = picked
                installer.applyKeyboard(picked)
            }
        }
    }
    TextField {
        Layout.fillWidth: true
        Layout.rightMargin: 12
        Layout.bottomMargin: 8
        placeholderText: "Type here to test the keyboard layout (also try symbols you use in passwords)"
    }
}
