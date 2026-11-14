#include "AssetServer.h"

#include <QCoreApplication>
#include <QGuiApplication>
#include <QScreen>
#include <QSettings>
#include <QDir>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QFile>
#include <QFileInfo>
#include <QRegularExpression>
#include <QHostAddress>
#include <QStandardPaths>
#include <QTcpSocket>
#include <QUrlQuery>

namespace {
// Same policy as the site's _headers file.
const QByteArray kCsp =
    "default-src 'self'; script-src 'self' blob:; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; "
    "font-src 'self' https://fonts.gstatic.com data:; img-src 'self' data: blob: https:; media-src 'self' data: blob: https:; "
    "connect-src 'self' https://*.supabase.co wss://*.supabase.co https://discord.com https://discordapp.com https://fonts.googleapis.com http://127.0.0.1:* http://localhost:* https:; "
    "base-uri 'self'; form-action 'self'; object-src 'none'; frame-ancestors 'none'";
constexpr int kMaxHeaderBytes = 16 * 1024;
constexpr qint64 kMaxBodyBytes = 512LL * 1024 * 1024;
}

AssetServer::AssetServer(QObject *parent) : QTcpServer(parent)
{
    // Folder where people can drop their own plugins (served at /user-plugins/).
    pluginsDir_ = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + QStringLiteral("/plugins");
    QDir().mkpath(pluginsDir_);
}

bool AssetServer::start(quint16 preferredPort)
{
    return listen(QHostAddress::LocalHost, preferredPort) || listen(QHostAddress::LocalHost, 0);
}

QUrl AssetServer::url() const
{
    return QUrl(QStringLiteral("http://127.0.0.1:%1/").arg(serverPort()));
}

QUrl AssetServer::downloadFolder() const
{
    QString dir = QStandardPaths::writableLocation(QStandardPaths::DownloadLocation);
    if (dir.isEmpty()) dir = QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation);
    return QUrl::fromLocalFile(dir);
}

void AssetServer::incomingConnection(qintptr handle)
{
    auto *socket = new QTcpSocket(this);
    if (!socket->setSocketDescriptor(handle)) {
        delete socket;
        return;
    }
    connect(socket, &QTcpSocket::readyRead, this, [this, socket] { onReadyRead(socket); });
    connect(socket, &QTcpSocket::disconnected, this, [this, socket] {
        buffers_.remove(socket);
        for (auto it = pending_.begin(); it != pending_.end();) {   // browser gave up: forget the save
            if (it->socket == socket) it = pending_.erase(it); else ++it;
        }
        socket->deleteLater();
    });
}

void AssetServer::onReadyRead(QTcpSocket *socket)
{
    QByteArray &buf = buffers_[socket];
    buf += socket->readAll();

    const int headerEnd = buf.indexOf("\r\n\r\n");
    if (headerEnd < 0) {
        if (buf.size() > kMaxHeaderBytes)
            respond(socket, 431, "Request Header Fields Too Large", "text/plain", "Too large");
        return; // wait for the rest of the headers
    }

    const QList<QByteArray> lines = buf.left(headerEnd).split('\n');
    const QList<QByteArray> parts = lines.value(0).trimmed().split(' ');
    if (parts.size() < 2) {
        respond(socket, 400, "Bad Request", "text/plain", "Bad request");
        return;
    }
    Headers headers;
    for (const QByteArray &line : lines.mid(1)) {
        const int c = line.indexOf(':');
        if (c > 0) headers.insert(line.left(c).trimmed().toLower(), line.mid(c + 1).trimmed());
    }

    const qint64 want = headers.value("content-length", "0").toLongLong();
    if (want < 0 || want > kMaxBodyBytes) {
        respond(socket, 413, "Payload Too Large", "text/plain", "Too large");
        return;
    }
    if (buf.size() - (headerEnd + 4) < want) return; // body still arriving

    const QByteArray body = buf.mid(headerEnd + 4, int(want));
    buf.clear();
    handle(socket, parts[0], parts[1], headers, body);
}

