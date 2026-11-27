#pragma once
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <functional>

// Process-local raw-mouse receiver; never consumes keyboard input or registers
// RIDEV_NOLEGACY. AimController separately owns focus checks and capture hooks.
class RawMouseInput {
public:
    ~RawMouseInput() { stop(); }
    bool start(std::function<void(int, int)> receiver);
    void stop();
    bool isActive() const { return window != nullptr; }
private:
    static LRESULT CALLBACK procedure(HWND hwnd, UINT message, WPARAM wparam, LPARAM lparam);
    HWND window = nullptr;
    std::function<void(int, int)> receive;
};
