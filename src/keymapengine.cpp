#include "keymapengine.h"
#include "clicksequence.h"
#include "rustcore.h"
#include <algorithm>

namespace {
int canonicalMod(int key) {
    if (key == 0x10 || key == 0xA0 || key == 0xA1) return 0x10;
    if (key == 0x11 || key == 0xA2 || key == 0xA3) return 0x11;
    if (key == 0x12 || key == 0xA4 || key == 0xA5) return 0x12;
    return 0;
}
}

KeymapEngine::KeymapEngine(QObject *parent) : QObject(parent) {
    frameTimer.setInterval(16);
    connect(&frameTimer, &QTimer::timeout, this, &KeymapEngine::tickAnimations);

    for (int i = 0; i < static_cast<int>(tapRunners.size()); ++i) {
        auto *seq = new ClickSequence(this);
        tapRunners[i].sequence = seq;
        connect(seq, &ClickSequence::pointerDown, this, [this, i](double x, double y) {
            auto &r = tapRunners[i];
            if (r.contact < 0) return;
            r.contactPressed = true;
            if (r.contact == SinglePointerContact) emit pointerDown(x, y);
            else emit touchDown(r.contact, x, y);
        });
        connect(seq, &ClickSequence::pointerUp, this, [this, i] {
            auto &r = tapRunners[i];
            if (r.contact < 0 || !r.contactPressed) return;
            r.contactPressed = false;
            if (r.contact == SinglePointerContact) emit pointerUp();
            else emit touchUp(r.contact);
        });
        connect(seq, &ClickSequence::finished, this, [this, i] {
            auto &r = tapRunners[i];
            if (r.contact >= 0) {
                if (r.contactPressed) {
                    r.contactPressed = false;
                    if (r.contact == SinglePointerContact) emit pointerUp();
                    else emit touchUp(r.contact);
                }
                if (r.contact == SinglePointerContact) singlePointerInUse = false;
                else if (r.contact >= 0 && r.contact < 10) slotInUse[r.contact] = false;
                r.contact = -1;
            }
            r.key = 0;
        });
    }
}

void KeymapEngine::configure(const KeymapProfile &profile, bool multitouch) {
    cancelAll();
    currentProfile = profile;
    touchMode = multitouch;
    dpadStates.clear();
    dpadStates.resize(currentProfile.dpads.size());
}

void KeymapEngine::setReservedTouchSlots(const QSet<int> &slotsReserved) {
    reservedSlots = slotsReserved;
}

int KeymapEngine::acquireContact() {
    if (touchMode) {
        for (int s = 0; s < 10; ++s) {
            if (!slotInUse[s] && !reservedSlots.contains(s)) {
                slotInUse[s] = true;
                return s;
            }
        }
        return -1;
    }
    if (singlePointerInUse) return -1;
    singlePointerInUse = true;
    return SinglePointerContact;
}

void KeymapEngine::contactDown(int contact, double x, double y) {
    if (contact == SinglePointerContact) emit pointerDown(x, y);
    else if (contact >= 0 && contact < 10) emit touchDown(contact, x, y);
}

void KeymapEngine::contactMove(int contact, double x, double y) {
    if (contact == SinglePointerContact) emit pointerMove(x, y);
    else if (contact >= 0 && contact < 10) emit touchMove(contact, x, y);
}

void KeymapEngine::contactUp(int contact) {
    if (contact == SinglePointerContact) {
        if (singlePointerInUse) {
            singlePointerInUse = false;
            emit pointerUp();
        }
    } else if (contact >= 0 && contact < 10) {
        if (slotInUse[contact]) {
            slotInUse[contact] = false;
            emit touchUp(contact);
        }
    }
}

void KeymapEngine::ActiveScript::releaseContacts(KeymapEngine *engine) {
    if (primaryContact >= 0) {
        if (primaryDown) engine->contactUp(primaryContact);
        else if (primaryContact == SinglePointerContact) engine->singlePointerInUse = false;
        else if (primaryContact < 10) engine->slotInUse[primaryContact] = false;
        primaryContact = -1;
        primaryDown = false;
    }
    if (secondaryContact >= 0) {
        if (secondaryDown) engine->contactUp(secondaryContact);
        else if (secondaryContact == SinglePointerContact) engine->singlePointerInUse = false;
        else if (secondaryContact < 10) engine->slotInUse[secondaryContact] = false;
        secondaryContact = -1;
        secondaryDown = false;
    }
}

