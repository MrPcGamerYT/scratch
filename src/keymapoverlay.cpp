#include "keymapoverlay.h"
#include <QApplication>
#include <QColor>
#include <QFontMetrics>
#include <QPainter>
#include <QPainterPath>
#include <QShowEvent>
#include <QTimer>
#include <algorithm>
#include <cmath>
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

namespace {
void drawKeyPill(QPainter *p, QPointF center, const QString &text, const QColor &accent = QColor("#9d86ff")) {
    QFont f = p->font();
    f.setPixelSize(11);
    f.setBold(true);
    p->setFont(f);
    const QFontMetrics fm(f);
    const qreal w = std::max<qreal>(24.0, fm.horizontalAdvance(text) + 12.0);
    const qreal h = 22.0;
    const QRectF r(center.x() - w / 2.0, center.y() - h / 2.0, w, h);
    p->setPen(QPen(accent, 1.4));
    p->setBrush(QColor(16, 18, 30, 215));
    p->drawRoundedRect(r, 11.0, 11.0);
    p->setPen(QColor("#f5f3ff"));
    p->drawText(r, Qt::AlignCenter, text);
}
}

KeymapOverlay::KeymapOverlay(QWidget *viewport, QWidget *mainWindow)
    : QWidget(mainWindow, Qt::Tool | Qt::FramelessWindowHint |
                          Qt::WindowDoesNotAcceptFocus | Qt::WindowTransparentForInput),
      targetViewport(viewport),
      ownerWindow(mainWindow),
      syncTimer(new QTimer(this)) {
    setAttribute(Qt::WA_TranslucentBackground, true);
    setAttribute(Qt::WA_ShowWithoutActivating, true);
    setAttribute(Qt::WA_TransparentForMouseEvents, true);
    syncTimer->setInterval(60);
    connect(syncTimer, &QTimer::timeout, this, &KeymapOverlay::syncGeometryAndVisibility);
}

void KeymapOverlay::setProfile(const KeymapProfile &profile) {
    currentProfile = profile;
    refreshSync();
    update();
}

void KeymapOverlay::setHintsVisible(bool visible) {
    showHints = visible;
    refreshSync();
    update();
}

void KeymapOverlay::setAimCrosshairActive(bool active) {
    crosshairActive = active;
    refreshSync();
    update();
}

void KeymapOverlay::refreshSync() {
    const bool wantAny = (showHints && currentProfile.overlayOpacity > 0 && currentProfile.hasAnyControls()) ||
                         (crosshairActive && currentProfile.aim.showCrosshair);
    if (wantAny) {
        if (!syncTimer->isActive()) syncTimer->start();
        syncGeometryAndVisibility();
    } else {
        syncTimer->stop();
        hide();
    }
}

void KeymapOverlay::showEvent(QShowEvent *event) {
    QWidget::showEvent(event);
    const HWND hwnd = reinterpret_cast<HWND>(winId());
    if (hwnd && IsWindow(hwnd)) {
        const LONG_PTR ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE,
                          ex | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW);
    }
}

void KeymapOverlay::syncGeometryAndVisibility() {
    if (!targetViewport || !ownerWindow || !ownerWindow->isVisible() || ownerWindow->isMinimized() ||
        !targetViewport->isVisible() || QApplication::activeModalWidget() != nullptr ||
        GetForegroundWindow() != reinterpret_cast<HWND>(ownerWindow->winId())) {
        if (isVisible()) hide();
        return;
    }
    const bool wantHints = showHints && currentProfile.overlayOpacity > 0 && currentProfile.hasAnyControls();
    const bool wantCross = crosshairActive && currentProfile.aim.showCrosshair;
    if (!wantHints && !wantCross) {
        if (isVisible()) hide();
        return;
    }
    const QPoint topLeft = targetViewport->mapToGlobal(QPoint(0, 0));
    const QRect targetRect(topLeft, targetViewport->size());
    if (targetRect.width() < 32 || targetRect.height() < 32) {
        if (isVisible()) hide();
        return;
    }
    if (geometry() != targetRect) setGeometry(targetRect);
    if (!isVisible()) show();
}

