import QtQuick
import QtQuick.Window
import QtQuick.Dialogs
import QtWebView

Window {
    id: root
    visible: true
    width: 1280
    height: 800
    minimumWidth: 360
    minimumHeight: 480
    title: "Mosaic"
    color: "#eceef3"

    property int saveId: 0

    // ---- window state: full screen (F11 in the page), remembered size and position ----
    readonly property bool desktop: ["windows", "linux", "osx", "macos"].indexOf(Qt.platform.os) >= 0
    property int lastWindowed: Window.Windowed
    property rect normalRect: Qt.rect(0, 0, 0, 0)
    function trackGeometry() { if (visibility === Window.Windowed) normalRect = Qt.rect(x, y, width, height) }
    onXChanged: trackGeometry()
    onYChanged: trackGeometry()
    onWidthChanged: trackGeometry()
    onHeightChanged: trackGeometry()
    function setFullScreen(on) {
        if (on) {
            if (visibility !== Window.FullScreen) visibility = Window.FullScreen
        } else if (visibility === Window.FullScreen) {
            visibility = (lastWindowed === Window.Maximized) ? Window.Maximized : Window.Windowed
        }
    }
    onVisibilityChanged: {
        assets.setFullScreenState(visibility === Window.FullScreen)
        if (visibility === Window.Windowed || visibility === Window.Maximized) lastWindowed = visibility
    }
    Component.onCompleted: {
        if (!desktop) return
        const g = assets.savedGeometry
        if (g.width > 0) { x = g.x; y = g.y; width = g.width; height = g.height }
        normalRect = Qt.rect(x, y, width, height)
        if (assets.startFullScreen) visibility = Window.FullScreen
        else if (assets.savedMaximized) visibility = Window.Maximized
    }
    onClosing: {
        if (!desktop) return
        const r = normalRect
        assets.saveWindow(r.x, r.y, r.width, r.height,
                          visibility === Window.Maximized || (visibility === Window.FullScreen && lastWindowed === Window.Maximized))
    }

    WebView {
        id: web
        anchors.fill: parent
        url: startUrl
        focus: true
        // Android back button/gesture walks back through the web app's history
        Keys.onBackPressed: (event) => {
            event.accepted = web.canGoBack
            if (web.canGoBack) web.goBack()
        }
    }

    // Exports from the web page arrive here and are written wherever the person chooses.
    FileDialog {
        id: saveDialog
        title: "Save export"
        fileMode: FileDialog.SaveFile
        onAccepted: assets.completeSave(root.saveId, selectedFile)
        onRejected: assets.cancelSave(root.saveId)
    }

    Connections {
        target: assets
        function onWindowCommand(command, value) {
            if (command === "fullscreen") root.setFullScreen(value)
        }
        function onSaveRequested(id, name) {
            root.saveId = id
            saveDialog.currentFolder = assets.downloadFolder
            saveDialog.currentFile = assets.downloadFolder + "/" + name
            saveDialog.open()
        }
    }
}
