#pragma once
#include <QLabel>
#include <QMouseEvent>
#include <QSettings>

// Separate small tool window stays above the native SDL child; only the badge
// accepts mouse input. Dragging it does not capture the Android viewport.
class FpsBadge : public QLabel {
public:
    FpsBadge(QWidget *owner, QWidget *viewport)
        : QLabel("Engine FPS: N/A", owner, Qt::Tool | Qt::FramelessWindowHint | Qt::WindowDoesNotAcceptFocus), target(viewport) {
        setAttribute(Qt::WA_ShowWithoutActivating);
        setCursor(Qt::SizeAllCursor);
        setToolTip("Drag to move · engine SDL presentation completions per second, not game FPS; unsupported renderers show N/A");
        setStyleSheet("background:#141927;color:#b8f5c8;padding:6px;border-radius:5px;");
    }
    void syncPosition() {
        auto point = QSettings().value("fpsPosition", QPoint(12, 12)).toPoint();
        point.setX(qBound(0, point.x(), qMax(0, target->width() - width())));
        point.setY(qBound(0, point.y(), qMax(0, target->height() - height())));
        move(target->mapToGlobal(point));
    }
protected:
    void mousePressEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton) { dragging = true; offset = event->globalPosition().toPoint() - pos(); event->accept(); }
    }
    void mouseMoveEvent(QMouseEvent *event) override {
        if (!dragging) return;
        QSettings().setValue("fpsPosition", target->mapFromGlobal(event->globalPosition().toPoint() - offset)); syncPosition();
    }
    void mouseReleaseEvent(QMouseEvent *) override { dragging = false; }
private:
    QWidget *target;
    QPoint offset;
    bool dragging = false;
};
