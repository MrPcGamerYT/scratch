#pragma once

#include <QWidget>
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

class QTimer;

class EmulatorView : public QWidget {
public:
    explicit EmulatorView(QWidget *parent = nullptr);
    ~EmulatorView() override;
    bool attach(HWND window);
    void clear();
    void setGuestVisible(bool visible);
    void setStartupStatus(const QString &title, const QString &detail, bool busy = false);
    HWND windowHandle() const { return guestWindow; }

protected:
    void focusInEvent(QFocusEvent *event) override;
    bool nativeEvent(const QByteArray &type, void *message, qintptr *result) override;
    void resizeEvent(QResizeEvent *event) override;
    void paintEvent(QPaintEvent *event) override;

private:
    HICON engineLargeIcon = nullptr;
    HICON engineSmallIcon = nullptr;
    HWND guestWindow = nullptr;
    bool guestVisible = true;
    QTimer *animation;
    int animationFrame = 0;
    bool starting = false;
    QString startupTitle = "Android is not running";
    QString startupDetail = "The developer-prepared Android system starts automatically. Open Settings → Diagnostics if startup fails.";
};
