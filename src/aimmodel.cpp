#include "aimmodel.h"
#include <QtGlobal>
#include <cmath>
AimModel::AimModel(QPointF anchor, double sensitivity)
    : center(qBound(0.1, anchor.x(), 0.9), qBound(0.1, anchor.y(), 0.9)), point(center),
      gain(qBound(0.1, sensitivity, 5.0)) {}
bool AimModel::move(double dx, double dy, int width, int height) {
    if (width <= 0 || height <= 0 || !std::isfinite(dx) || !std::isfinite(dy)) return false;
    const double x = point.x() + dx / width * gain;
    const double y = point.y() + dy / height * gain;
    // Restart the gesture before reaching the screen edge. This is not a
    // continuous multitouch gesture and may be interpreted differently by games.
    if (x < 0.03 || x > 0.97 || y < 0.03 || y > 0.97) { reset(); return true; }
    point = QPointF(x, y); return false;
}
