#pragma once
#include <QPointF>

class AimModel {
public:
    AimModel(QPointF anchor, double sensitivity);
    AimModel(QPointF anchor, double sensitivityX, double sensitivityY);
    QPointF anchor() const { return center; }
    QPointF position() const { return point; }
    void reset() { point = center; }
    // True means the caller should release and re-press at the anchor.
    bool move(double dxPixels, double dyPixels, int width, int height);
private:
    QPointF center;
    QPointF point;
    double gainX;
    double gainY;
};
