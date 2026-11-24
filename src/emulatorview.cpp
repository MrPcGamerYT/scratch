#include "emulatorview.h"
#include <QResizeEvent>
#include <QPainter>
#include <QLinearGradient>
#include <QTimer>

EmulatorView::EmulatorView(QWidget *parent) : QWidget(parent) {
    animation = new QTimer(this); animation->setInterval(80);
    connect(animation, &QTimer::timeout, this, [this] { animationFrame = (animationFrame + 1) % 45; update(); });
    setAttribute(Qt::WA_NativeWindow);
    setMinimumSize(480, 320);
    setAttribute(Qt::WA_OpaquePaintEvent);
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
    animation->stop(); starting = false;
    guestWindow = window;
    SetWindowPos(window, nullptr, 0, 0, qRound(width() * devicePixelRatioF()), qRound(height() * devicePixelRatioF()),
                 SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW);
    return true;
}

void EmulatorView::clear() { guestWindow = nullptr; setStartupStatus("Android stopped", "Your per-user disk is preserved. Open Settings → Diagnostics for details."); }

void EmulatorView::setStartupStatus(const QString &title, const QString &detail, bool busy) {
    startupTitle = title; startupDetail = detail; starting = busy;
    if (busy && !guestWindow) animation->start(); else animation->stop();
    update();
}

void EmulatorView::paintEvent(QPaintEvent *) {
    if (guestWindow && IsWindow(guestWindow)) return;
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
