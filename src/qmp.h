#pragma once
#include <QObject>
#include <QByteArray>
#include <QJsonArray>
#include <QSet>
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
    void touchDown(int slot, double x, double y);
    void touchMove(int slot, double x, double y);
    void touchUp(int slot);
    void releaseAllInputs();
    const QSet<int> &activeTouchSlots() const { return touches; }
    static QJsonArray touchEvents(const QString &type, int slot, double x, double y, bool firstOrLast);
    static QJsonArray moveEvents(double x, double y);
    static QJsonArray pointerEvents(double x, double y, bool down);
signals:
    void readyChanged(bool ready);
    void error(const QString &message);
private:
    void readOutput();
    void sendTouch(const QJsonArray &events);
    void send(const QString &command, const QJsonArray &events = {});
    QProcess *process;
    QByteArray buffer;
    QSet<int> touches;
    bool pointerPressed = false;
    bool ready = false;
};
