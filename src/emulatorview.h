#pragma once

#include <QWidget>
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

class EmulatorView : public QWidget {
public:
    explicit EmulatorView(QWidget *parent = nullptr);
    bool attach(HWND window);
    void clear();
    HWND windowHandle() const { return guestWindow; }

protected:
    void resizeEvent(QResizeEvent *event) override;
    void paintEvent(QPaintEvent *event) override;

private:
    HWND guestWindow = nullptr;
};
