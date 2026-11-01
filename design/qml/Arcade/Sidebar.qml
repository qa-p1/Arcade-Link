import QtQuick
import QtQuick.Templates as T

// Template A sidebar: 212 px on bg with a right line; the brand (28 px mark
// tile + app name), NavItem tabs (its children, Up/Down move between them),
// a footer slot and "Version x.y.z". `collapsed` (below 760 px) shows icons.
Rectangle {
    id: root
    property string appName: ""
    property string appId: Theme.appId
    property string version: ""
    property bool collapsed: false
    property alias currentIndex: tabs.currentIndex
    default property alias items: tabs.contentData
    property alias footer: footerSlot.data
    implicitWidth: collapsed ? Theme.sidebarCollapsed : Theme.sidebar
    color: Theme.bg
    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.line }

    Row {
        id: brand
        x: root.collapsed ? (root.width - Theme.tileSm) / 2 : Theme.space12 + Theme.space8
        y: Theme.space16
        spacing: Theme.space10
        Rectangle {
            width: Theme.tileSm; height: Theme.tileSm
            radius: Theme.radiusSm
            color: Theme.text
            Glyph { anchors.centerIn: parent; appId: root.appId; color: Theme.bg }
        }
        Label {
            visible: !root.collapsed
            text: root.appName
            kind: "heading"
            anchors.verticalCenter: parent.verticalCenter
        }
    }
    T.TabBar {
        id: tabs
        x: Theme.space12
        y: brand.y + brand.height + Theme.space24
        width: root.width - Theme.space12 * 2
        height: contentItem.implicitHeight
        spacing: Theme.space2
        contentItem: ListView {
            model: tabs.contentModel
            currentIndex: tabs.currentIndex
            spacing: tabs.spacing
            interactive: false
            implicitHeight: contentHeight
        }
        Keys.onUpPressed: { currentIndex = Math.max(0, currentIndex - 1); itemAt(currentIndex).forceActiveFocus() }
        Keys.onDownPressed: { currentIndex = Math.min(count - 1, currentIndex + 1); itemAt(currentIndex).forceActiveFocus() }
    }
    Column {
        id: foot
        x: Theme.space12 + Theme.space8
        width: root.width - x * 2
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.space14
        spacing: Theme.space10
        Rectangle { width: parent.width + Theme.space8 * 2; x: -Theme.space8; height: 1; color: Theme.line }
        Column { id: footerSlot; width: parent.width; spacing: Theme.space10 }
        Item {
            visible: root.version !== "" && !root.collapsed
            width: parent.width
            height: versionText.implicitHeight
            Label { text: "Version"; kind: "caption"; color: Theme.muted }
            Label { id: versionText; anchors.right: parent.right; text: root.version; kind: "caption"; color: Theme.muted; font.features: { "tnum": 1, "cv11": 1 } }
        }
    }
}
