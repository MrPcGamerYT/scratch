#include "aimcontroller.h"
#include "qmp.h"
#include <QTimer>
#include <QtGlobal>

AimController *AimController::instance = nullptr;
AimController::AimController(Qmp *qmp, QObject *parent) : QObject(parent), monitor(qmp), timer(new QTimer(this)) {
    timer->setInterval(16);
    connect(timer, &QTimer::timeout, this, [this] {
        if (active && (!focused() || !monitor->isReady())) { stop(); return; }
        if (active) {
            RECT r{}; POINT top{0,0};
            if (!GetClientRect(guest, &r) || !ClientToScreen(guest, &top) ||
                top.x != lockedBounds.left || top.y != lockedBounds.top ||
                top.x + r.right != lockedBounds.right || top.y + r.bottom != lockedBounds.bottom) { stop(); return; }
        }
        if (active && !firing) flushMove();
    });
}
AimController::~AimController() { disarm(); }
bool AimController::focused() const {
    if (!guest || !IsWindow(guest) || GetAncestor(guest, GA_ROOT) != GetForegroundWindow()) return false;
    GUITHREADINFO info{}; info.cbSize = sizeof(info);
    DWORD thread = GetWindowThreadProcessId(guest, nullptr);
    return thread && GetGUIThreadInfo(thread, &info) && info.hwndFocus && (info.hwndFocus == guest || IsChild(guest, info.hwndFocus));
}
bool AimController::arm(HWND window, QPointF anchor, QPointF fire, double sensitivity) {
    disarm();
    if (instance || !window || !IsWindow(window) || !monitor->isReady()) return false;
    guest = window; model = AimModel(anchor, sensitivity);
    firePoint = QPointF(qBound(0.0, fire.x(), 1.0), qBound(0.0, fire.y(), 1.0));
    instance = this;
    keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, keyboardProc, nullptr, 0);
    if (!keyboard) { instance = nullptr; guest = nullptr; return false; }
    emit statusChanged(true, false, "Aim armed: click the Android screen and press F8. F8 releases the mouse.");
    return true;
}
void AimController::disarm() {
    stop();
    if (!keyboard) return;
    UnhookWindowsHookEx(keyboard); keyboard = nullptr; guest = nullptr; f8Held = false;
    if (instance == this) instance = nullptr;
    emit statusChanged(false, false, "Aim mode off.");
}
bool AimController::centerCursor() {
    RECT rect{}; if (!guest || !GetClientRect(guest, &rect) || rect.right <= 0 || rect.bottom <= 0) return false;
    POINT top{0,0}, bottom{rect.right, rect.bottom};
    if (!ClientToScreen(guest, &top) || !ClientToScreen(guest, &bottom)) return false;
    RECT bounds{top.x, top.y, bottom.x, bottom.y};
    if (!ClipCursor(&bounds)) return false;
    lockedBounds = bounds;
    cursorCenter = POINT{top.x + (bottom.x - top.x) / 2, top.y + (bottom.y - top.y) / 2};
    return SetCursorPos(cursorCenter.x, cursorCenter.y) != 0;
}
bool AimController::start() {
    if (!focused() || !monitor->isReady()) return false;
    mouse = SetWindowsHookExW(WH_MOUSE_LL, mouseProc, nullptr, 0);
    if (!mouse) return false;
    active = true; firing = false; pendingX = pendingY = 0; model.reset();
    if (!centerCursor()) { stop(); return false; }
    monitor->pointerDown(model.anchor().x(), model.anchor().y());
    timer->start();
    emit statusChanged(true, true, "Aim active: move mouse; left click fires. Press F8 to release.");
    return true;
}
void AimController::stop() {
    if (!active && !mouse) return;
    timer->stop();
    if (active) monitor->pointerUp();
    active = false; firing = false; pendingX = pendingY = 0;
    if (mouse) { UnhookWindowsHookEx(mouse); mouse = nullptr; }
    ClipCursor(nullptr);
    emit statusChanged(keyboard != nullptr, false, "Aim released. Focus Android and press F8 to resume.");
}
void AimController::flushMove() {
    if (!pendingX && !pendingY) return;
    RECT r{}; if (!GetClientRect(guest, &r) || r.right <= 0 || r.bottom <= 0) { stop(); return; }
    const int dx = pendingX, dy = pendingY; pendingX = pendingY = 0;
    if (model.move(dx, dy, r.right, r.bottom)) {
        monitor->pointerUp(); monitor->pointerDown(model.anchor().x(), model.anchor().y());
    } else monitor->pointerMove(model.position().x(), model.position().y());
}
LRESULT CALLBACK AimController::keyboardProc(int code, WPARAM msg, LPARAM data) {
    auto *self = instance;
    if (code == HC_ACTION && self) {
        const auto *key = reinterpret_cast<const KBDLLHOOKSTRUCT *>(data);
        if (!(key->flags & LLKHF_INJECTED) && key->vkCode == VK_F8 && (self->f8Held || self->active || self->focused())) {
            if (msg == WM_KEYUP || msg == WM_SYSKEYUP) { self->f8Held = false; return 1; }
            if (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN) {
                if (!self->f8Held) { self->f8Held = true; if (self->active) self->stop(); else if (!self->start()) emit self->statusChanged(true, false, "Could not activate aim. Focus the Android display and try F8 again."); }
                return 1;
            }
        }
    }
    return CallNextHookEx(nullptr, code, msg, data);
}
LRESULT CALLBACK AimController::mouseProc(int code, WPARAM msg, LPARAM data) {
    auto *self = instance;
    if (code != HC_ACTION || !self || !self->active) return CallNextHookEx(nullptr, code, msg, data);
    if (!self->focused()) { self->stop(); return CallNextHookEx(nullptr, code, msg, data); }
    const auto *event = reinterpret_cast<const MSLLHOOKSTRUCT *>(data);
    if (msg == WM_MOUSEMOVE) {
        int dx = event->pt.x - self->cursorCenter.x, dy = event->pt.y - self->cursorCenter.y;
        if (!(event->flags & LLMHF_INJECTED) && (dx || dy) && !self->firing) {
            self->pendingX = qBound(-4096, self->pendingX + dx, 4096);
            self->pendingY = qBound(-4096, self->pendingY + dy, 4096);
        }
        if ((dx || dy) && !SetCursorPos(self->cursorCenter.x, self->cursorCenter.y)) self->stop();
        return 1;
    }
    if (event->flags & LLMHF_INJECTED) return 1;
    if (msg == WM_LBUTTONDOWN && !self->firing) {
        self->pendingX = self->pendingY = 0; self->monitor->pointerUp();
        self->monitor->pointerDown(self->firePoint.x(), self->firePoint.y()); self->firing = true; return 1;
    }
    if (msg == WM_LBUTTONUP && self->firing) {
        self->monitor->pointerUp(); self->firing = false; self->model.reset();
        self->monitor->pointerDown(self->model.anchor().x(), self->model.anchor().y()); return 1;
    }
    // Avoid a second host mouse button corrupting our single-pointer gesture.
    return 1;
}
