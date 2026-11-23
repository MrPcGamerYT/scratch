#include "emulatorview.h"
#include <QResizeEvent>

EmulatorView::EmulatorView(QWidget *parent) : QWidget(parent) {
    setAttribute(Qt::WA_NativeWindow);
    setMinimumSize(480, 320);
    setStyleSheet("background: #101522;");
}

bool EmulatorView::attach(HWND window) {
    if (!IsWindow(window)) return false;
    const LONG_PTR oldStyle = GetWindowLongPtr(window, GWL_STYLE);
    SetWindowLongPtr(window, GWL_STYLE,
                     (oldStyle & ~(WS_POPUP | WS_CAPTION | WS_THICKFRAME)) | WS_CHILD);
    SetLastError(0);
    HWND previousParent = SetParent(window, reinterpret_cast<HWND>(winId()));
    if (!previousParent && GetLastError() != 0) {
        SetWindowLongPtr(window, GWL_STYLE, oldStyle);
        return false;
    }
    guestWindow = window;
    SetWindowPos(window, nullptr, 0, 0, qRound(width() * devicePixelRatioF()), qRound(height() * devicePixelRatioF()),
                 SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW);
    return true;
}

void EmulatorView::clear() { guestWindow = nullptr; }

void EmulatorView::resizeEvent(QResizeEvent *event) {
    QWidget::resizeEvent(event);
    if (guestWindow && IsWindow(guestWindow))
        MoveWindow(guestWindow, 0, 0, qRound(width() * devicePixelRatioF()), qRound(height() * devicePixelRatioF()), TRUE);
}
