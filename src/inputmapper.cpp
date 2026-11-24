#include "inputmapper.h"
#include "keymapengine.h"
#include "qmp.h"
#include <QTimer>
#include <algorithm>

InputMapper *InputMapper::instance = nullptr;

InputMapper::InputMapper(Qmp *monitor, QObject *parent)
    : QObject(parent), qmp(monitor), engine(new KeymapEngine(this)), focusTimer(new QTimer(this)) {
    connect(engine, &KeymapEngine::pointerDown, qmp, &Qmp::pointerDown);
    connect(engine, &KeymapEngine::pointerMove, qmp, &Qmp::pointerMove);
    connect(engine, &KeymapEngine::pointerUp, qmp, &Qmp::pointerUp);
    connect(engine, &KeymapEngine::touchDown, qmp, &Qmp::touchDown);
    connect(engine, &KeymapEngine::touchMove, qmp, &Qmp::touchMove);
    connect(engine, &KeymapEngine::touchUp, qmp, &Qmp::touchUp);

    focusTimer->setInterval(100);
    connect(focusTimer, &QTimer::timeout, this, [this] {
        if ((!heldKeys.isEmpty() || !heldMouseButtons.isEmpty() || engine->active()) && !viewportFocused())
            release();
    });
}

InputMapper::~InputMapper() { stop(); }

void InputMapper::setReservedTouchSlots(const QSet<int> &slotsReserved) {
    engine->setReservedTouchSlots(slotsReserved);
}

bool InputMapper::start(HWND window, const QList<KeyBinding> &bindings, bool multitouch) {
    KeymapProfile profile;
    profile.taps = bindings;
    return start(window, profile, multitouch);
}

bool InputMapper::start(HWND window, const KeymapProfile &profile, bool multitouch) {
    stop();
    if (!window || !IsWindow(window) || !qmp->isReady() || !profile.hasAnyInputBindings() || instance)
        return false;

    guest = window;
    engine->configure(profile, multitouch);
    instance = this;

    keyHook = SetWindowsHookExW(WH_KEYBOARD_LL, &InputMapper::keyHookProc, nullptr, 0);
    if (!keyHook) {
        instance = nullptr;
        guest = nullptr;
        return false;
    }

    bool needsMouseHook = !profile.mobaSkills.isEmpty();
    if (!needsMouseHook) {
        for (const auto &t : profile.taps) if (isMouseBindingKey(t.key)) needsMouseHook = true;
        for (const auto &sw : profile.swipes) if (isMouseBindingKey(sw.key)) needsMouseHook = true;
        for (const auto &z : profile.zooms) if (isMouseBindingKey(z.keyIn) || isMouseBindingKey(z.keyOut)) needsMouseHook = true;
        for (const auto &sc : profile.scripts) if (isMouseBindingKey(sc.key)) needsMouseHook = true;
    }
    if (needsMouseHook) {
        mouseHook = SetWindowsHookExW(WH_MOUSE_LL, &InputMapper::mouseHookProc, nullptr, 0);
    }

    focusTimer->start();
    return true;
}

void InputMapper::release() {
    engine->cancelAll();
    heldKeys.clear();
    heldMouseButtons.clear();
}

void InputMapper::stop() {
    release();
    focusTimer->stop();
    if (mouseHook) { UnhookWindowsHookEx(mouseHook); mouseHook = nullptr; }
    if (keyHook) { UnhookWindowsHookEx(keyHook); keyHook = nullptr; }
    guest = nullptr;
    aimCapturingMouse = false;
    if (instance == this) instance = nullptr;
}

bool InputMapper::viewportFocused() const {
    if (!guest || !IsWindow(guest) || GetAncestor(guest, GA_ROOT) != GetForegroundWindow()) return false;
    GUITHREADINFO info{}; info.cbSize = sizeof(info);
    const DWORD thread = GetWindowThreadProcessId(guest, nullptr);
    return thread && GetGUIThreadInfo(thread, &info) && info.hwndFocus &&
           (info.hwndFocus == guest || IsChild(guest, info.hwndFocus));
}

bool InputMapper::cursorNormalized(POINT screenPt, QPointF *outNorm) const {
    RECT rect{};
    if (!guest || !IsWindow(guest) || !GetClientRect(guest, &rect) || rect.right <= 0 || rect.bottom <= 0)
        return false;
    POINT top{0, 0}, bottom{rect.right, rect.bottom};
    if (!ClientToScreen(guest, &top) || !ClientToScreen(guest, &bottom)) return false;
    if (screenPt.x < top.x || screenPt.x > bottom.x || screenPt.y < top.y || screenPt.y > bottom.y)
        return false;
    if (outNorm) {
        *outNorm = QPointF(
            std::clamp(static_cast<double>(screenPt.x - top.x) / (bottom.x - top.x), 0.0, 1.0),
            std::clamp(static_cast<double>(screenPt.y - top.y) / (bottom.y - top.y), 0.0, 1.0));
    }
    return true;
}

