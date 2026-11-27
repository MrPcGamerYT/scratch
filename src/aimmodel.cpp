#include "aimmodel.h"
#include "rustcore.h"
#include <cstddef>

static_assert(sizeof(NexusAimResult) == 24 && offsetof(NexusAimResult, reset) == 16,
              "Rust/C++ aim result ABI mismatch");

AimModel::AimModel(QPointF anchor, double sensitivity)
    : AimModel(anchor, sensitivity, sensitivity) {}

AimModel::AimModel(QPointF anchor, double sensitivityX, double sensitivityY)
    : center(nexus_aim_clamp_anchor(anchor.x()), nexus_aim_clamp_anchor(anchor.y())),
      point(center),
      gainX(nexus_aim_clamp_sensitivity(sensitivityX)),
      gainY(nexus_aim_clamp_sensitivity(sensitivityY)) {}

bool AimModel::move(double dx, double dy, int width, int height) {
    const auto result = nexus_aim_move_xy(center.x(), center.y(), point.x(), point.y(),
                                          gainX, gainY, dx, dy, width, height);
    point = QPointF(result.x, result.y);
    return result.reset != 0;
}