void AssetServer::handle(QTcpSocket *socket, const QByteArray &method, const QByteArray &target,
                         const Headers &headers, const QByteArray &body)
{
    // Only answer requests addressed to the loopback origin (blocks DNS-rebinding tricks).
    const QByteArray port = QByteArray::number(serverPort());
    const QByteArray host = headers.value("host").toLower();
    if (host != "127.0.0.1:" + port && host != "localhost:" + port) {
        respond(socket, 403, "Forbidden", "text/plain", "Forbidden");
        return;
    }

    const bool head = (method == "HEAD");
    const QByteArray rawPath = target.split('?').value(0).split('#').value(0);

    if (method == "POST" && rawPath == "/__save") {
        handleSave(socket, target, headers, body);
        return;
    }
    if (rawPath == "/__window") { // full screen and remembered window state
        handleWindow(socket, method, target, headers);
        return;
    }
    if (method != "GET" && !head) {
        respond(socket, 405, "Method Not Allowed", "text/plain", "Method not allowed");
        return;
    }
    if (rawPath == "/__app") { // lets the page detect that it is running inside the app
        const QJsonObject o{{"app", true}, {"version", QCoreApplication::applicationVersion()}, {"pluginsDir", pluginsDir_}};
        respond(socket, 200, "OK", "application/json", QJsonDocument(o).toJson(QJsonDocument::Compact), head);
        return;
    }
    if (rawPath.startsWith("/user-plugins/")) {
        handlePlugins(socket, QUrl::fromPercentEncoding(rawPath.mid(14)), head);
        return;
    }

    QString path = QUrl::fromPercentEncoding(rawPath);
    if (path.isEmpty() || path == QLatin1String("/")) path = QStringLiteral("/index.html");
    if (path.contains(QLatin1String("..")) || path.contains(QLatin1Char('\\')) || !path.startsWith(QLatin1Char('/'))) {
        respond(socket, 400, "Bad Request", "text/plain", "Bad request", head);
        return;
    }
    QFile file(QStringLiteral(":/web") + path);
    if (!file.open(QIODevice::ReadOnly)) {
        respond(socket, 404, "Not Found", "text/plain", "Not found", head);
        return;
    }
    respond(socket, 200, "OK", contentTypeFor(path), file.readAll(), head);
}

void AssetServer::handlePlugins(QTcpSocket *socket, const QString &rel, bool head)
{
    if (rel == QLatin1String("index.json")) { // every sub-folder that has a valid manifest.json
        QJsonArray list;
        const QDir root(pluginsDir_);
        for (const QString &d : root.entryList(QDir::Dirs | QDir::NoDotAndDotDot, QDir::Name)) {
            if (d.startsWith(QLatin1Char('.'))) continue;
            QFile f(root.filePath(d + QStringLiteral("/manifest.json")));
            if (!f.open(QIODevice::ReadOnly)) continue;
            QJsonObject m = QJsonDocument::fromJson(f.readAll()).object();
            const QString entry = m.value(QStringLiteral("entry")).toString();
            if (m.isEmpty() || entry.isEmpty() || entry.contains(QLatin1String("..")) || entry.startsWith(QLatin1Char('/'))) continue;
            m.insert(QStringLiteral("entry"), d + QLatin1Char('/') + entry);
            list.append(m);
        }
        respond(socket, 200, "OK", "application/json", QJsonDocument(list).toJson(QJsonDocument::Compact), head);
        return;
    }
    static const QStringList ok{"js", "json", "css", "png", "svg", "txt", "md"};
    const QStringList segs = rel.split(QLatin1Char('/'));
    bool good = !rel.isEmpty() && ok.contains(rel.section(QLatin1Char('.'), -1).toLower());
    for (const QString &s : segs)
        if (s.isEmpty() || s.startsWith(QLatin1Char('.')) || s.contains(QLatin1Char('\\'))) good = false;
    QFile f(pluginsDir_ + QLatin1Char('/') + rel);
    if (!good || !f.open(QIODevice::ReadOnly)) {
        respond(socket, 404, "Not Found", "text/plain", "Not found", head);
        return;
    }
    respond(socket, 200, "OK", contentTypeFor(rel), f.readAll(), head);
}