QPointF InputMapper::currentCursorNorm() const {
    POINT pt{};
    QPointF norm(0.5, 0.5);
    if (GetCursorPos(&pt)) cursorNormalized(pt, &norm);
    return norm;
}

LRESULT CALLBACK InputMapper::keyHookProc(int code, WPARAM message, LPARAM data) {
    auto *self = instance;
    if (code != HC_ACTION || !self) return CallNextHookEx(nullptr, code, message, data);
    const auto *event = reinterpret_cast<const KBDLLHOOKSTRUCT *>(data);
    if (event->flags & LLKHF_INJECTED) return CallNextHookEx(nullptr, code, message, data);
    const bool down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    const bool up = message == WM_KEYUP || message == WM_SYSKEYUP;
    if (!down && !up) return CallNextHookEx(nullptr, code, message, data);

    const int key = static_cast<int>(event->vkCode);
    if (up) {
        const bool wasHeld = self->heldKeys.remove(key);
        const bool released = self->engine->handleKeyRelease(key);
        if (wasHeld || released) return 1;
        return CallNextHookEx(nullptr, code, message, data);
    }

    if (!self->viewportFocused()) {
        if (!self->heldKeys.isEmpty() || !self->heldMouseButtons.isEmpty() || self->engine->active())
            self->release();
        return CallNextHookEx(nullptr, code, message, data);
    }

    const bool ctrl = (GetAsyncKeyState(VK_CONTROL) & 0x8000) != 0;
    const bool alt = (GetAsyncKeyState(VK_MENU) & 0x8000) != 0;
    const bool win = ((GetAsyncKeyState(VK_LWIN) & 0x8000) != 0) || ((GetAsyncKeyState(VK_RWIN) & 0x8000) != 0);
    // Never intercept Windows key combos, Ctrl+Alt combos (e.g. Ctrl+Alt+L unlock), Alt+Tab, or Alt+F4.
    if (win || (ctrl && alt) || (alt && (key == VK_TAB || key == VK_F4)) || key == VK_LWIN || key == VK_RWIN || key == VK_ESCAPE)
        return CallNextHookEx(nullptr, code, message, data);

    if (self->heldKeys.contains(key)) return 1;
    if (self->engine->handleKeyPress(key, self->currentCursorNorm())) {
        self->heldKeys.insert(key);
        return 1;
    }
    return CallNextHookEx(nullptr, code, message, data);
}

LRESULT CALLBACK InputMapper::mouseHookProc(int code, WPARAM message, LPARAM data) {
    auto *self = instance;
    if (code != HC_ACTION || !self || self->aimCapturingMouse)
        return CallNextHookEx(nullptr, code, message, data);

    const auto *event = reinterpret_cast<const MSLLHOOKSTRUCT *>(data);
    if (event->flags & LLMHF_INJECTED) return CallNextHookEx(nullptr, code, message, data);

    if (message == WM_MOUSEMOVE) {
        QPointF norm;
        if (self->viewportFocused() && self->cursorNormalized(event->pt, &norm)) {
            self->engine->handleCursorMove(norm);
        }
        return CallNextHookEx(nullptr, code, message, data);
    }

    int buttonKey = 0;
    bool down = false;
    bool up = false;
    bool wheel = false;

    switch (message) {
    case WM_RBUTTONDOWN: buttonKey = MouseRightButton; down = true; break;
    case WM_RBUTTONUP:   buttonKey = MouseRightButton; up = true; break;
    case WM_MBUTTONDOWN: buttonKey = MouseMiddleButton; down = true; break;
    case WM_MBUTTONUP:   buttonKey = MouseMiddleButton; up = true; break;
    case WM_XBUTTONDOWN:
        buttonKey = (HIWORD(event->mouseData) == XBUTTON2) ? MouseX2Button : MouseX1Button;
        down = true;
        break;
    case WM_XBUTTONUP:
        buttonKey = (HIWORD(event->mouseData) == XBUTTON2) ? MouseX2Button : MouseX1Button;
        up = true;
        break;
    case WM_MOUSEWHEEL: {
        const short delta = static_cast<short>(HIWORD(event->mouseData));
        if (delta != 0) {
            buttonKey = delta > 0 ? MouseWheelUp : MouseWheelDown;
            wheel = true;
        }
        break;
    }
    default:
        return CallNextHookEx(nullptr, code, message, data);
    }

    if (up && self->heldMouseButtons.remove(buttonKey)) {
        self->engine->handleKeyRelease(buttonKey);
        return 1;
    }
    if (!down && !wheel) return CallNextHookEx(nullptr, code, message, data);

    QPointF norm;
    if (!self->viewportFocused() || !self->cursorNormalized(event->pt, &norm))
        return CallNextHookEx(nullptr, code, message, data);

    if (wheel) {
        if (self->engine->handleWheel(buttonKey, norm)) return 1;
        return CallNextHookEx(nullptr, code, message, data);
    }

    if (self->engine->handleKeyPress(buttonKey, norm)) {
        self->heldMouseButtons.insert(buttonKey);
        return 1;
    }
    return CallNextHookEx(nullptr, code, message, data);
}
