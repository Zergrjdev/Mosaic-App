#include <QGuiApplication>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QSGRendererInterface>
#include <QtWebView/QtWebView>

#include "AssetServer.h"

int main(int argc, char *argv[])
{
    // Windows and Linux render pages with Qt WebEngine, which needs OpenGL.
    // (Android, iOS and macOS use the system web view and need nothing here.)
#if (defined(Q_OS_LINUX) && !defined(Q_OS_ANDROID)) || defined(Q_OS_WIN)
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGLRhi);
#endif
    // Must run before the QGuiApplication is created.
    QtWebView::initialize();

    QGuiApplication app(argc, argv);
    QGuiApplication::setApplicationName("Mosaic");
    QGuiApplication::setOrganizationName("Mosaic");
    QGuiApplication::setApplicationVersion(APP_VERSION);
    QGuiApplication::setWindowIcon(QIcon(":/web/icon-512.png"));

    AssetServer server;
    if (!server.start()) {
        qCritical("Mosaic: could not start the local asset server.");
        return 1;
    }

    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("startUrl", server.url().resolved(QUrl(QStringLiteral("app.html"))));
    engine.rootContext()->setContextProperty("assets", &server);
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed,
                     &app, [] { QCoreApplication::exit(-1); }, Qt::QueuedConnection);
    engine.load(QUrl(QStringLiteral("qrc:/qml/Main.qml")));

    return app.exec();
}
