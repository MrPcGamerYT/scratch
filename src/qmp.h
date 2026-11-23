#pragma once
#include <QObject>
#include <QByteArray>
#include <QJsonArray>
class QProcess;

class Qmp : public QObject {
    Q_OBJECT
public:
    explicit Qmp(QProcess *process, QObject *parent = nullptr);
    void reset();
    bool isReady() const { return ready; }
    void powerDown();
    void pointerDown(double x, double y);
    void pointerUp();
    void pointerMove(double x, double y);
    static QJsonArray moveEvents(double x, double y);
    static QJsonArray pointerEvents(double x, double y, bool down);
signals:
    void readyChanged(bool ready);
    void error(const QString &message);
private:
    void readOutput();
    void send(const QString &command, const QJsonArray &events = {});
    QProcess *process;
    QByteArray buffer;
    bool ready = false;
};
