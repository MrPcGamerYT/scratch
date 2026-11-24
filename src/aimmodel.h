#pragma once
#include <QPointF>

// Experimental single-pointer camera drag, not guest multitouch.
class AimModel {
public:
    AimModel(QPointF anchor, double sensitivity);
    QPointF anchor() const { return center; }
    QPointF position() const { return point; }
    void reset() { point = center; }
    // True means the caller should release and re-press at the anchor.
    bool move(double dxPixels, double dyPixels, int width, int height);
private:
    QPointF center;
    QPointF point;
    double gain;
};