void KeymapEngine::updateDpad(int index) {
    if (index < 0 || index >= currentProfile.dpads.size() || index >= dpadStates.size()) return;
    const auto &cfg = currentProfile.dpads[index];
    auto &st = dpadStates[index];
    const bool anyDir = st.up || st.down || st.left || st.right;
    if (!anyDir) {
        if (st.contact >= 0) {
            const int c = st.contact;
            st.contact = -1;
            contactUp(c);
        }
        return;
    }
    double scale = 1.0;
    if (cfg.sprintKey != 0 && st.sprint) scale = cfg.sprintMultiplier;
    else if (cfg.walkKey != 0 && st.walk) scale = cfg.walkMultiplier;

    const auto target = nexus_dpad_target(
        cfg.x, cfg.y, cfg.radius,
        static_cast<std::uint8_t>(st.up),
        static_cast<std::uint8_t>(st.down),
        static_cast<std::uint8_t>(st.left),
        static_cast<std::uint8_t>(st.right),
        scale);

    if (st.contact < 0) {
        const int c = acquireContact();
        if (c < 0) return;
        st.contact = c;
        contactDown(c, cfg.x, cfg.y);
        contactMove(c, target.x, target.y);
    } else {
        contactMove(st.contact, target.x, target.y);
    }
}