void KeymapOverlay::paintEvent(QPaintEvent *) {
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, true);
    const qreal w = width();
    const qreal h = height();
    if (w <= 0 || h <= 0) return;

    if (showHints && currentProfile.overlayOpacity > 0) {
        p.save();
        p.setOpacity(std::clamp(currentProfile.overlayOpacity, 0, 100) / 100.0);

        for (const auto &d : currentProfile.dpads) {
            if (!d.showOverlay) continue;
            const QPointF c(d.x * w, d.y * h);
            const qreal r = std::max<qreal>(32.0, d.radius * std::min(w, h));
            p.setPen(QPen(QColor(157, 134, 255, 165), 1.5, Qt::DashLine));
            p.setBrush(QColor(16, 18, 30, 75));
            p.drawEllipse(c, r, r);
            drawKeyPill(&p, QPointF(c.x(), c.y() - r * 0.72), keyShortBadge(d.keyUp));
            drawKeyPill(&p, QPointF(c.x(), c.y() + r * 0.72), keyShortBadge(d.keyDown));
            drawKeyPill(&p, QPointF(c.x() - r * 0.72, c.y()), keyShortBadge(d.keyLeft));
            drawKeyPill(&p, QPointF(c.x() + r * 0.72, c.y()), keyShortBadge(d.keyRight));
        }

        for (const auto &sw : currentProfile.swipes) {
            if (!sw.showOverlay) continue;
            const QPointF p1(sw.x1 * w, sw.y1 * h);
            const QPointF p2(sw.x2 * w, sw.y2 * h);
            p.setPen(QPen(QColor("#57d9ff"), 2.0, Qt::DashLine));
            p.drawLine(p1, p2);
            p.setBrush(QColor("#57d9ff"));
            p.setPen(Qt::NoPen);
            p.drawEllipse(p2, 4.5, 4.5);
            drawKeyPill(&p, (p1 + p2) * 0.5, QStringLiteral("↝ ") + keyShortBadge(sw.key), QColor("#57d9ff"));
        }

        for (const auto &z : currentProfile.zooms) {
            if (!z.showOverlay) continue;
            const QPointF c(z.x * w, z.y * h);
            const qreal r = std::max<qreal>(26.0, z.radius * std::min(w, h) * 0.7);
            p.setPen(QPen(QColor("#ffd166"), 1.5, Qt::DotLine));
            p.setBrush(Qt::NoBrush);
            p.drawEllipse(c, r, r);
            const QString mod = z.modifierKey ? (keyShortBadge(z.modifierKey) + "+") : QString();
            drawKeyPill(&p, c, mod + keyShortBadge(z.keyIn) + "/" + keyShortBadge(z.keyOut), QColor("#ffd166"));
        }

        for (const auto &m : currentProfile.mobaSkills) {
            if (!m.showOverlay) continue;
            const QPointF c(m.x * w, m.y * h);
            const qreal r = std::max<qreal>(24.0, m.radius * std::min(w, h) * 0.65);
            p.setPen(QPen(QColor("#ff7ab8"), 1.6));
            p.setBrush(QColor(255, 122, 184, 30));
            p.drawEllipse(c, r, r);
            drawKeyPill(&p, c, QStringLiteral("⚡") + keyShortBadge(m.key), QColor("#ff7ab8"));
        }

        for (const auto &sc : currentProfile.scripts) {
            if (!sc.showOverlay) continue;
            drawKeyPill(&p, QPointF(sc.x * w, sc.y * h), QStringLiteral("⌘ ") + keyShortBadge(sc.key), QColor("#6ee7b7"));
        }

        for (const auto &b : currentProfile.taps) {
            if (!b.showOverlay) continue;
            QString badge = keyShortBadge(b.key);
            if (b.repeat) badge += QStringLiteral("↻");
            else if (b.burstCount > 1) badge += QStringLiteral("×%1").arg(b.burstCount);
            drawKeyPill(&p, QPointF(b.x * w, b.y * h), badge);
        }

        if (currentProfile.aim.enabled && currentProfile.aim.showOverlay) {
            const auto &a = currentProfile.aim;
            drawKeyPill(&p, QPointF(a.anchorX * w, a.anchorY * h),
                        QStringLiteral("⌖ ") + keyShortBadge(a.toggleKey), QColor("#7cf7c4"));
            if (a.fireEnabled) drawKeyPill(&p, QPointF(a.fireX * w, a.fireY * h), QStringLiteral("LMB"), QColor("#ff7875"));
            if (a.adsEnabled) drawKeyPill(&p, QPointF(a.adsX * w, a.adsY * h), QStringLiteral("RMB"), QColor("#ffd166"));
            if (a.freeLookEnabled && a.freeLookKey != 0)
                drawKeyPill(&p, QPointF(a.freeLookX * w, a.freeLookY * h),
                            QStringLiteral("👁 ") + keyShortBadge(a.freeLookKey), QColor("#69c0ff"));
        }
        p.restore();
    }

    if (crosshairActive && currentProfile.aim.showCrosshair) {
        p.save();
        p.setOpacity(0.95);
        if (currentProfile.aim.fireEnabled && !showHints)
            drawKeyPill(&p, QPointF(currentProfile.aim.fireX * w, currentProfile.aim.fireY * h), QStringLiteral("FIRE · LMB"), QColor("#ff7875"));
        const QPointF center(w * 0.5, h * 0.5);
        const qreal sz = std::clamp(currentProfile.aim.crosshairSize, 4, 64);
        QColor col(currentProfile.aim.crosshairColor);
        if (!col.isValid()) col = QColor("#7cf7c4");

        auto drawReticle = [&](const QPen &pen) {
            p.setPen(pen);
            p.setBrush(Qt::NoBrush);
            switch (currentProfile.aim.crosshairStyle) {
            case CrosshairStyle::CrossDot:
                p.drawLine(QPointF(center.x() - sz, center.y()), QPointF(center.x() - 4, center.y()));
                p.drawLine(QPointF(center.x() + 4, center.y()), QPointF(center.x() + sz, center.y()));
                p.drawLine(QPointF(center.x(), center.y() - sz), QPointF(center.x(), center.y() - 4));
                p.drawLine(QPointF(center.x(), center.y() + 4), QPointF(center.x(), center.y() + sz));
                p.setBrush(pen.color());
                p.setPen(Qt::NoPen);
                p.drawEllipse(center, 2.2, 2.2);
                break;
            case CrosshairStyle::Cross:
                p.drawLine(QPointF(center.x() - sz, center.y()), QPointF(center.x() + sz, center.y()));
                p.drawLine(QPointF(center.x(), center.y() - sz), QPointF(center.x(), center.y() + sz));
                break;
            case CrosshairStyle::Dot:
                p.setBrush(pen.color());
                p.setPen(Qt::NoPen);
                p.drawEllipse(center, std::max<qreal>(3.0, sz * 0.25), std::max<qreal>(3.0, sz * 0.25));
                break;
            case CrosshairStyle::Circle:
                p.drawEllipse(center, sz * 0.85, sz * 0.85);
                p.setBrush(pen.color());
                p.setPen(Qt::NoPen);
                p.drawEllipse(center, 2.0, 2.0);
                break;
            }
        };
        drawReticle(QPen(QColor(0, 0, 0, 180), 3.4));
        drawReticle(QPen(col, 1.8));
        p.restore();
    }
}
