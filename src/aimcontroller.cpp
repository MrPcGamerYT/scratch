#include "aimcontroller.h"
#include "qmp.h"
#include <QTimer>
#include <QtGlobal>
#include <QSettings>
#include "rustcore.h"

AimController *AimController::instance = nullptr;

AimController::AimController(Qmp *qmp, QObject *parent)
    : QObject(parent), monitor(qmp), timer(new QTimer(this)) {
    timer->setInterval(16);
    timer->setTimerType(Qt::PreciseTimer);
    connect(timer, &QTimer::timeout, this, [this] {
        if (active && (!focused() || !monitor->isReady())) { stop(); return; }
        if (active && !suspended) {
            RECT r{}; POINT top{0, 0};
            if (!GetClientRect(guest, &r) || !ClientToScreen(guest, &top) ||
                top.x != lockedBounds.left || top.y != lockedBounds.top ||
                top.x + r.right != lockedBounds.right || top.y + r.bottom != lockedBounds.bottom) {
                stop();
                return;
            }
            if (!firing || touchMode) flushMove();
        }
    });
}

AimController::~AimController() { disarm(); }

void AimController::notifyState(bool armedState, bool activeState, bool suspendedState, const QString &msg) {
    emit statusChanged(armedState, activeState, msg);
    emit stateChanged(armedState, activeState, suspendedState, msg);
}

bool AimController::focused() const {
    if (!guest || !IsWindow(guest) || GetAncestor(guest, GA_ROOT) != GetForegroundWindow()) return false;
    GUITHREADINFO info{}; info.cbSize = sizeof(info);
    DWORD thread = GetWindowThreadProcessId(guest, nullptr);
    return thread && GetGUIThreadInfo(thread, &info) && info.hwndFocus &&
           (info.hwndFocus == guest || IsChild(guest, info.hwndFocus));
}

bool AimController::arm(HWND window, QPointF anchor, QPointF fire, double sensitivity, bool multitouch) {
    AimConfig simple;
    simple.enabled = true;
    simple.anchorX = anchor.x();
    simple.anchorY = anchor.y();
    simple.fireEnabled = true;
    simple.fireX = fire.x();
    simple.fireY = fire.y();
    simple.sensitivityX = sensitivity;
    simple.sensitivityY = sensitivity;
    return arm(window, simple, multitouch);
}

bool AimController::arm(HWND window, const AimConfig &config, bool multitouch) {
    disarm();
    if (instance || !window || !IsWindow(window) || !monitor->isReady()) return false;
    guest = window;
    cfg = config;
    cfg.fireX = qBound(0.0, cfg.fireX, 1.0);
    cfg.fireY = qBound(0.0, cfg.fireY, 1.0);
    cfg.adsX = qBound(0.0, cfg.adsX, 1.0);
    cfg.adsY = qBound(0.0, cfg.adsY, 1.0);
    cfg.freeLookX = qBound(0.1, cfg.freeLookX, 0.9);
    cfg.freeLookY = qBound(0.1, cfg.freeLookY, 0.9);
    model = AimModel(QPointF(cfg.anchorX, cfg.anchorY), cfg.sensitivityX, cfg.sensitivityY);
    freeLookModel = AimModel(QPointF(cfg.freeLookX, cfg.freeLookY), cfg.sensitivityX, cfg.sensitivityY);
    touchMode = multitouch;
    instance = this;
    keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, keyboardProc, nullptr, 0);
    if (!keyboard) { instance = nullptr; guest = nullptr; return false; }
    const QString toggleLabel = keyName(cfg.toggleKey);
    notifyState(true, false, false,
                QStringLiteral("Aim armed: click Android and press %1 to lock mouse (%1 again releases).").arg(toggleLabel));
    return true;
}

void AimController::disarm() {
    stop();
    if (!keyboard) return;
    UnhookWindowsHookEx(keyboard);
    keyboard = nullptr;
    guest = nullptr;
    toggleHeld = false;
    suspendHeld = false;
    if (instance == this) instance = nullptr;
    notifyState(false, false, false, QStringLiteral("Aim mode off."));
}