bool KeymapEngine::handleKeyPress(int key, QPointF cursorNorm) {
    lastCursor = QPointF(std::clamp(cursorNorm.x(), 0.0, 1.0), std::clamp(cursorNorm.y(), 0.0, 1.0));
    if (const int mod = canonicalMod(key)) heldModifiers.insert(mod);

    bool consumed = false;

    // 1. Active MOBA skill cancel takes highest priority while a manual skill is aiming.
    for (auto &moba : activeMobas) {
        if (!moba.cancelling && !moba.autoReleasePending && moba.cfg.cancelEnabled &&
            keysMatch(moba.cfg.cancelKey, key)) {
            moba.cancelling = true;
            moba.autoReleasePending = true;
            moba.releaseAfterMs = 35;
            moba.timer.start();
            contactMove(moba.contact, moba.cfg.cancelX, moba.cfg.cancelY);
            if (!frameTimer.isActive()) frameTimer.start();
            return true;
        }
    }

    // 2. D-pad directions and sprint/walk modifiers.
    for (int i = 0; i < currentProfile.dpads.size(); ++i) {
        const auto &d = currentProfile.dpads[i];
        auto &st = dpadStates[i];
        bool dirMatched = false;
        bool modMatched = false;
        if (keysMatch(d.keyUp, key)) { st.up = true; dirMatched = true; }
        if (keysMatch(d.keyDown, key)) { st.down = true; dirMatched = true; }
        if (keysMatch(d.keyLeft, key)) { st.left = true; dirMatched = true; }
        if (keysMatch(d.keyRight, key)) { st.right = true; dirMatched = true; }
        if (d.sprintKey != 0 && keysMatch(d.sprintKey, key)) { st.sprint = true; modMatched = true; }
        if (d.walkKey != 0 && keysMatch(d.walkKey, key)) { st.walk = true; modMatched = true; }
        if (dirMatched || (modMatched && st.contact >= 0)) {
            updateDpad(i);
            if (dirMatched) consumed = true;
        }
    }

    // 3. Pinch-zoom (requires 2 simultaneous contacts -> multi-touch mode).
    for (const auto &z : currentProfile.zooms) {
        const bool modOk = z.modifierKey == 0 ||
            heldModifiers.contains(canonicalMod(z.modifierKey)) ||
            keysMatch(z.modifierKey, key);
        const bool inMatch = modOk && keysMatch(z.keyIn, key);
        const bool outMatch = modOk && keysMatch(z.keyOut, key);
        if (inMatch || outMatch) {
            if (touchMode) {
                const int cA = acquireContact();
                const int cB = cA >= 0 ? acquireContact() : -1;
                if (cA >= 0 && cB >= 0) {
                    ActiveZoom az;
                    az.contactA = cA;
                    az.contactB = cB;
                    az.x = z.x; az.y = z.y; az.radius = z.radius;
                    az.durationMs = z.durationMs;
                    az.zoomIn = inMatch;
                    const auto p0 = nexus_pinch_points(z.x, z.y, z.radius, 0.0, static_cast<std::uint8_t>(az.zoomIn));
                    contactDown(cA, p0.x1, p0.y1);
                    contactDown(cB, p0.x2, p0.y2);
                    az.elapsed.start();
                    activeZooms.append(az);
                    if (!frameTimer.isActive()) frameTimer.start();
                    consumed = true;
                } else if (cA >= 0) {
                    slotInUse[cA] = false;
                }
            }
        }
    }

    // 4. Swipes.
    for (int i = 0; i < currentProfile.swipes.size(); ++i) {
        const auto &sw = currentProfile.swipes[i];
        if (!keysMatch(sw.key, key)) continue;
        bool already = false;
        for (const auto &a : activeSwipes) if (a.controlIndex == i) { already = true; break; }
        if (already) { consumed = true; continue; }
        const int c = acquireContact();
        if (c >= 0) {
            ActiveSwipe as;
            as.controlIndex = i;
            as.key = key;
            as.contact = c;
            as.durationMs = sw.durationMs;
            as.holdAtEnd = sw.holdAtEnd;
            as.keyHeld = true;
            as.x1 = sw.x1; as.y1 = sw.y1; as.x2 = sw.x2; as.y2 = sw.y2;
            contactDown(c, sw.x1, sw.y1);
            as.elapsed.start();
            activeSwipes.append(as);
            if (!frameTimer.isActive()) frameTimer.start();
            consumed = true;
        }
    }

    // 5. MOBA Skill Pads.
    for (int i = 0; i < currentProfile.mobaSkills.size(); ++i) {
        const auto &m = currentProfile.mobaSkills[i];
        if (!keysMatch(m.key, key)) continue;
        bool already = false;
        for (const auto &am : activeMobas) if (am.controlIndex == i) { already = true; break; }
        if (already) { consumed = true; continue; }
        const int c = acquireContact();
        if (c >= 0) {
            ActiveMoba am;
            am.controlIndex = i;
            am.key = key;
            am.contact = c;
            am.cfg = m;
            contactDown(c, m.x, m.y);
            if (m.mode == MobaCastMode::AutoCastTap) {
                am.autoReleasePending = true;
                am.releaseAfterMs = 50;
                am.timer.start();
            } else {
                const auto target = nexus_moba_target(m.x, m.y, m.radius, m.originX, m.originY,
                                                      lastCursor.x(), lastCursor.y());
                contactMove(c, target.x, target.y);
                if (m.mode == MobaCastMode::QuickCast) {
                    am.autoReleasePending = true;
                    am.releaseAfterMs = std::clamp(m.quickCastHoldMs, 20, 1000);
                    am.timer.start();
                }
            }
            activeMobas.append(am);
            if (am.autoReleasePending && !frameTimer.isActive()) frameTimer.start();
            consumed = true;
        }
    }

    // 6. Macro Scripts.
    for (int i = 0; i < currentProfile.scripts.size(); ++i) {
        const auto &sc = currentProfile.scripts[i];
        if (!keysMatch(sc.key, key)) continue;
        bool already = false;
        for (const auto &as : activeScripts) if (as.controlIndex == i) { already = true; break; }
        if (already) { consumed = true; continue; }
        QList<ScriptStep> compiled;
        if (parseScriptText(sc.scriptText, &compiled, nullptr) && !compiled.isEmpty()) {
            ActiveScript run;
            run.controlIndex = i;
            run.key = key;
            run.repeatWhileHeld = sc.repeatWhileHeld;
            run.keyHeld = true;
            run.steps = compiled;
            activeScripts.append(run);
            advanceScripts();
            if (!activeScripts.isEmpty() && !frameTimer.isActive()) frameTimer.start();
            consumed = true;
        }
    }

    // 7. Tap / Sequential / Burst / Repeat bindings.
    bool hasTapForKey = false;
    for (const auto &b : currentProfile.taps) {
        if (keysMatch(b.key, key)) { hasTapForKey = true; break; }
    }
    if (hasTapForKey) {
        bool alreadyRunning = false;
        for (const auto &r : tapRunners) {
            if (r.sequence->active() && keysMatch(r.key, key)) { alreadyRunning = true; break; }
        }
        if (alreadyRunning) {
            consumed = true;
        } else {
            const int limit = touchMode ? static_cast<int>(tapRunners.size()) : 1;
            for (int i = 0; i < limit; ++i) {
                auto &r = tapRunners[i];
                if (!r.sequence->active()) {
                    const int c = acquireContact();
                    if (c < 0) break;
                    r.contact = c;
                    r.contactPressed = false;
                    r.key = key;
                    if (r.sequence->press(key, currentProfile.taps)) {
                        consumed = true;
                    } else {
                        if (c == SinglePointerContact) singlePointerInUse = false;
                        else if (c >= 0 && c < 10) slotInUse[c] = false;
                        r.contact = -1;
                        r.key = 0;
                    }
                    break;
                }
            }
        }
    }

    return consumed;
}

