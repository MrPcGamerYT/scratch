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
    std::int32_t interval_ms;
};
struct NexusAimResult {
    double x;
    double y;
    std::uint32_t reset;
};
std::uint8_t nexus_valid_key(std::int32_t key);
std::uint32_t nexus_validate_bindings(const NexusBinding *bindings, std::size_t length);
double nexus_aim_clamp_anchor(double value);
double nexus_aim_clamp_sensitivity(double value);
NexusAimResult nexus_aim_move(double anchor_x, double anchor_y, double x, double y, double gain,
                              double dx, double dy, std::int32_t width, std::int32_t height);
}
