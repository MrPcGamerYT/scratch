#include "mouselock.h"
#include <QTimer>

MouseLock *MouseLock::instance = nullptr;
MouseLock::MouseLock(QObject *parent) : QObject(parent), timer(new QTimer(this)) {
    timer->setInterval(100);
    connect(timer, &QTimer::timeout, this, [this] {
        if (!window || !IsWindow(window) || GetForegroundWindow() != GetAncestor(window, GA_ROOT) || !updateClip()) unlock();
    });
}
MouseLock::~MouseLock() { unlock(); }
bool MouseLock::lock(HWND guest) {
    unlock();
    if (instance || !guest || !IsWindow(guest) || GetForegroundWindow() != GetAncestor(guest, GA_ROOT)) return false;
    window = guest; instance = this;
    hook = SetWindowsHookExW(WH_KEYBOARD_LL, &MouseLock::hookProc, nullptr, 0);
    if (!hook) { instance = nullptr; window = nullptr; return false; }
    if (!updateClip()) { unlock(); return false; }
    timer->start(); return true;
}
void MouseLock::unlock() {
    if (!hook) return;
    timer->stop(); ClipCursor(nullptr); UnhookWindowsHookEx(hook);
    hook = nullptr; window = nullptr;
    if (instance == this) instance = nullptr;
    emit unlocked();
}
bool MouseLock::updateClip() {
    RECT rect{};
    if (!window || !GetClientRect(window, &rect) || rect.right <= 0 || rect.bottom <= 0) return false;
    POINT top{rect.left, rect.top}, bottom{rect.right, rect.bottom};
    if (!ClientToScreen(window, &top) || !ClientToScreen(window, &bottom)) return false;
    RECT screen{top.x, top.y, bottom.x, bottom.y};
    return ClipCursor(&screen) != 0;
}
LRESULT CALLBACK MouseLock::hookProc(int code, WPARAM message, LPARAM data) {
    auto *self = instance;
    if (code == HC_ACTION && self && (message == WM_KEYDOWN || message == WM_SYSKEYDOWN)) {
        const auto *event = reinterpret_cast<const KBDLLHOOKSTRUCT *>(data);
        if (!(event->flags & LLKHF_INJECTED) && event->vkCode == 'L' &&
            (GetAsyncKeyState(VK_CONTROL) & 0x8000) && (GetAsyncKeyState(VK_MENU) & 0x8000)) {
            self->unlock(); return 1;
        }
    }
    return CallNextHookEx(nullptr, code, message, data);
}
