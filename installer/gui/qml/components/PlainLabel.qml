// A wrapping label that never interprets markup: disk models, labels and log text come
// from outside the wizard.
import QtQuick
import QtQuick.Controls

Label {
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
}