bool AimController::centerCursor() {
    RECT rect{};
    if (!guest || !GetClientRect(guest, &rect) || rect.right <= 0 || rect.bottom <= 0) return false;
    POINT top{0, 0}, bottom{rect.right, rect.bottom};
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
    const QSettings settings;
    useRawMouse = settings.value("input/rawMouse", false).toBool();
    timer->setInterval(static_cast<int>(nexus_input_interval(settings.value("input/aimInterval", 16).toUInt())));
    if (useRawMouse && !rawMouse.start([this](int dx, int dy) {
        if (active && !suspended && focused() && (!firing || touchMode)) queueMove(dx, dy);
    })) {
        UnhookWindowsHookEx(mouse); mouse = nullptr;
        notifyState(true, false, false, "Raw mouse registration failed or is owned by another component. Choose compatibility input in Settings → Game settings.");
        return false;
    }
    receivedMoves = sentBatches = 0; maximumBatchWait = 0; pendingAge.invalidate();
    active = true;
    suspended = false;
    freeLooking = false;
    firing = false;
    adsHeld = false;
    pendingX = pendingY = 0;
    model.reset();
    freeLookModel.reset();
    if (!centerCursor()) { stop(); return false; }
    if (touchMode) monitor->touchDown(0, model.anchor().x(), model.anchor().y());
    else monitor->pointerDown(model.anchor().x(), model.anchor().y());
    timer->start();
    QString hint = QStringLiteral("Aim active: move mouse%1%2. Press %3 to release.")
        .arg(cfg.fireEnabled ? QStringLiteral(", LMB fires") : QString())
        .arg(cfg.adsEnabled ? QStringLiteral(", RMB ADS") : QString())
        .arg(keyName(cfg.toggleKey));
    if (cfg.suspendKey != 0) hint += QStringLiteral(" Hold %1 for cursor.").arg(keyName(cfg.suspendKey));
    hint += QString(" Input: %1, %2 ms batching.").arg(useRawMouse ? "raw mouse" : "compatibility").arg(timer->interval());
    notifyState(true, true, false, hint);
    return true;
}

void AimController::enterSuspend() {
    if (!active || suspended) return;
    suspended = true;
    pendingX = pendingY = 0;
    if (touchMode) {
        monitor->touchUp(0);
        if (firing) { monitor->touchUp(1); firing = false; }
        if (adsHeld) { monitor->touchUp(2); adsHeld = false; }
    } else {
        monitor->pointerUp();
        firing = false;
        adsHeld = false;
    }
    freeLooking = false;
    ClipCursor(nullptr);
    notifyState(true, true, true,
                QStringLiteral("Aim cursor free while %1 is held.").arg(keyName(cfg.suspendKey)));
}

void AimController::exitSuspend() {
    if (!active || !suspended) return;
    if (!focused() || !centerCursor()) { stop(); return; }
    suspended = false;
    pendingX = pendingY = 0;
    model.reset();
    if (touchMode) monitor->touchDown(0, model.anchor().x(), model.anchor().y());
    else monitor->pointerDown(model.anchor().x(), model.anchor().y());
    notifyState(true, true, false,
                QStringLiteral("Aim resumed (%1 releases).").arg(keyName(cfg.toggleKey)));
}

void AimController::setFreeLookActive(bool on) {
    if (!active || suspended || !cfg.freeLookEnabled || freeLooking == on) return;
    freeLooking = on;
    pendingX = pendingY = 0;
    if (touchMode) {
        monitor->touchUp(0);
        if (on) {
            freeLookModel.reset();
            monitor->touchDown(0, freeLookModel.anchor().x(), freeLookModel.anchor().y());
        } else {
            model.reset();
            monitor->touchDown(0, model.anchor().x(), model.anchor().y());
        }
    } else if (!firing) {
        monitor->pointerUp();
        if (on) {
            freeLookModel.reset();
            monitor->pointerDown(freeLookModel.anchor().x(), freeLookModel.anchor().y());
        } else {
            model.reset();
            monitor->pointerDown(model.anchor().x(), model.anchor().y());
        }
    }
}

