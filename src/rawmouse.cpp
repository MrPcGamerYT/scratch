#include "rawmouse.h"
#include <vector>
#include <utility>

namespace { constexpr wchar_t windowClass[] = L"NexusRawMouseReceiver"; }
bool RawMouseInput::start(std::function<void(int, int)> receiver) {
    stop();
    // Do not steal another library's mouse registration within this process.
    UINT count = 0;
    if (GetRegisteredRawInputDevices(nullptr, &count, sizeof(RAWINPUTDEVICE)) == UINT(-1)) return false;
    std::vector<RAWINPUTDEVICE> devices(count);
    if (count && GetRegisteredRawInputDevices(devices.data(), &count, sizeof(RAWINPUTDEVICE)) == UINT(-1)) return false;
    for (const auto &device : devices) if (device.usUsagePage == 1 && device.usUsage == 2) return false;
    WNDCLASSW cls{}; cls.lpfnWndProc = procedure; cls.hInstance = GetModuleHandleW(nullptr); cls.lpszClassName = windowClass;
    if (!RegisterClassW(&cls) && GetLastError() != ERROR_CLASS_ALREADY_EXISTS) return false;
    receive = std::move(receiver);
    window = CreateWindowExW(0, windowClass, L"", 0, 0, 0, 0, 0, HWND_MESSAGE, nullptr, cls.hInstance, this);
    if (!window) { receive = {}; return false; }
    RAWINPUTDEVICE mouse{1, 2, RIDEV_INPUTSINK, window};
    if (!RegisterRawInputDevices(&mouse, 1, sizeof(mouse))) { stop(); return false; }
    return true;
}
void RawMouseInput::stop() {
    if (!window) return;
    UINT count = 0;
    if (GetRegisteredRawInputDevices(nullptr, &count, sizeof(RAWINPUTDEVICE)) != UINT(-1)) {
        std::vector<RAWINPUTDEVICE> devices(count);
        if (!count || GetRegisteredRawInputDevices(devices.data(), &count, sizeof(RAWINPUTDEVICE)) != UINT(-1)) {
            for (const auto &device : devices) if (device.usUsagePage == 1 && device.usUsage == 2 && device.hwndTarget == window) {
                RAWINPUTDEVICE remove{1, 2, RIDEV_REMOVE, nullptr}; RegisterRawInputDevices(&remove, 1, sizeof(remove));
            }
        }
    }
    SetWindowLongPtrW(window, GWLP_USERDATA, 0);
    DestroyWindow(window); window = nullptr; receive = {};
}
LRESULT CALLBACK RawMouseInput::procedure(HWND hwnd, UINT message, WPARAM wparam, LPARAM lparam) {
    if (message == WM_NCCREATE) {
        auto *create = reinterpret_cast<CREATESTRUCTW *>(lparam);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(create->lpCreateParams));
    }
    auto *self = reinterpret_cast<RawMouseInput *>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    if (message == WM_INPUT && self) {
        RAWINPUT input{}; UINT size = sizeof(input);
        const UINT read = GetRawInputData(reinterpret_cast<HRAWINPUT>(lparam), RID_INPUT, &input, &size, sizeof(RAWINPUTHEADER));
        if (read != UINT(-1) && read >= sizeof(RAWINPUTHEADER) + sizeof(RAWMOUSE) && input.header.dwType == RIM_TYPEMOUSE && input.header.hDevice != nullptr &&
            !(input.data.mouse.usFlags & MOUSE_MOVE_ABSOLUTE) && self->receive) {
            self->receive(input.data.mouse.lLastX, input.data.mouse.lLastY);
        }
    }
    // DefWindowProc performs foreground raw-input cleanup.
    return DefWindowProcW(hwnd, message, wparam, lparam);
}
