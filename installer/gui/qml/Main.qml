// Samurai OS graphical installer: step list on the left, one page on the right.
// It only edits choices; `installer` (Python) validates them and runs the one install engine.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "components"

ApplicationWindow {
    id: window
    title: "Install Samurai OS"
    width: 1024
    height: 720
    minimumWidth: 900
    minimumHeight: 620
    visible: true
    visibility: Window.Maximized
    color: palette.window

    readonly property var facts: installer.facts
    readonly property var options: installer.options
    property int index: 0
    property string welcomeMessage: ""
    property bool confirmed: false

    readonly property var pages: [
        { id: "welcome", file: "Welcome", title: "Welcome", hint: "Check this computer before you start" },
        { id: "region", file: "Region", title: "Language & region", hint: "Language, time zone and keyboard" },
        { id: "disk", file: "Disk", title: "Disk", hint: "Where Samurai OS goes, and its encryption" },
        { id: "user", file: "User", title: "User", hint: "Your account and the computer's name" },
        { id: "kernels", file: "Kernels", title: "Kernels", hint: "At least one; two give you a fallback" },
        { id: "bootloader", file: "Bootloader", title: "Bootloader", hint: "The menu that starts the system" },
        { id: "desktop", file: "Desktop", title: "Desktop & theme", hint: "How your system looks" },
        { id: "hardware", file: "Hardware", title: "Hardware", hint: "Graphics drivers" },
        { id: "gaming", file: "Gaming", title: "Gaming", hint: "Launchers, tools and compatibility layers" },
        { id: "development", file: "Development", title: "Development", hint: "Languages, containers and AI tools" },
        { id: "apps", file: "Apps", title: "Applications", hint: "Search and tick what you want" },
        { id: "network", file: "Network", title: "Network & security", hint: "Firewall and remote access" },
        { id: "summary", file: "Summary", title: "Summary", hint: "Nothing has been changed yet" },
        { id: "install", file: "Install", title: "Installing", hint: "Keep the computer powered on" },
        { id: "done", file: "Done", title: "Done", hint: "Samurai OS is installed" }
    ]
    readonly property string pageId: pages[index].id
    readonly property int summaryIndex: 12
    readonly property bool wizard: index <= summaryIndex
    readonly property string pageError: {
        installer.revision
        return installer.pageError(pageId)
    }
    readonly property string blocker: {
        installer.revision
        if (pageId !== "summary")
            return pageError
        for (let i = 0; i < summaryIndex; i++) {
            const problem = installer.pageError(pages[i].id)
            if (problem !== "")
                return pages[i].title + ": " + problem
        }
        if (installer.summary.error)
            return installer.summary.error
        return installer.networkRequirement()
    }

    function go(target) {
        if (target < 0 || target >= pages.length)
            return
        if (target !== summaryIndex)
            confirmed = false
        if (pages[target].id === "summary")
            installer.buildSummary()
        if (pages[target].id === "disk")
            installer.refreshDisks()
        index = target
        Qt.callLater(activatePage)
    }
    function activatePage() {
        const item = pageRepeater.itemAt(index)
        if (item && item.item && item.item.activate)
            item.item.activate()
    }
    function next() {
        if (wizard && index < summaryIndex && blocker === "")
            go(index + 1)
    }
    function back() {
        if (wizard && index > 0)
            go(index - 1)
    }
    function install() {
        if (pageId === "summary" && blocker === "" && confirmed) {
            go(summaryIndex + 1)
            installer.startInstall()
        }
    }

    onClosing: close => {
        if (installer.state === "running") {
            close.accepted = false
            busyDialog.open()
        }
    }
    Component.onCompleted: Qt.callLater(activatePage)

    Connections {
        target: installer
        function onStateChanged() {
            if (installer.state === "done")
                window.go(window.summaryIndex + 2)
        }
        function onChoicesReset() {
            pageRepeater.model = 0
            pageRepeater.model = window.pages.length
        }
    }

    Shortcut { sequence: "Alt+N"; onActivated: window.next() }
    Shortcut { sequence: "Alt+B"; onActivated: window.back() }
    Shortcut { sequence: "Alt+I"; onActivated: window.install() }

    Dialog {
        id: busyDialog
        anchors.centerIn: parent
        modal: true
        title: "Installation in progress"
        standardButtons: Dialog.Ok
        Label { text: "Closing now would not stop the installation.\nPlease wait until it finishes." }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: 220
            color: palette.base

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 12
                spacing: 2

                Label {
                    text: "Samurai OS"
                    font.pointSize: 17
                    font.bold: true
                    Layout.bottomMargin: 10
                    Layout.leftMargin: 8
                }
                Repeater {
                    model: window.pages
                    delegate: ItemDelegate {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        text: (index + 1) + ".  " + modelData.title
                        highlighted: index === window.index
                        // going back is always safe; forward only through Next
                        readonly property bool reachable: window.wizard && index <= window.index
                        hoverEnabled: reachable
                        focusPolicy: reachable ? Qt.StrongFocus : Qt.NoFocus
                        opacity: reachable || highlighted ? 1 : 0.6
                        onClicked: if (reachable) window.go(index)
                    }
                }
                Item { Layout.fillHeight: true }
                Label {
                    text: (window.facts.flavor === "offline" ? "Offline" : "Online") + " edition  ·  "
                          + (installer.online ? "network connected" : "no network")
                    opacity: 0.7
                    font.pointSize: 9
                    Layout.leftMargin: 8
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }
            }
        }
        Rectangle { Layout.fillHeight: true; Layout.preferredWidth: 1; color: palette.mid; opacity: 0.4 }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            ColumnLayout {
                Layout.fillWidth: true
                Layout.margins: 20
                Layout.bottomMargin: 8
                spacing: 2
                Label {
                    text: window.pages[window.index].title
                    textFormat: Text.PlainText
                    font.pointSize: 18
                    font.bold: true
                }
                Label { text: window.pages[window.index].hint; opacity: 0.75 }
            }

            StackLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.leftMargin: 20
                Layout.rightMargin: 8
                currentIndex: window.index
                Repeater {
                    id: pageRepeater
                    model: window.pages.length
                    delegate: Loader {
                        required property int index
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        source: "pages/" + window.pages[index].file + ".qml"
                    }
                }
            }

            Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: palette.mid; opacity: 0.4 }
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 12
                spacing: 12
                visible: window.wizard

                Button {
                    text: "Back"
                    icon.name: "go-previous"
                    enabled: window.index > 0
                    onClicked: window.back()
                }
                PlainLabel {
                    Layout.fillWidth: true
                    text: window.blocker
                    horizontalAlignment: Text.AlignRight
                    opacity: 0.85
                }
                Button {
                    visible: window.index < window.summaryIndex
                    text: "Next"
                    icon.name: "go-next"
                    highlighted: true
                    enabled: window.blocker === ""
                    onClicked: window.next()
                }
                Button {
                    visible: window.pageId === "summary"
                    text: "Erase disk and install"
                    icon.name: "run-install"
                    highlighted: enabled
                    enabled: window.blocker === "" && window.confirmed
                    onClicked: window.install()
                }
            }
        }
    }
}