QString AssetServer::safeFileName(const QString &name)
{
    QString n = QFileInfo(name).fileName();
    n.replace(QRegularExpression(QStringLiteral("[\\x00-\\x1f<>:\"/\\\\|?*]")), QStringLiteral("_"));
    n = n.trimmed().left(120);
    return n.isEmpty() ? QStringLiteral("mosaic-export") : n;
}

// Must come from our own page: right Origin and a custom header (which other websites cannot add
// without a CORS preflight that this server never answers).
bool AssetServer::fromOurPage(const Headers &headers) const
{
    const QByteArray origin = headers.value("origin");
    const QByteArray port = QByteArray::number(serverPort());
    return headers.value("x-mosaic") == "1"
        && (origin == "http://127.0.0.1:" + port || origin == "http://localhost:" + port);
}

bool AssetServer::startFullScreen() const { return QSettings().value(QStringLiteral("window/startFullScreen"), false).toBool(); }
bool AssetServer::savedMaximized() const
{
    QSettings s;
    return s.value(QStringLiteral("window/remember"), true).toBool() && s.value(QStringLiteral("window/maximized"), false).toBool();
}

QRect AssetServer::savedGeometry() const
{
    QSettings s;
    if (!s.value(QStringLiteral("window/remember"), true).toBool() || !s.contains(QStringLiteral("window/w")))
        return QRect();
    const QRect r(s.value(QStringLiteral("window/x")).toInt(), s.value(QStringLiteral("window/y")).toInt(),
                  s.value(QStringLiteral("window/w")).toInt(), s.value(QStringLiteral("window/h")).toInt());
    if (r.width() < 360 || r.height() < 480) return QRect();
    // Only reuse the position if a good part of the window would still be on a connected screen.
    for (QScreen *sc : QGuiApplication::screens()) {
        const QRect i = sc->availableGeometry().intersected(r);
        if (i.width() >= 200 && i.height() >= 120) return r;
    }
    return QRect();
}

void AssetServer::saveWindow(int x, int y, int w, int h, bool maximized)
{
    QSettings s;
    if (!s.value(QStringLiteral("window/remember"), true).toBool()) return;
    if (w >= 360 && h >= 480) {
        s.setValue(QStringLiteral("window/x"), x);
        s.setValue(QStringLiteral("window/y"), y);
        s.setValue(QStringLiteral("window/w"), w);
        s.setValue(QStringLiteral("window/h"), h);
    }
    s.setValue(QStringLiteral("window/maximized"), maximized);
}

void AssetServer::handleWindow(QTcpSocket *socket, const QByteArray &method, const QByteArray &target, const Headers &headers)
{
    if (headers.value("x-mosaic") != "1") {
        respond(socket, 403, "Forbidden", "text/plain", "Forbidden");
        return;
    }
    QSettings s;
    if (method == "GET") {
        const QJsonObject o{{"fullscreen", fullScreen_},
                            {"startFullscreen", s.value(QStringLiteral("window/startFullScreen"), false).toBool()},
                            {"remember", s.value(QStringLiteral("window/remember"), true).toBool()}};
        respond(socket, 200, "OK", "application/json", QJsonDocument(o).toJson(QJsonDocument::Compact));
        return;
    }
    if (method != "POST" || !fromOurPage(headers)) {
        respond(socket, method == "POST" ? 403 : 405, method == "POST" ? "Forbidden" : "Method Not Allowed", "text/plain", "Forbidden");
        return;
    }
    const int q = target.indexOf('?');
    const QUrlQuery qs(QString::fromLatin1(q < 0 ? QByteArray() : target.mid(q + 1)));
    const QString cmd = qs.queryItemValue(QStringLiteral("cmd"));
    const bool on = qs.queryItemValue(QStringLiteral("v")) == QLatin1String("1");
    if (cmd == QLatin1String("fullscreen")) emit windowCommand(cmd, on);
    else if (cmd == QLatin1String("startFullscreen")) s.setValue(QStringLiteral("window/startFullScreen"), on);
    else if (cmd == QLatin1String("remember")) s.setValue(QStringLiteral("window/remember"), on);
    else {
        respond(socket, 400, "Bad Request", "text/plain", "Unknown command");
        return;
    }
    respond(socket, 200, "OK", "text/plain", "ok");
}