bool KeymapEngine::handleKeyRelease(int key) {
    if (const int mod = canonicalMod(key)) heldModifiers.remove(mod);
    bool consumed = false;

    for (int i = 0; i < currentProfile.dpads.size(); ++i) {
        const auto &d = currentProfile.dpads[i];
        auto &st = dpadStates[i];
        bool changed = false;
        if (keysMatch(d.keyUp, key) && st.up) { st.up = false; changed = true; consumed = true; }
        if (keysMatch(d.keyDown, key) && st.down) { st.down = false; changed = true; consumed = true; }
        if (keysMatch(d.keyLeft, key) && st.left) { st.left = false; changed = true; consumed = true; }
        if (keysMatch(d.keyRight, key) && st.right) { st.right = false; changed = true; consumed = true; }
        if (d.sprintKey != 0 && keysMatch(d.sprintKey, key) && st.sprint) { st.sprint = false; changed = true; }
        if (d.walkKey != 0 && keysMatch(d.walkKey, key) && st.walk) { st.walk = false; changed = true; }
        if (changed) updateDpad(i);
    }

    for (int i = activeSwipes.size() - 1; i >= 0; --i) {
        auto &sw = activeSwipes[i];
        if (keysMatch(sw.key, key)) {
            sw.keyHeld = false;
            consumed = true;
            if (sw.holdAtEnd && sw.reachedEnd) {
                contactUp(sw.contact);
                activeSwipes.removeAt(i);
            }
        }
    }

    for (int i = activeMobas.size() - 1; i >= 0; --i) {
        auto &am = activeMobas[i];
        if (keysMatch(am.key, key)) {
            consumed = true;
            if (!am.autoReleasePending && !am.cancelling) {
                contactUp(am.contact);
                activeMobas.removeAt(i);
            }
        }
    }

    for (auto &sc : activeScripts) {
        if (keysMatch(sc.key, key)) {
            sc.keyHeld = false;
            consumed = true;
        }
    }

    for (auto &r : tapRunners) {
        if (r.sequence->active() && keysMatch(r.key, key)) {
            r.sequence->release(key);
            consumed = true;
        }
    }

    return consumed;
}

bool KeymapEngine::handleWheel(int wheelKey, QPointF cursorNorm) {
    // For simple hold taps bound to wheel, ensure at least a 2-pulse burst or short tap;
    // for Zoom/Swipe/Burst/QuickCast/Script, press+release runs the full timed gesture.
    const bool ok = handleKeyPress(wheelKey, cursorNorm);
    if (ok) {
        QTimer::singleShot(45, this, [this, wheelKey] { handleKeyRelease(wheelKey); });
    }
    return ok;
}

void KeymapEngine::handleCursorMove(QPointF cursorNorm) {
    lastCursor = QPointF(std::clamp(cursorNorm.x(), 0.0, 1.0), std::clamp(cursorNorm.y(), 0.0, 1.0));
    for (auto &am : activeMobas) {
        if (am.cfg.mode == MobaCastMode::ManualAimWhileHeld && !am.cancelling && !am.autoReleasePending && am.contact >= 0) {
            const auto target = nexus_moba_target(am.cfg.x, am.cfg.y, am.cfg.radius,
                                                  am.cfg.originX, am.cfg.originY,
                                                  lastCursor.x(), lastCursor.y());
            contactMove(am.contact, target.x, target.y);
        }
    }
}

