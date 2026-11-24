#pragma once
#include <cstddef>
#include <cstdint>

// Stable C ABI shared with rust/nexus-core. No Qt or C++ types cross it.
extern "C" {
struct NexusBinding {
    std::int32_t key;
    double x;
    double y;
    std::uint8_t repeat;
    std::uint8_t burst_count;
    std::uint16_t reserved;
    std::int32_t interval_ms;
};
struct NexusAimResult {
    double x;
    double y;
    std::uint32_t reset;
};
struct NexusPoint {
    double x;
    double y;
};
struct NexusPinchPair {
    double x1;
    double y1;
    double x2;
    double y2;
};
struct NexusScriptStep {
    std::uint8_t op;
    std::uint8_t reserved[3];
    std::int32_t duration_ms;
    double x1;
    double y1;
    double x2;
    double y2;
};

std::uint8_t nexus_valid_key(std::int32_t key);
std::uint32_t nexus_validate_bindings(const NexusBinding *bindings, std::size_t length);
double nexus_aim_clamp_anchor(double value);
double nexus_aim_clamp_sensitivity(double value);
NexusAimResult nexus_aim_move(double anchor_x, double anchor_y, double x, double y, double gain,
                              double dx, double dy, std::int32_t width, std::int32_t height);
NexusAimResult nexus_aim_move_xy(double anchor_x, double anchor_y, double x, double y,
                                 double gain_x, double gain_y, double dx, double dy,
                                 std::int32_t width, std::int32_t height);
NexusPoint nexus_dpad_target(double center_x, double center_y, double radius,
                             std::uint8_t up, std::uint8_t down, std::uint8_t left, std::uint8_t right,
                             double modifier_scale);
NexusPoint nexus_swipe_point(double x1, double y1, double x2, double y2, double progress);
NexusPinchPair nexus_pinch_points(double center_x, double center_y, double radius,
                                  double progress, std::uint8_t zoom_in);
NexusPoint nexus_moba_target(double skill_x, double skill_y, double radius,
                             double origin_x, double origin_y, double cursor_x, double cursor_y);
std::uint32_t nexus_validate_script_steps(const NexusScriptStep *steps, std::size_t length);
// Caller owns UTF-8 NUL-terminated strings and output buffer for each call.
// flags: 1 multitouch, 2 hide cursor, 4 local ADB, 8 explicitly selected TCG.
struct NexusVmRequest {
    const char *disk;
    const char *firmware_dir;
    const char *uefi_code;
    const char *uefi_vars;
    std::uint32_t memory_mb, cpus, adb_port, flags;
};
std::size_t nexus_vm_plan(const NexusVmRequest *request, std::uint8_t *output, std::size_t capacity);
std::int32_t nexus_whpx_available();
}
