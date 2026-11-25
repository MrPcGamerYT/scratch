#include "rawmouse.h"
#include <iostream>
int main() {
    RawMouseInput input;
    bool called = false;
    if (!input.start([&](int, int) { called = true; })) {
        std::cerr << "Could not register raw mouse receiver: " << GetLastError() << '\n'; return 1;
    }
    if (!input.isActive()) return 2;
    // A second consumer must not steal the first receiver's registration.
    RawMouseInput second;
    if (second.start([](int, int) {})) return 3;
    input.stop(); input.stop();
    if (input.isActive() || called) return 4;
    if (!second.start([](int, int) {})) return 5;
    second.stop();
    std::cout << "Raw mouse registration ownership and cleanup passed; no physical mouse latency test.\n";
}