void KeymapEngine::advanceScripts() {
    for (int i = activeScripts.size() - 1; i >= 0; --i) {
        auto &sc = activeScripts[i];
        bool keepStepping = true;
        while (keepStepping && sc.stepIndex < sc.steps.size()) {
            keepStepping = false;
            const auto &step = sc.steps[sc.stepIndex];
            switch (step.op) {
            case ScriptOp::Tap:
                if (!sc.stepStarted) {
                    if (sc.primaryContact < 0) sc.primaryContact = acquireContact();
                    if (sc.primaryContact < 0) { sc.releaseContacts(this); activeScripts.removeAt(i); goto next_script; }
                    contactDown(sc.primaryContact, step.x1, step.y1);
                    sc.primaryDown = true;
                    sc.stepStarted = true;
                    sc.stepTimer.start();
                } else if (sc.stepTimer.elapsed() >= step.durationMs) {
                    contactUp(sc.primaryContact);
                    sc.primaryContact = -1;
                    sc.primaryDown = false;
                    sc.stepStarted = false;
                    ++sc.stepIndex;
                    keepStepping = true;
                }
                break;

            case ScriptOp::Wait:
                if (!sc.stepStarted) {
                    sc.stepStarted = true;
                    sc.stepTimer.start();
                } else if (sc.stepTimer.elapsed() >= step.durationMs) {
                    sc.stepStarted = false;
                    ++sc.stepIndex;
                    keepStepping = true;
                }
                break;

            case ScriptOp::Swipe:
                if (!sc.stepStarted) {
                    if (sc.primaryContact < 0) sc.primaryContact = acquireContact();
                    if (sc.primaryContact < 0) { sc.releaseContacts(this); activeScripts.removeAt(i); goto next_script; }
                    contactDown(sc.primaryContact, step.x1, step.y1);
                    sc.primaryDown = true;
                    sc.stepStarted = true;
                    sc.stepTimer.start();
                } else {
                    const double t = std::clamp(static_cast<double>(sc.stepTimer.elapsed()) / step.durationMs, 0.0, 1.0);
                    const auto pt = nexus_swipe_point(step.x1, step.y1, step.x2, step.y2, t);
                    contactMove(sc.primaryContact, pt.x, pt.y);
                    if (t >= 1.0) {
                        contactUp(sc.primaryContact);
                        sc.primaryContact = -1;
                        sc.primaryDown = false;
                        sc.stepStarted = false;
                        ++sc.stepIndex;
                        keepStepping = true;
                    }
                }
                break;

            case ScriptOp::Down:
                if (!sc.primaryDown) {
                    if (sc.primaryContact < 0) sc.primaryContact = acquireContact();
                    if (sc.primaryContact < 0) { sc.releaseContacts(this); activeScripts.removeAt(i); goto next_script; }
                    contactDown(sc.primaryContact, step.x1, step.y1);
                    sc.primaryDown = true;
                } else {
                    contactMove(sc.primaryContact, step.x1, step.y1);
                }
                ++sc.stepIndex;
                keepStepping = true;
                break;

            case ScriptOp::Move:
                if (sc.primaryDown && sc.primaryContact >= 0) {
                    contactMove(sc.primaryContact, step.x1, step.y1);
                }
                ++sc.stepIndex;
                keepStepping = true;
                break;

            case ScriptOp::Up:
                if (sc.primaryContact >= 0) {
                    if (sc.primaryDown) contactUp(sc.primaryContact);
                    else if (sc.primaryContact == SinglePointerContact) singlePointerInUse = false;
                    else if (sc.primaryContact < 10) slotInUse[sc.primaryContact] = false;
                    sc.primaryContact = -1;
                    sc.primaryDown = false;
                }
                ++sc.stepIndex;
                keepStepping = true;
                break;

            case ScriptOp::MultiPress:
                if (!sc.stepStarted) {
                    if (!touchMode) {
                        // In single-pointer mode degrade gracefully to tapping the first point.
                        if (sc.primaryContact < 0) sc.primaryContact = acquireContact();
                        if (sc.primaryContact < 0) { sc.releaseContacts(this); activeScripts.removeAt(i); goto next_script; }
                        contactDown(sc.primaryContact, step.x1, step.y1);
                        sc.primaryDown = true;
                    } else {
                        if (sc.primaryContact < 0) sc.primaryContact = acquireContact();
                        if (sc.secondaryContact < 0) sc.secondaryContact = acquireContact();
                        if (sc.primaryContact < 0 || sc.secondaryContact < 0) {
                            sc.releaseContacts(this); activeScripts.removeAt(i); goto next_script;
                        }
                        contactDown(sc.primaryContact, step.x1, step.y1);
                        contactDown(sc.secondaryContact, step.x2, step.y2);
                        sc.primaryDown = true;
                        sc.secondaryDown = true;
                    }
                    sc.stepStarted = true;
                    sc.stepTimer.start();
                } else if (sc.stepTimer.elapsed() >= step.durationMs) {
                    sc.releaseContacts(this);
                    sc.stepStarted = false;
                    ++sc.stepIndex;
                    keepStepping = true;
                }
                break;
            }
        }
        if (sc.stepIndex >= sc.steps.size()) {
            sc.releaseContacts(this);
            if (sc.repeatWhileHeld && sc.keyHeld) {
                sc.stepIndex = 0;
                sc.stepStarted = false;
            } else {
                activeScripts.removeAt(i);
            }
        }
    next_script:;
    }
}

