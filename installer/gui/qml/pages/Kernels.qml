import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

PageBody {
    Repeater {
        model: window.options.kernels
        delegate: Choice {
            required property var modelData
            label: modelData.name + "  (" + modelData.id + ")"
            text: modelData.text
            checked: installer.choices.kernels.indexOf(modelData.id) >= 0
            onToggled: on => installer.toggle("kernels", modelData.id, on)
        }
    }
    Heading { text: "Start by default with" }
    ComboBox {
        id: combo
        Layout.preferredWidth: 360
        model: window.options.kernels.map(k => k.id)
                   .filter(id => installer.choices.kernels.indexOf(id) >= 0)
        currentIndex: model.indexOf(installer.choices.default_kernel)
        onActivated: at => installer.choices.default_kernel = model[at]
    }
    Notice { text: "The other kernels stay in the boot menu. You can add or remove kernels later with “samurai kernel”." }
}