void AssetServer::handleSave(QTcpSocket *socket, const QByteArray &target, const Headers &headers, const QByteArray &body)
{
    if (!fromOurPage(headers)) {
        respond(socket, 403, "Forbidden", "text/plain", "Forbidden");
        return;
    }

    const int q = target.indexOf('?');
    const QString name = safeFileName(QUrlQuery(QString::fromLatin1(q < 0 ? QByteArray() : target.mid(q + 1)))
                                          .queryItemValue(QStringLiteral("name"), QUrl::FullyDecoded));

    // Automation / testing: save straight into a folder instead of asking.
    const QString autoDir = qEnvironmentVariable("MOSAIC_AUTOSAVE_DIR");
    if (!autoDir.isEmpty()) {
        QDir().mkpath(autoDir);
        QFile f(QDir(autoDir).filePath(name));
        const bool ok = f.open(QIODevice::WriteOnly | QIODevice::Truncate) && f.write(body) == body.size();
        respond(socket, ok ? 200 : 500, ok ? "OK" : "Error", "text/plain", ok ? "saved" : "failed");
        return;
    }

    const int id = nextId_++;
    pending_.insert(id, {socket, body});
    emit saveRequested(id, name);
}

void AssetServer::completeSave(int id, const QUrl &target)
{
    const Pending p = pending_.take(id);
    if (!p.socket) return;
    const QString path = target.isLocalFile() ? target.toLocalFile() : target.toString(); // content:// on Android
    QFile f(path);
    const bool ok = f.open(QIODevice::WriteOnly | QIODevice::Truncate) && f.write(p.data) == p.data.size();
    f.close();
    respond(p.socket, ok ? 200 : 500, ok ? "OK" : "Error", "text/plain", ok ? "saved" : "failed");
}

void AssetServer::cancelSave(int id)
{
    const Pending p = pending_.take(id);
    if (p.socket) respond(p.socket, 409, "Conflict", "text/plain", "cancelled");
}

void AssetServer::respond(QTcpSocket *socket, int status, const QByteArray &reason,
                          const QByteArray &contentType, const QByteArray &body, bool headOnly)
{
    QByteArray out = "HTTP/1.1 " + QByteArray::number(status) + " " + reason + "\r\n";
    out += "Content-Type: " + contentType + "\r\n";
    out += "Content-Length: " + QByteArray::number(body.size()) + "\r\n";
    out += "Cache-Control: no-cache\r\n";
    out += "X-Content-Type-Options: nosniff\r\n";
    out += "Referrer-Policy: strict-origin-when-cross-origin\r\n";
    out += "Content-Security-Policy: " + kCsp + "\r\n";
    out += "Connection: close\r\n\r\n";
    if (!headOnly) out += body;
    socket->write(out);
    socket->disconnectFromHost();
}

QByteArray AssetServer::contentTypeFor(const QString &path)
{
    const QString ext = path.section(QLatin1Char('.'), -1).toLower();
    if (ext == "html") return "text/html; charset=utf-8";
    if (ext == "js")   return "text/javascript; charset=utf-8";
    if (ext == "css")  return "text/css; charset=utf-8";
    if (ext == "json" || ext == "webmanifest") return "application/json; charset=utf-8";
    if (ext == "png")  return "image/png";
    if (ext == "ico")  return "image/x-icon";
    if (ext == "svg")  return "image/svg+xml";
    if (ext == "woff2") return "font/woff2";
    if (ext == "webp") return "image/webp";
    if (ext == "txt")  return "text/plain; charset=utf-8";
    return "application/octet-stream";
}
