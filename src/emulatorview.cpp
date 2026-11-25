#include "emulatorview.h"
#include <QResizeEvent>
#include <QPainter>
#include <QLinearGradient>
#include <QTimer>
#include <QFocusEvent>
#include <QCoreApplication>
#include <shellapi.h>

EmulatorView::EmulatorView(QWidget *parent) : QWidget(parent) {
    const auto engine = (QCoreApplication::applicationDirPath() + "/qemu/bin/NexusEngine.exe").toStdWString();
    ExtractIconExW(engine.c_str(), 0, &engineLargeIcon, &engineSmallIcon, 1);
    animation = new QTimer(this); animation->setInterval(80);
    connect(animation, &QTimer::timeout, this, [this] { animationFrame = (animationFrame + 1) % 45; update(); });
    setAttribute(Qt::WA_NativeWindow);
    setFocusPolicy(Qt::StrongFocus);
    setMinimumSize(480, 320);
    setAttribute(Qt::WA_OpaquePaintEvent);
}

EmulatorView::~EmulatorView() {
    if (engineLargeIcon) DestroyIcon(engineLargeIcon);
    if (engineSmallIcon) DestroyIcon(engineSmallIcon);
}

bool EmulatorView::attach(HWND window) {
    if (!IsWindow(window)) return false;
    const LONG_PTR oldStyle = GetWindowLongPtr(window, GWL_STYLE);
    SetWindowLongPtr(window, GWL_STYLE,
                     (oldStyle & ~(WS_POPUP | WS_CAPTION | WS_THICKFRAME | WS_DISABLED)) | WS_CHILD | WS_TABSTOP);
    SetLastError(0);
    HWND previousParent = SetParent(window, reinterpret_cast<HWND>(winId()));
    if (!previousParent && GetLastError() != 0) {
        SetWindowLongPtr(window, GWL_STYLE, oldStyle);
        return false;
    }
    if (guestVisible) { animation->stop(); starting = false; }
    guestWindow = window;
    DWORD_PTR ignored = 0;
    if (engineLargeIcon) SendMessageTimeoutW(window, WM_SETICON, ICON_BIG, reinterpret_cast<LPARAM>(engineLargeIcon), SMTO_ABORTIFHUNG, 250, &ignored);
    if (engineSmallIcon) SendMessageTimeoutW(window, WM_SETICON, ICON_SMALL, reinterpret_cast<LPARAM>(engineSmallIcon), SMTO_ABORTIFHUNG, 250, &ignored);
    SetWindowPos(window, nullptr, 0, 0, qRound(width() * devicePixelRatioF()), qRound(height() * devicePixelRatioF()),
                 SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    ShowWindow(window, guestVisible ? SW_SHOW : SW_HIDE);
    return true;
}

void EmulatorView::setGuestVisible(bool visible) {
    guestVisible = visible;
    if (guestWindow && IsWindow(guestWindow)) ShowWindow(guestWindow, visible ? SW_SHOW : SW_HIDE);
    if (visible) animation->stop(); else { starting = true; animation->start(); }
    update();
}

void EmulatorView::clear() { guestWindow = nullptr; setStartupStatus("Android stopped", "Your per-user disk is preserved. Open Settings → Diagnostics for details."); }

void EmulatorView::setStartupStatus(const QString &title, const QString &detail, bool busy) {
    startupTitle = title; startupDetail = detail; starting = busy;
    if (busy && (!guestWindow || !guestVisible)) animation->start(); else animation->stop();
    update();
}

void EmulatorView::paintEvent(QPaintEvent *) {
    if (guestVisible && guestWindow && IsWindow(guestWindow)) return;
    QPainter p(this); p.setRenderHint(QPainter::Antialiasing);
    p.fillRect(rect(), QColor("#0b0d17"));
    const QPixmap wallpaper(":/assets/nexus-wallpaper.png");
    if (!wallpaper.isNull()) {
        QSizeF size = wallpaper.size(); size.scale(QSizeF(width(), height()), Qt::KeepAspectRatioByExpanding);
        QRectF target((width() - size.width()) / 2, (height() - size.height()) / 2, size.width(), size.height());
        p.setOpacity(0.48); p.drawPixmap(target, wallpaper, QRectF(wallpaper.rect())); p.setOpacity(1);
    }
    QLinearGradient shade(0, 0, 0, height()); shade.setColorAt(0, QColor(8, 10, 18, 60)); shade.setColorAt(1, QColor(8, 10, 18, 190)); p.fillRect(rect(), shade);
    const qreal centerX = width() / 2.0, centerY = height() / 2.0;
    p.setPen(QPen(QColor("#a995ff"), 2)); p.setBrush(QColor(139, 115, 255, 35)); p.drawRoundedRect(QRectF(centerX - 26, centerY - 93, 52, 52), 15, 15);
    p.setPen(QColor("#d9d1ff")); QFont logo = font(); logo.setPixelSize(24); logo.setBold(true); p.setFont(logo); p.drawText(QRectF(centerX - 26, centerY - 93, 52, 52), Qt::AlignCenter, "N");
    QFont title = font(); title.setPixelSize(23); title.setBold(true); p.setFont(title); p.setPen(QColor("#f4f2ff"));
    p.drawText(QRectF(20, centerY - 17, width() - 40, 38), Qt::AlignCenter, startupTitle);
    QFont small = font(); small.setPixelSize(13); p.setFont(small); p.setPen(QColor("#a8abc1"));
    p.drawText(QRectF(20, centerY + 24, width() - 40, 50), Qt::AlignHCenter | Qt::AlignTop | Qt::TextWordWrap, startupDetail);
    if (starting) {
        p.setPen(QPen(QColor("#60a5fa"), 3)); p.setBrush(Qt::NoBrush);
        p.drawArc(QRectF(centerX - 12, centerY + 92, 24, 24), animationFrame * 8 * 16, 260 * 16);
    }
}

void EmulatorView::resizeEvent(QResizeEvent *event) {
    QWidget::resizeEvent(event);
    if (guestWindow && IsWindow(guestWindow))
        MoveWindow(guestWindow, 0, 0, qRound(width() * devicePixelRatioF()), qRound(height() * devicePixelRatioF()), TRUE);
}

void EmulatorView::focusInEvent(QFocusEvent *event) {
    QWidget::focusInEvent(event);
    if (guestVisible && guestWindow && IsWindow(guestWindow)) ::SetFocus(guestWindow);
}

bool EmulatorView::nativeEvent(const QByteArray &type, void *message, qintptr *result) {
    const auto *msg = static_cast<MSG *>(message);
    if (msg->message == WM_MOUSEACTIVATE && guestVisible && guestWindow && IsWindow(guestWindow)) {
        // Do not consume the first click after activating the embedded SDL child.
        ::SetFocus(guestWindow); *result = MA_ACTIVATE; return true;
    }
    return QWidget::nativeEvent(type, message, result);
}