void AimController::triggerAdsDown() {
    if (!cfg.adsEnabled) return;
    if (touchMode) {
        if (cfg.adsHoldMode) {
            if (!adsHeld) {
                monitor->touchDown(2, cfg.adsX, cfg.adsY);
                adsHeld = true;
            }
        } else {
            monitor->touchDown(2, cfg.adsX, cfg.adsY);
            QTimer::singleShot(45, this, [this] { if (touchMode) monitor->touchUp(2); });
        }
    } else if (!firing) {
        pendingX = pendingY = 0;
        monitor->pointerUp();
        monitor->pointerDown(cfg.adsX, cfg.adsY);
        adsHeld = true;
        if (!cfg.adsHoldMode) {
            QTimer::singleShot(45, this, [this] { triggerAdsUp(); });
        }
    }
}

void AimController::triggerAdsUp() {
    if (!adsHeld) return;
    adsHeld = false;
    if (touchMode) {
        monitor->touchUp(2);
    } else if (active && !suspended && !firing) {
        monitor->pointerUp();
        auto &cam = freeLooking ? freeLookModel : model;
        cam.reset();
        monitor->pointerDown(cam.anchor().x(), cam.anchor().y());
    }
}

void AimController::stop() {
    if (!active && !mouse) return;
    timer->stop();
    rawMouse.stop();
    if (active && !suspended) {
        if (touchMode) {
            monitor->touchUp(0);
            if (firing) monitor->touchUp(1);
            if (adsHeld) monitor->touchUp(2);
        } else {
            monitor->pointerUp();
        }
    }
    active = false;
    suspended = false;
    freeLooking = false;
    firing = false;
    adsHeld = false;
    suspendHeld = false;
    pendingX = pendingY = 0;
    if (mouse) { UnhookWindowsHookEx(mouse); mouse = nullptr; }
    ClipCursor(nullptr);
    notifyState(keyboard != nullptr, false, false,
                QStringLiteral("Aim released. Focus Android and press %1 to resume.").arg(keyName(cfg.toggleKey)));
}

void AimController::flushMove() {
    if (!pendingX && !pendingY) return;
    RECT r{};
    if (!GetClientRect(guest, &r) || r.right <= 0 || r.bottom <= 0) { stop(); return; }
    if (pendingAge.isValid()) maximumBatchWait = qMax(maximumBatchWait, pendingAge.elapsed());
    pendingAge.invalidate(); ++sentBatches;
    const int dx = pendingX, dy = pendingY;
    pendingX = pendingY = 0;
    auto &cam = freeLooking ? freeLookModel : model;
    if (cam.move(dx, dy, r.right, r.bottom)) {
        if (touchMode) {
            monitor->touchUp(0);
            monitor->touchDown(0, cam.anchor().x(), cam.anchor().y());
        } else {
            monitor->pointerUp();
            monitor->pointerDown(cam.anchor().x(), cam.anchor().y());
        }
    } else if (touchMode) {
        monitor->touchMove(0, cam.position().x(), cam.position().y());
    } else {
        monitor->pointerMove(cam.position().x(), cam.position().y());
    }
}

void AimController::queueMove(int dx, int dy) {
    if (!dx && !dy) return;
    if (!pendingX && !pendingY) pendingAge.start();
    const auto delta = nexus_input_accumulate(pendingX, pendingY, dx, dy);
    pendingX = delta.x; pendingY = delta.y; ++receivedMoves;
}
QString AimController::inputDiagnostics() const {
    return QString("Aim input: %1; capture: %2; target batch interval: %3 ms\nMovement packets: %4; sent movement batches: %5; largest observed host batch wait: %6 ms\nHost-only counters, not game FPS or end-to-end input latency. Absolute raw-mouse devices are unsupported.")
        .arg(useRawMouse ? "experimental relative raw mouse" : "compatibility cursor deltas")
        .arg(isCapturingMouse() ? "active" : "inactive").arg(timer->interval()).arg(receivedMoves).arg(sentBatches).arg(maximumBatchWait);
}