void KeymapEngine::tickAnimations() {
    for (int i = activeSwipes.size() - 1; i >= 0; --i) {
        auto &sw = activeSwipes[i];
        if (!sw.reachedEnd) {
            const double t = std::clamp(static_cast<double>(sw.elapsed.elapsed()) / sw.durationMs, 0.0, 1.0);
            const auto pt = nexus_swipe_point(sw.x1, sw.y1, sw.x2, sw.y2, t);
            contactMove(sw.contact, pt.x, pt.y);
            if (t >= 1.0) {
                sw.reachedEnd = true;
                if (!sw.holdAtEnd || !sw.keyHeld) {
                    contactUp(sw.contact);
                    activeSwipes.removeAt(i);
                }
            }
        } else if (!sw.holdAtEnd || !sw.keyHeld) {
            contactUp(sw.contact);
            activeSwipes.removeAt(i);
        }
    }

    for (int i = activeZooms.size() - 1; i >= 0; --i) {
        auto &az = activeZooms[i];
        const double t = std::clamp(static_cast<double>(az.elapsed.elapsed()) / az.durationMs, 0.0, 1.0);
        const auto pts = nexus_pinch_points(az.x, az.y, az.radius, t, static_cast<std::uint8_t>(az.zoomIn));
        contactMove(az.contactA, pts.x1, pts.y1);
        contactMove(az.contactB, pts.x2, pts.y2);
        if (t >= 1.0) {
            contactUp(az.contactA);
            contactUp(az.contactB);
            activeZooms.removeAt(i);
        }
    }

    for (int i = activeMobas.size() - 1; i >= 0; --i) {
        auto &am = activeMobas[i];
        if (am.autoReleasePending && am.timer.elapsed() >= am.releaseAfterMs) {
            contactUp(am.contact);
            activeMobas.removeAt(i);
        }
    }

    if (!activeScripts.isEmpty()) advanceScripts();

    bool needsTimer = !activeZooms.isEmpty() || !activeScripts.isEmpty();
    if (!needsTimer) {
        for (const auto &sw : activeSwipes) if (!sw.reachedEnd) { needsTimer = true; break; }
    }
    if (!needsTimer) {
        for (const auto &am : activeMobas) if (am.autoReleasePending) { needsTimer = true; break; }
    }
    if (!needsTimer) frameTimer.stop();
}

void KeymapEngine::cancelAll() {
    frameTimer.stop();
    heldModifiers.clear();
    for (auto &r : tapRunners) r.sequence->cancel();
    for (auto &st : dpadStates) {
        st.up = st.down = st.left = st.right = st.sprint = st.walk = false;
        if (st.contact >= 0) { contactUp(st.contact); st.contact = -1; }
    }
    for (auto &sw : activeSwipes) if (sw.contact >= 0) contactUp(sw.contact);
    activeSwipes.clear();
    for (auto &az : activeZooms) {
        if (az.contactA >= 0) contactUp(az.contactA);
        if (az.contactB >= 0) contactUp(az.contactB);
    }
    activeZooms.clear();
    for (auto &am : activeMobas) if (am.contact >= 0) contactUp(am.contact);
    activeMobas.clear();
    for (auto &sc : activeScripts) sc.releaseContacts(this);
    activeScripts.clear();
    slotInUse.fill(false);
    singlePointerInUse = false;
}

bool KeymapEngine::active() const {
    for (const auto &r : tapRunners) if (r.sequence->active()) return true;
    for (const auto &st : dpadStates) if (st.contact >= 0) return true;
    return !activeSwipes.isEmpty() || !activeZooms.isEmpty() ||
           !activeMobas.isEmpty() || !activeScripts.isEmpty();
}
