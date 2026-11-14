#pragma once
#include <QHash>
#include <QPointer>
#include <QRect>
#include <QTcpServer>
#include <QUrl>

class QTcpSocket;

// Tiny HTTP server bound to 127.0.0.1. It
//  * serves the web app from the Qt resource system (keeps the app's strict CSP, localStorage
//    and Supabase auth working, which file:// does not), and
//  * accepts "save this file" requests from the page (POST /__save) and hands them to the native
//    Save dialog, because embedded web views do not handle <a download> consistently.
class AssetServer : public QTcpServer
{
    Q_OBJECT
    Q_PROPERTY(QUrl downloadFolder READ downloadFolder CONSTANT)
    Q_PROPERTY(QString pluginsDir READ pluginsDir CONSTANT)
    Q_PROPERTY(bool startFullScreen READ startFullScreen CONSTANT)
    Q_PROPERTY(bool savedMaximized READ savedMaximized CONSTANT)
    Q_PROPERTY(QRect savedGeometry READ savedGeometry CONSTANT)
public:
    explicit AssetServer(QObject *parent = nullptr);

    // Tries the preferred port first (so the web origin, and therefore saved
    // sign-in / localStorage data, stays the same between launches), then any free port.
    bool start(quint16 preferredPort = 38417);
    QUrl url() const;
    QUrl downloadFolder() const;
    QString pluginsDir() const { return pluginsDir_; }

    // Window preferences (stored with QSettings) and state shared with the web page.
    bool startFullScreen() const;
    bool savedMaximized() const;
    QRect savedGeometry() const; // empty when nothing is saved, remembering is off, or the spot is off screen
    Q_INVOKABLE void setFullScreenState(bool fullScreen) { fullScreen_ = fullScreen; }
    Q_INVOKABLE void saveWindow(int x, int y, int w, int h, bool maximized);

    Q_INVOKABLE void completeSave(int id, const QUrl &target);
    Q_INVOKABLE void cancelSave(int id);

signals:
    void saveRequested(int id, const QString &suggestedName);
    void windowCommand(const QString &command, bool value); // from the page: "fullscreen"

protected:
    void incomingConnection(qintptr handle) override;

private:
    struct Pending { QPointer<QTcpSocket> socket; QByteArray data; };
    using Headers = QHash<QByteArray, QByteArray>;

    void onReadyRead(QTcpSocket *socket);
    void handle(QTcpSocket *socket, const QByteArray &method, const QByteArray &target,
                const Headers &headers, const QByteArray &body);
    void handlePlugins(QTcpSocket *socket, const QString &path, bool head);
    void handleSave(QTcpSocket *socket, const QByteArray &target, const Headers &headers, const QByteArray &body);
    void handleWindow(QTcpSocket *socket, const QByteArray &method, const QByteArray &target, const Headers &headers);
    bool fromOurPage(const Headers &headers) const;
    void respond(QTcpSocket *socket, int status, const QByteArray &reason,
                 const QByteArray &contentType, const QByteArray &body, bool headOnly = false);
    static QByteArray contentTypeFor(const QString &path);
    static QString safeFileName(const QString &name);

    QHash<QTcpSocket *, QByteArray> buffers_;
    QHash<int, Pending> pending_;
    int nextId_ = 1;
    bool fullScreen_ = false;
    QString pluginsDir_;
};