LRESULT CALLBACK AimController::keyboardProc(int code, WPARAM msg, LPARAM data) {
    auto *self = instance;
    if (code == HC_ACTION && self) {
        const auto *key = reinterpret_cast<const KBDLLHOOKSTRUCT *>(data);
        if (!(key->flags & LLKHF_INJECTED)) {
            const int vk = static_cast<int>(key->vkCode);
            if (vk == VK_F11) return CallNextHookEx(nullptr, code, msg, data);
            const bool down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            const bool up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

            if (keysMatch(self->cfg.toggleKey, vk) && (self->toggleHeld || self->active || self->focused())) {
                if (up) { self->toggleHeld = false; return 1; }
                if (down) {
                    if (!self->toggleHeld) {
                        self->toggleHeld = true;
                        if (self->active) self->stop();
                        else if (!self->start())
                            self->notifyState(true, false, false,
                                QStringLiteral("Could not activate aim. Focus the Android display and try %1 again.")
                                    .arg(keyName(self->cfg.toggleKey)));
                    }
                    return 1;
                }
            }

            if (self->active && self->cfg.suspendKey != 0 && keysMatch(self->cfg.suspendKey, vk)) {
                const bool altTab = (vk == VK_TAB) && ((GetAsyncKeyState(VK_MENU) & 0x8000) != 0);
                if (!altTab) {
                    if (down && !self->suspendHeld) {
                        self->suspendHeld = true;
                        self->enterSuspend();
                    } else if (up && self->suspendHeld) {
                        self->suspendHeld = false;
                        self->exitSuspend();
                    }
                    return 1;
                }
            }

            if (self->active && !self->suspended && self->cfg.freeLookEnabled &&
                self->cfg.freeLookKey != 0 && keysMatch(self->cfg.freeLookKey, vk)) {
                if (down && !self->freeLooking) self->setFreeLookActive(true);
                else if (up && self->freeLooking) self->setFreeLookActive(false);
                return 1;
            }
        }
    }
    return CallNextHookEx(nullptr, code, msg, data);
}

LRESULT CALLBACK AimController::mouseProc(int code, WPARAM msg, LPARAM data) {
    auto *self = instance;
    if (code != HC_ACTION || !self || !self->active || self->suspended)
        return CallNextHookEx(nullptr, code, msg, data);
    if (!self->focused()) { self->stop(); return CallNextHookEx(nullptr, code, msg, data); }
    const auto *event = reinterpret_cast<const MSLLHOOKSTRUCT *>(data);
    if (msg == WM_MOUSEMOVE) {
        int dx = event->pt.x - self->cursorCenter.x, dy = event->pt.y - self->cursorCenter.y;
        if (!self->useRawMouse && !(event->flags & LLMHF_INJECTED) && (dx || dy) && (!self->firing || self->touchMode)) {
            self->queueMove(dx, dy);
        }
        if ((dx || dy) && !SetCursorPos(self->cursorCenter.x, self->cursorCenter.y)) self->stop();
        return 1;
    }
    if (event->flags & LLMHF_INJECTED) return 1;
    if (msg == WM_LBUTTONDOWN && self->cfg.fireEnabled && !self->firing) {
        if (self->touchMode) {
            self->monitor->touchDown(1, self->cfg.fireX, self->cfg.fireY);
        } else {
            self->pendingX = self->pendingY = 0;
            self->monitor->pointerUp();
            self->monitor->pointerDown(self->cfg.fireX, self->cfg.fireY);
        }
        self->firing = true;
        return 1;
    }
    if (msg == WM_LBUTTONUP && self->firing) {
        if (self->touchMode) {
            self->monitor->touchUp(1);
        } else {
            self->monitor->pointerUp();
            auto &cam = self->freeLooking ? self->freeLookModel : self->model;
            cam.reset();
            self->monitor->pointerDown(cam.anchor().x(), cam.anchor().y());
        }
        self->firing = false;
        return 1;
    }
    if (msg == WM_RBUTTONDOWN && self->cfg.adsEnabled) {
        self->triggerAdsDown();
        return 1;
    }
    if (msg == WM_RBUTTONUP && self->cfg.adsEnabled) {
        if (self->cfg.adsHoldMode) self->triggerAdsUp();
        return 1;
    }
    // Pass wheel/side mouse buttons to InputMapper when multi-touch allows concurrent binds.
    if (self->touchMode && (msg == WM_MOUSEWHEEL || msg == WM_MBUTTONDOWN || msg == WM_MBUTTONUP ||
                            msg == WM_XBUTTONDOWN || msg == WM_XBUTTONUP)) {
        return CallNextHookEx(nullptr, code, msg, data);
    }
    return 1;
}
