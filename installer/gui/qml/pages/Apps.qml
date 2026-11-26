import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

ColumnLayout {
    id: page
    spacing: 8
    function activate() { search.forceActiveFocus() }
    function listKey(app) { return app.category === "browsers" ? "browsers" : "apps" }
    function isOn(app) { return installer.choices[listKey(app)].indexOf(app.id) >= 0 }

    readonly property var catalog: window.facts.apps.filter(a => a.category !== "ai-cli")
    readonly property var shown: {
        const needle = search.text.trim().toLowerCase()
        return catalog.filter(a => {
            if (onlySelected.checked && !isOn(a))
                return false
            return needle === "" || (a.name + " " + a.id + " " + a.category_name + " " + a.description)
                       .toLowerCase().indexOf(needle) >= 0
        })
    }
    readonly property int selected: installer.choices.apps.length + installer.choices.browsers.length

    RowLayout {
        Layout.fillWidth: true
        Layout.rightMargin: 12
        TextField {
            id: search
            Layout.fillWidth: true
            placeholderText: "Search " + page.catalog.length + " applications…"
        }
        CheckBox { id: onlySelected; text: "Selected only" }
        Label { text: "Selected: " + page.selected }
    }
    Notice {
        Layout.rightMargin: 12
        visible: window.facts.catalogError !== ""
        kind: "danger"
        title: "The application list could not be read"
        text: window.facts.catalogError
    }
    PlainLabel {
        Layout.fillWidth: true
        visible: installer.offlineInstall
        text: "Offline install: everything except Firefox installs after the first boot."
        opacity: 0.75
    }
    PlainLabel {
        visible: installer.choices.desktop === "none"
        text: "Without a desktop, browsers are not installed."
        color: "#f67400"
    }
    Frame {
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.rightMargin: 12
        Layout.bottomMargin: 8
        padding: 1
        ListView {
            id: list
            anchors.fill: parent
            clip: true
            model: page.shown
            reuseItems: false
            ScrollBar.vertical: ScrollBar {}
            // keep the scroll position when a tick rebuilds the "selected only" view
            delegate: ColumnLayout {
                id: row
                required property var modelData
                required property int index
                width: list.width - 14
                spacing: 0
                PlainLabel {
                    visible: row.index === 0
                             || page.shown[row.index - 1].category !== row.modelData.category
                    text: row.modelData.category_name
                    font.bold: true
                    leftPadding: 10
                    topPadding: 10
                    bottomPadding: 4
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    CheckBox {
                        text: row.modelData.name
                        Layout.preferredWidth: 280
                        enabled: !(row.modelData.category === "browsers"
                                   && installer.choices.desktop === "none")
                        checked: page.isOn(row.modelData)
                        onClicked: {
                            installer.toggle(page.listKey(row.modelData), row.modelData.id, checked)
                            checked = Qt.binding(() => page.isOn(row.modelData))
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        textFormat: Text.PlainText
                        text: row.modelData.description
                        elide: Text.ElideRight
                        opacity: 0.7
                    }
                }
            }
            Label {
                anchors.centerIn: parent
                visible: list.count === 0
                text: "Nothing matches."
            }
        }
    }
}
