#include "aimmodel.h"
#include "rustcore.h"
#include <cstddef>
static_assert(sizeof(NexusAimResult) == 24 && offsetof(NexusAimResult, reset) == 16,
              "Rust/C++ aim result ABI mismatch");
AimModel::AimModel(QPointF anchor, double sensitivity)
    : center(nexus_aim_clamp_anchor(anchor.x()), nexus_aim_clamp_anchor(anchor.y())),
      point(center), gain(nexus_aim_clamp_sensitivity(sensitivity)) {}
bool AimModel::move(double dx, double dy, int width, int height) {
    const auto result = nexus_aim_move(center.x(), center.y(), point.x(), point.y(), gain, dx, dy, width, height);
    point = QPointF(result.x, result.y);
    return result.reset != 0;
}
