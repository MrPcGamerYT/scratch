#include "inputmapper.h"
#include "qmp.h"
#include "clicksequence.h"
#include <QTimer>

InputMapper *InputMapper::instance = nullptr;
InputMapper::InputMapper(Qmp *monitor, QObject *parent) : QObject(parent), qmp(monitor), focusTimer(new QTimer(this)), sequence(new ClickSequence(this)) {
    connect(sequence, &ClickSequence::pointerDown, qmp, &Qmp::pointerDown);
    connect(sequence, &ClickSequence::pointerUp, qmp, &Qmp::pointerUp);
    focusTimer->setInterval(100);
    connect(focusTimer, &QTimer::timeout, this, [this] { if ((!heldKeys.isEmpty() || sequence->active()) && !viewportFocused()) release(); });
}
InputMapper::~InputMapper() { stop(); }
bool InputMapper::start(HWND window, const QList<KeyBinding> &map) {
    stop();
    if (!window || !IsWindow(window) || !qmp->isReady() || map.isEmpty() || instance) return false;
    guest = window; bindings = map; instance = this;
    hook = SetWindowsHookExW(WH_KEYBOARD_LL, &InputMapper::hookProc, nullptr, 0);
    if (!hook) { instance = nullptr; guest = nullptr; return false; }
    focusTimer->start(); return true;
}
void InputMapper::release() { sequence->cancel(); heldKeys.clear(); }
void InputMapper::stop() {
    release(); focusTimer->stop();
    if (hook) UnhookWindowsHookEx(hook);
    hook = nullptr; guest = nullptr; bindings.clear();
    if (instance == this) instance = nullptr;
}
bool InputMapper::viewportFocused() const {
    if (!guest || !IsWindow(guest) || GetAncestor(guest, GA_ROOT) != GetForegroundWindow()) return false;
    GUITHREADINFO info{}; info.cbSize = sizeof(info);
    const DWORD thread = GetWindowThreadProcessId(guest, nullptr);
    return thread && GetGUIThreadInfo(thread, &info) && info.hwndFocus && (info.hwndFocus == guest || IsChild(guest, info.hwndFocus));
}
LRESULT CALLBACK InputMapper::hookProc(int code, WPARAM message, LPARAM data) {
    auto *self = instance;
    if (code != HC_ACTION || !self) return CallNextHookEx(nullptr, code, message, data);
    const auto *event = reinterpret_cast<const KBDLLHOOKSTRUCT *>(data);
    if (event->flags & LLKHF_INJECTED) return CallNextHookEx(nullptr, code, message, data);
    const bool down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    const bool up = message == WM_KEYUP || message == WM_SYSKEYUP;
    if (!down && !up) return CallNextHookEx(nullptr, code, message, data);
    const int key = static_cast<int>(event->vkCode);
    if (up && self->heldKeys.remove(key)) { self->sequence->release(key); return 1; }
    if (up) return CallNextHookEx(nullptr, code, message, data);
    if (!self->viewportFocused()) { if (!self->heldKeys.isEmpty() || self->sequence->active()) self->release(); return CallNextHookEx(nullptr, code, message, data); }
    if ((GetAsyncKeyState(VK_CONTROL) & 0x8000) || (GetAsyncKeyState(VK_MENU) & 0x8000) ||
        (GetAsyncKeyState(VK_LWIN) & 0x8000) || (GetAsyncKeyState(VK_RWIN) & 0x8000))
        return CallNextHookEx(nullptr, code, message, data);
    for (const auto &binding : self->bindings) if (binding.key == key) {
        if (down && !self->heldKeys.contains(key)) {
            self->heldKeys.insert(key);
            self->sequence->press(key, self->bindings);
        }
        return 1;
    }
    return CallNextHookEx(nullptr, code, message, data);
}
