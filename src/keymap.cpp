#include "keymap.h"
#include "rustcore.h"
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QRegularExpression>
#include <QSaveFile>
#include <QVector>
#include <algorithm>
#include <cmath>
#include <cstddef>

static_assert(sizeof(NexusBinding) == 32 && offsetof(NexusBinding, x) == 8 &&
              offsetof(NexusBinding, repeat) == 24 && offsetof(NexusBinding, burst_count) == 25 &&
              offsetof(NexusBinding, interval_ms) == 28,
              "Rust/C++ key binding ABI mismatch");
static_assert(sizeof(NexusPoint) == 16 && sizeof(NexusPinchPair) == 32 &&
              sizeof(NexusScriptStep) == 40 && offsetof(NexusScriptStep, x1) == 8,
              "Rust/C++ extended keymap ABI mismatch");

bool validKey(int key) { return nexus_valid_key(key) != 0; }
bool isMouseBindingKey(int key) { return key >= MouseRightButton && key <= MouseWheelDown; }

static int canonicalModifierGroup(int key) {
    if (key == 0x10 || key == 0xA0 || key == 0xA1) return 0x10;
    if (key == 0x11 || key == 0xA2 || key == 0xA3) return 0x11;
    if (key == 0x12 || key == 0xA4 || key == 0xA5) return 0x12;
    return key;
}

bool keysMatch(int boundKey, int eventKey) {
    if (boundKey <= 0 || eventKey <= 0) return false;
    if (boundKey == eventKey) return true;
    // Generic VK_SHIFT/CONTROL/MENU matches left/right variants; specific L/R only matches exact or generic.
    if (boundKey == 0x10 && (eventKey == 0xA0 || eventKey == 0xA1)) return true;
    if (boundKey == 0x11 && (eventKey == 0xA2 || eventKey == 0xA3)) return true;
    if (boundKey == 0x12 && (eventKey == 0xA4 || eventKey == 0xA5)) return true;
    if (eventKey == 0x10 && (boundKey == 0xA0 || boundKey == 0xA1)) return true;
    if (eventKey == 0x11 && (boundKey == 0xA2 || boundKey == 0xA3)) return true;
    if (eventKey == 0x12 && (boundKey == 0xA4 || boundKey == 0xA5)) return true;
    return false;
}

QString keyName(int key) {
    if (key >= 'A' && key <= 'Z') return QString(QChar(key));
    if (key >= '0' && key <= '9') return QString(QChar(key));
    if (key >= 0x70 && key <= 0x7B) return QStringLiteral("F%1").arg(key - 0x70 + 1);
    if (key >= 0x60 && key <= 0x69) return QStringLiteral("Num %1").arg(key - 0x60);
    switch (key) {
    case 0: return QStringLiteral("None");
    case 0x08: return QStringLiteral("Backspace");
    case 0x09: return QStringLiteral("Tab");
    case 0x0D: return QStringLiteral("Enter");
    case 0x10: return QStringLiteral("Shift");
    case 0x11: return QStringLiteral("Ctrl");
    case 0x12: return QStringLiteral("Alt");
    case 0x14: return QStringLiteral("CapsLock");
    case 0x20: return QStringLiteral("Space");
    case 0x21: return QStringLiteral("PageUp");
    case 0x22: return QStringLiteral("PageDown");
    case 0x23: return QStringLiteral("End");
    case 0x24: return QStringLiteral("Home");
    case 0x25: return QStringLiteral("Left");
    case 0x26: return QStringLiteral("Up");
    case 0x27: return QStringLiteral("Right");
    case 0x28: return QStringLiteral("Down");
    case 0x2D: return QStringLiteral("Insert");
    case 0x2E: return QStringLiteral("Delete");
    case 0x6A: return QStringLiteral("Num *");
    case 0x6B: return QStringLiteral("Num +");
    case 0x6D: return QStringLiteral("Num -");
    case 0x6E: return QStringLiteral("Num .");
    case 0x6F: return QStringLiteral("Num /");
    case 0xA0: return QStringLiteral("LShift");
    case 0xA1: return QStringLiteral("RShift");
    case 0xA2: return QStringLiteral("LCtrl");
    case 0xA3: return QStringLiteral("RCtrl");
    case 0xA4: return QStringLiteral("LAlt");
    case 0xA5: return QStringLiteral("RAlt");
    case 0xBA: return QStringLiteral(";");
    case 0xBB: return QStringLiteral("=");
    case 0xBC: return QStringLiteral(",");
    case 0xBD: return QStringLiteral("-");
    case 0xBE: return QStringLiteral(".");
    case 0xBF: return QStringLiteral("/");
    case 0xC0: return QStringLiteral("`");
    case 0xDB: return QStringLiteral("[");
    case 0xDC: return QStringLiteral("\\");
    case 0xDD: return QStringLiteral("]");
    case 0xDE: return QStringLiteral("'");
    case MouseRightButton: return QStringLiteral("Right Click");
    case MouseMiddleButton: return QStringLiteral("Middle Click");
    case MouseX1Button: return QStringLiteral("Mouse 4");
    case MouseX2Button: return QStringLiteral("Mouse 5");
    case MouseWheelUp: return QStringLiteral("Wheel Up");
    case MouseWheelDown: return QStringLiteral("Wheel Down");
    default: return QStringLiteral("Key %1").arg(key);
    }
}

QString keyShortBadge(int key) {
    if (key >= 'A' && key <= 'Z') return QString(QChar(key));
    if (key >= '0' && key <= '9') return QString(QChar(key));
    if (key >= 0x70 && key <= 0x7B) return QStringLiteral("F%1").arg(key - 0x70 + 1);
    if (key >= 0x60 && key <= 0x69) return QStringLiteral("N%1").arg(key - 0x60);
    switch (key) {
    case 0: return QStringLiteral("—");
    case 0x08: return QStringLiteral("BK");
    case 0x09: return QStringLiteral("TAB");
    case 0x0D: return QStringLiteral("ENT");
    case 0x10: case 0xA0: case 0xA1: return QStringLiteral("SHF");
    case 0x11: case 0xA2: case 0xA3: return QStringLiteral("CTL");
    case 0x12: case 0xA4: case 0xA5: return QStringLiteral("ALT");
    case 0x14: return QStringLiteral("CAP");
    case 0x20: return QStringLiteral("SP");
    case 0x21: return QStringLiteral("PGU");
    case 0x22: return QStringLiteral("PGD");
    case 0x23: return QStringLiteral("END");
    case 0x24: return QStringLiteral("HOM");
    case 0x25: return QStringLiteral("←");
    case 0x26: return QStringLiteral("↑");
    case 0x27: return QStringLiteral("→");
    case 0x28: return QStringLiteral("↓");
    case 0x2D: return QStringLiteral("INS");
    case 0x2E: return QStringLiteral("DEL");
    case MouseRightButton: return QStringLiteral("RMB");
    case MouseMiddleButton: return QStringLiteral("MMB");
    case MouseX1Button: return QStringLiteral("M4");
    case MouseX2Button: return QStringLiteral("M5");
    case MouseWheelUp: return QStringLiteral("MW↑");
    case MouseWheelDown: return QStringLiteral("MW↓");
    default: return keyName(key);
    }
}

int qtKeyToBindingKey(int qtKey, Qt::KeyboardModifiers modifiers) {
    if (qtKey >= Qt::Key_A && qtKey <= Qt::Key_Z) return qtKey;
    if (qtKey >= Qt::Key_0 && qtKey <= Qt::Key_9) {
        if (modifiers & Qt::KeypadModifier) return 0x60 + (qtKey - Qt::Key_0);
        return qtKey;
    }
    if (qtKey >= Qt::Key_F1 && qtKey <= Qt::Key_F12) return 0x70 + (qtKey - Qt::Key_F1);
    switch (qtKey) {
    case Qt::Key_Space: return 0x20;
    case Qt::Key_Tab: case Qt::Key_Backtab: return 0x09;
    case Qt::Key_Return: case Qt::Key_Enter: return 0x0D;
    case Qt::Key_Backspace: return 0x08;
    case Qt::Key_Shift: return 0x10;
    case Qt::Key_Control: return 0x11;
    case Qt::Key_Alt: return 0x12;
    case Qt::Key_CapsLock: return 0x14;
    case Qt::Key_PageUp: return 0x21;
    case Qt::Key_PageDown: return 0x22;
    case Qt::Key_End: return 0x23;
    case Qt::Key_Home: return 0x24;
    case Qt::Key_Left: return 0x25;
    case Qt::Key_Up: return 0x26;
    case Qt::Key_Right: return 0x27;
    case Qt::Key_Down: return 0x28;
    case Qt::Key_Insert: return 0x2D;
    case Qt::Key_Delete: return 0x2E;
    case Qt::Key_Semicolon: case Qt::Key_Colon: return 0xBA;
    case Qt::Key_Equal: case Qt::Key_Plus: return (modifiers & Qt::KeypadModifier) ? 0x6B : 0xBB;
    case Qt::Key_Comma: case Qt::Key_Less: return 0xBC;
    case Qt::Key_Minus: case Qt::Key_Underscore: return (modifiers & Qt::KeypadModifier) ? 0x6D : 0xBD;
    case Qt::Key_Period: case Qt::Key_Greater: return (modifiers & Qt::KeypadModifier) ? 0x6E : 0xBE;
    case Qt::Key_Slash: case Qt::Key_Question: return (modifiers & Qt::KeypadModifier) ? 0x6F : 0xBF;
    case Qt::Key_Asterisk: return 0x6A;
    case Qt::Key_QuoteLeft: case Qt::Key_AsciiTilde: return 0xC0;
    case Qt::Key_BracketLeft: case Qt::Key_BraceLeft: return 0xDB;
    case Qt::Key_Backslash: case Qt::Key_Bar: return 0xDC;
    case Qt::Key_BracketRight: case Qt::Key_BraceRight: return 0xDD;
    case Qt::Key_Apostrophe: case Qt::Key_QuoteDbl: return 0xDE;
    default: return 0;
    }
}

int mouseButtonToBindingKey(Qt::MouseButton button) {
    switch (button) {
    case Qt::RightButton: return MouseRightButton;
    case Qt::MiddleButton: return MouseMiddleButton;
    case Qt::BackButton: return MouseX1Button;
    case Qt::ForwardButton: return MouseX2Button;
    default: return 0;
    }
}

QList<QPair<int, QString>> allBindableKeys() {
    QList<QPair<int, QString>> items;
    for (int k = 'A'; k <= 'Z'; ++k) items.append({k, keyName(k)});
    for (int k = '0'; k <= '9'; ++k) items.append({k, keyName(k)});
    const int extras[] = {
        0x20, 0x09, 0x0D, 0x08, 0x10, 0x11, 0x12, 0x14,
        0x26, 0x28, 0x25, 0x27, 0x2D, 0x2E, 0x24, 0x23, 0x21, 0x22,
        0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x7B,
        0xC0, 0xBD, 0xBB, 0xDB, 0xDD, 0xDC, 0xBA, 0xDE, 0xBC, 0xBE, 0xBF,
        0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
        MouseRightButton, MouseMiddleButton, MouseX1Button, MouseX2Button, MouseWheelUp, MouseWheelDown
    };
    for (int k : extras) items.append({k, keyName(k)});
    return items;
}

static bool validUnit(double v) {
    return std::isfinite(v) && v >= 0.0 && v <= 1.0;
}

static bool validOptionalKey(int key) {
    return key == 0 || validKey(key);
}

static bool parsePercentCoord(const QString &token, double *outUnit) {
    bool ok = false;
    QString clean = token.trimmed();
    if (clean.endsWith('%')) clean.chop(1);
    const double pct = clean.toDouble(&ok);
    if (!ok || !std::isfinite(pct) || pct < 0.0 || pct > 100.0) return false;
    *outUnit = pct / 100.0;
    return true;
}

static bool parsePositiveInt(const QString &token, int minVal, int maxVal, int *outVal) {
    bool ok = false;
    const int val = token.trimmed().toInt(&ok);
    if (!ok || val < minVal || val > maxVal) return false;
    *outVal = val;
    return true;
}

bool parseScriptText(const QString &text, QList<ScriptStep> *steps, QString *error) {
    QList<ScriptStep> parsed;
    const QStringList lines = text.split('\n');
    static const QRegularExpression ws(QStringLiteral("\\s+"));
    for (int i = 0; i < lines.size(); ++i) {
        const QString line = lines[i].trimmed();
        if (line.isEmpty() || line.startsWith('#') || line.startsWith(QStringLiteral("//"))) continue;
        const QStringList parts = line.split(ws, Qt::SkipEmptyParts);
        const QString cmd = parts[0].toLower();
        ScriptStep step;
        if (cmd == QStringLiteral("tap")) {
            if (parts.size() < 3 || parts.size() > 4 ||
                !parsePercentCoord(parts[1], &step.x1) || !parsePercentCoord(parts[2], &step.y1)) {
                if (error) *error = QStringLiteral("Line %1: expected 'tap <x%> <y%> [hold_ms]'.").arg(i + 1);
                return false;
            }
            step.op = ScriptOp::Tap;
            step.durationMs = 50;
            if (parts.size() == 4 && !parsePositiveInt(parts[3], 10, 10000, &step.durationMs)) {
                if (error) *error = QStringLiteral("Line %1: tap hold_ms must be 10..10000.").arg(i + 1);
                return false;
            }
        } else if (cmd == QStringLiteral("wait") || cmd == QStringLiteral("sleep") || cmd == QStringLiteral("delay")) {
            if (parts.size() != 2 || !parsePositiveInt(parts[1], 10, 60000, &step.durationMs)) {
                if (error) *error = QStringLiteral("Line %1: expected 'wait <10..60000>'.").arg(i + 1);
                return false;
            }
            step.op = ScriptOp::Wait;
        } else if (cmd == QStringLiteral("swipe")) {
            if (parts.size() < 5 || parts.size() > 6 ||
                !parsePercentCoord(parts[1], &step.x1) || !parsePercentCoord(parts[2], &step.y1) ||
                !parsePercentCoord(parts[3], &step.x2) || !parsePercentCoord(parts[4], &step.y2)) {
                if (error) *error = QStringLiteral("Line %1: expected 'swipe <x1%> <y1%> <x2%> <y2%> [duration_ms]'.").arg(i + 1);
                return false;
            }
            step.op = ScriptOp::Swipe;
            step.durationMs = 160;
            if (parts.size() == 6 && !parsePositiveInt(parts[5], 20, 10000, &step.durationMs)) {
                if (error) *error = QStringLiteral("Line %1: swipe duration_ms must be 20..10000.").arg(i + 1);
                return false;
            }
        } else if (cmd == QStringLiteral("down") || cmd == QStringLiteral("press")) {
            if (parts.size() != 3 || !parsePercentCoord(parts[1], &step.x1) || !parsePercentCoord(parts[2], &step.y1)) {
                if (error) *error = QStringLiteral("Line %1: expected 'down <x%> <y%>'.").arg(i + 1);
                return false;
            }
            step.op = ScriptOp::Down;
        } else if (cmd == QStringLiteral("move")) {
            if (parts.size() != 3 || !parsePercentCoord(parts[1], &step.x1) || !parsePercentCoord(parts[2], &step.y1)) {
                if (error) *error = QStringLiteral("Line %1: expected 'move <x%> <y%>'.").arg(i + 1);
                return false;
            }
            step.op = ScriptOp::Move;
        } else if (cmd == QStringLiteral("up") || cmd == QStringLiteral("release")) {
            if (parts.size() != 1) {
                if (error) *error = QStringLiteral("Line %1: expected 'up' with no arguments.").arg(i + 1);
                return false;
            }
            step.op = ScriptOp::Up;
        } else if (cmd == QStringLiteral("multipress") || cmd == QStringLiteral("pinchtap")) {
            if (parts.size() < 5 || parts.size() > 6 ||
                !parsePercentCoord(parts[1], &step.x1) || !parsePercentCoord(parts[2], &step.y1) ||
                !parsePercentCoord(parts[3], &step.x2) || !parsePercentCoord(parts[4], &step.y2)) {
                if (error) *error = QStringLiteral("Line %1: expected 'multipress <x1%> <y1%> <x2%> <y2%> [hold_ms]'.").arg(i + 1);
                return false;
            }
            step.op = ScriptOp::MultiPress;
            step.durationMs = 70;
            if (parts.size() == 6 && !parsePositiveInt(parts[5], 20, 10000, &step.durationMs)) {
                if (error) *error = QStringLiteral("Line %1: multipress hold_ms must be 20..10000.").arg(i + 1);
                return false;
            }
        } else {
            if (error) *error = QStringLiteral("Line %1: unknown script command '%2'.").arg(i + 1).arg(parts[0]);
            return false;
        }
        parsed.append(step);
        if (parsed.size() > 256) {
            if (error) *error = QStringLiteral("Script exceeds maximum of 256 steps.");
            return false;
        }
    }
    if (parsed.isEmpty()) {
        if (error) *error = QStringLiteral("Script must contain at least one command.");
        return false;
    }
    QVector<NexusScriptStep> rustSteps;
    rustSteps.reserve(parsed.size());
    for (const auto &s : parsed) {
        rustSteps.append({static_cast<std::uint8_t>(s.op), {0, 0, 0}, s.durationMs, s.x1, s.y1, s.x2, s.y2});
    }
    if (nexus_validate_script_steps(rustSteps.constData(), static_cast<std::size_t>(rustSteps.size())) != 0) {
        if (error) *error = QStringLiteral("Script validation rejected out-of-range parameters.");
        return false;
    }
    if (steps) *steps = parsed;
    return true;
}

static bool validBindings(const QList<KeyBinding> &bindings, QString *error) {
    if (bindings.size() > 128) {
        if (error) *error = QStringLiteral("Too many keymap markers (maximum 128).");
        return false;
    }
    QVector<NexusBinding> rustBindings;
    rustBindings.reserve(bindings.size());
    for (const auto &b : bindings) {
        if (b.burstCount < 1 || b.burstCount > 50) {
            if (error) *error = QStringLiteral("Burst tap count must be between 1 and 50.");
            return false;
        }
        rustBindings.append({b.key, b.x, b.y, static_cast<std::uint8_t>(b.repeat),
                             static_cast<std::uint8_t>(b.burstCount), 0, b.intervalMs});
    }
    switch (nexus_validate_bindings(rustBindings.constData(), static_cast<std::size_t>(rustBindings.size()))) {
    case 0: return true;
    case 1: if (error) *error = QStringLiteral("Too many keymap markers (maximum 128)."); break;
    case 2: if (error) *error = QStringLiteral("Invalid key, position, burst count, or repeat interval."); break;
    case 3: if (error) *error = QStringLiteral("Markers for the same key must use the same repeat and burst settings."); break;
    default: if (error) *error = QStringLiteral("Could not validate keymap."); break;
    }
    return false;
}

bool validateKeymapProfile(const KeymapProfile &profile, QString *error) {
    if (profile.overlayOpacity < 0 || profile.overlayOpacity > 100) {
        if (error) *error = QStringLiteral("Overlay opacity must be 0..100%.");
        return false;
    }
    if (!validBindings(profile.taps, error)) return false;
    if (profile.dpads.size() > 8 || profile.swipes.size() > 64 || profile.zooms.size() > 8 ||
        profile.mobaSkills.size() > 24 || profile.scripts.size() > 32) {
        if (error) *error = QStringLiteral("Profile exceeds maximum control limits.");
        return false;
    }
    for (const auto &d : profile.dpads) {
        if (!validKey(d.keyUp) || !validKey(d.keyDown) || !validKey(d.keyLeft) || !validKey(d.keyRight) ||
            !validOptionalKey(d.sprintKey) || !validOptionalKey(d.walkKey) ||
            !validUnit(d.x) || !validUnit(d.y) ||
            !std::isfinite(d.radius) || d.radius < 0.02 || d.radius > 0.45 ||
            !std::isfinite(d.sprintMultiplier) || d.sprintMultiplier < 1.0 || d.sprintMultiplier > 2.5 ||
            !std::isfinite(d.walkMultiplier) || d.walkMultiplier < 0.2 || d.walkMultiplier > 1.0) {
            if (error) *error = QStringLiteral("Invalid D-pad keys, coordinates, or radius settings.");
            return false;
        }
        const int dirs[4] = {canonicalModifierGroup(d.keyUp), canonicalModifierGroup(d.keyDown),
                             canonicalModifierGroup(d.keyLeft), canonicalModifierGroup(d.keyRight)};
        for (int i = 0; i < 4; ++i)
            for (int j = i + 1; j < 4; ++j)
                if (dirs[i] == dirs[j]) {
                    if (error) *error = QStringLiteral("D-pad Up/Down/Left/Right keys must be distinct.");
                    return false;
                }
    }
    for (const auto &sw : profile.swipes) {
        if (!validKey(sw.key) || !validUnit(sw.x1) || !validUnit(sw.y1) || !validUnit(sw.x2) || !validUnit(sw.y2) ||
            sw.durationMs < 30 || sw.durationMs > 5000) {
            if (error) *error = QStringLiteral("Invalid swipe key, coordinates, or duration.");
            return false;
        }
    }
    for (const auto &z : profile.zooms) {
        if (!validKey(z.keyIn) || !validKey(z.keyOut) || keysMatch(z.keyIn, z.keyOut) ||
            !validOptionalKey(z.modifierKey) || !validUnit(z.x) || !validUnit(z.y) ||
            !std::isfinite(z.radius) || z.radius < 0.04 || z.radius > 0.45 ||
            z.durationMs < 40 || z.durationMs > 3000) {
            if (error) *error = QStringLiteral("Invalid pinch-zoom keys, radius, or duration.");
            return false;
        }
    }
    for (const auto &m : profile.mobaSkills) {
        if (!validKey(m.key) || !validOptionalKey(m.cancelKey) ||
            !validUnit(m.x) || !validUnit(m.y) || !validUnit(m.originX) || !validUnit(m.originY) ||
            (m.cancelEnabled && (!validKey(m.cancelKey) || !validUnit(m.cancelX) || !validUnit(m.cancelY))) ||
            !std::isfinite(m.radius) || m.radius < 0.02 || m.radius > 0.35 ||
            m.quickCastHoldMs < 20 || m.quickCastHoldMs > 1000) {
            if (error) *error = QStringLiteral("Invalid MOBA skill key, origin, cancel target, or radius.");
            return false;
        }
    }
    for (const auto &sc : profile.scripts) {
        if (!validKey(sc.key) || !validUnit(sc.x) || !validUnit(sc.y)) {
            if (error) *error = QStringLiteral("Invalid script trigger key or overlay position.");
            return false;
        }
        QString scriptErr;
        if (!parseScriptText(sc.scriptText, nullptr, &scriptErr)) {
            if (error) *error = QStringLiteral("Script '%1': %2").arg(sc.label.isEmpty() ? keyName(sc.key) : sc.label, scriptErr);
            return false;
        }
    }
    const auto &a = profile.aim;
    if (!validKey(a.toggleKey) || !validOptionalKey(a.suspendKey) || !validOptionalKey(a.freeLookKey) ||
        !std::isfinite(a.anchorX) || a.anchorX < 0.1 || a.anchorX > 0.9 ||
        !std::isfinite(a.anchorY) || a.anchorY < 0.1 || a.anchorY > 0.9 ||
        !std::isfinite(a.sensitivityX) || a.sensitivityX < 0.05 || a.sensitivityX > 10.0 ||
        !std::isfinite(a.sensitivityY) || a.sensitivityY < 0.05 || a.sensitivityY > 10.0 ||
        !validUnit(a.fireX) || !validUnit(a.fireY) || !validUnit(a.adsX) || !validUnit(a.adsY) ||
        !validUnit(a.freeLookX) || !validUnit(a.freeLookY) ||
        a.crosshairSize < 4 || a.crosshairSize > 64) {
        if (error) *error = QStringLiteral("Invalid aim, ADS, free-look, or crosshair settings.");
        return false;
    }
    return true;
}

QStringList detectKeyConflicts(const KeymapProfile &profile) {
    QHash<int, QStringList> owners;
    auto record = [&owners](int key, const QString &desc) {
        if (key <= 0) return;
        const int canon = canonicalModifierGroup(key);
        if (!owners[canon].contains(desc)) owners[canon].append(desc);
    };
    for (const auto &t : profile.taps) record(t.key, QStringLiteral("Tap (%1)").arg(t.label.isEmpty() ? keyName(t.key) : t.label));
    for (int i = 0; i < profile.dpads.size(); ++i) {
        const auto &d = profile.dpads[i];
        const QString tag = QStringLiteral("D-Pad #%1").arg(i + 1);
        record(d.keyUp, tag + " Up");
        record(d.keyDown, tag + " Down");
        record(d.keyLeft, tag + " Left");
        record(d.keyRight, tag + " Right");
    }
    for (int i = 0; i < profile.swipes.size(); ++i)
        record(profile.swipes[i].key, QStringLiteral("Swipe #%1").arg(i + 1));
    for (int i = 0; i < profile.zooms.size(); ++i) {
        if (profile.zooms[i].modifierKey == 0) {
            record(profile.zooms[i].keyIn, QStringLiteral("Zoom #%1 In").arg(i + 1));
            record(profile.zooms[i].keyOut, QStringLiteral("Zoom #%1 Out").arg(i + 1));
        }
    }
    for (int i = 0; i < profile.mobaSkills.size(); ++i)
        record(profile.mobaSkills[i].key, QStringLiteral("MOBA Skill #%1").arg(i + 1));
    for (int i = 0; i < profile.scripts.size(); ++i)
        record(profile.scripts[i].key, QStringLiteral("Script #%1").arg(i + 1));
    if (profile.aim.enabled) {
        record(profile.aim.toggleKey, QStringLiteral("Aim Toggle"));
        if (profile.aim.suspendKey != 0) record(profile.aim.suspendKey, QStringLiteral("Aim Suspend"));
        if (profile.aim.freeLookEnabled && profile.aim.freeLookKey != 0) record(profile.aim.freeLookKey, QStringLiteral("Free-Look"));
        if (profile.aim.adsEnabled) record(MouseRightButton, QStringLiteral("Aim ADS (Right Click)"));
    }
    QStringList warnings;
    for (auto it = owners.cbegin(); it != owners.cend(); ++it) {
        if (it.value().size() > 1) {
            warnings.append(QStringLiteral("%1 is shared by: %2").arg(keyName(it.key()), it.value().join(QStringLiteral(", "))));
        }
    }
    warnings.sort();
    return warnings;
}

static bool parseJsonInt(const QJsonValue &v, int minVal, int maxVal, int *out) {
    if (!v.isDouble()) return false;
    const double raw = v.toDouble();
    if (!std::isfinite(raw) || raw != std::floor(raw) || raw < minVal || raw > maxVal) return false;
    *out = static_cast<int>(raw);
    return true;
}

bool loadKeymapProfile(const QString &path, KeymapProfile *profile, QString *error) {
    QFile f(path);
    if (!f.exists()) {
        *profile = KeymapProfile{};
        return true;
    }
    if (!f.open(QIODevice::ReadOnly)) {
        if (error) *error = f.errorString();
        return false;
    }
    if (f.size() > 262144) {
        if (error) *error = QStringLiteral("Profile is too large.");
        return false;
    }
    QJsonParseError parse;
    const auto doc = QJsonDocument::fromJson(f.readAll(), &parse);
    if (parse.error != QJsonParseError::NoError || !doc.isObject()) {
        if (error) *error = QStringLiteral("Invalid keymap profile.");
        return false;
    }
    const auto root = doc.object();
    const int version = root.value(QStringLiteral("version")).toInt(0);
    if ((version < 1 || version > 3) || !root.value(QStringLiteral("bindings")).isArray()) {
        if (error) *error = QStringLiteral("Invalid keymap profile.");
        return false;
    }

    KeymapProfile loaded;
    loaded.name = root.value(QStringLiteral("name")).toString(QFileInfo(path).completeBaseName());
    if (loaded.name.trimmed().isEmpty()) loaded.name = QStringLiteral("Default");
    if (root.contains(QStringLiteral("overlayOpacity"))) {
        if (!parseJsonInt(root.value(QStringLiteral("overlayOpacity")), 0, 100, &loaded.overlayOpacity)) {
            if (error) *error = QStringLiteral("Invalid overlay opacity.");
            return false;
        }
    }

    const auto entries = root.value(QStringLiteral("bindings")).toArray();
    if (entries.size() > 128) {
        if (error) *error = QStringLiteral("Too many key bindings.");
        return false;
    }
    for (const auto &entry : entries) {
        if (!entry.isObject()) { if (error) *error = QStringLiteral("Invalid key binding."); return false; }
        const auto o = entry.toObject();
        int key = 0;
        if (!parseJsonInt(o.value(QStringLiteral("key")), 0, 0x1FF, &key) ||
            !o.value(QStringLiteral("x")).isDouble() || !o.value(QStringLiteral("y")).isDouble()) {
            if (error) *error = QStringLiteral("Invalid key binding fields.");
            return false;
        }
        KeyBinding b;
        b.key = key;
        b.x = o.value(QStringLiteral("x")).toDouble();
        b.y = o.value(QStringLiteral("y")).toDouble();
        if (version >= 2) {
            if (!o.value(QStringLiteral("repeat")).isBool() ||
                !parseJsonInt(o.value(QStringLiteral("intervalMs")), 25, 5000, &b.intervalMs)) {
                if (error) *error = QStringLiteral("Invalid repeat settings.");
                return false;
            }
            b.repeat = o.value(QStringLiteral("repeat")).toBool();
        }
        if (version >= 3) {
            if (o.contains(QStringLiteral("burstCount")) &&
                !parseJsonInt(o.value(QStringLiteral("burstCount")), 1, 50, &b.burstCount)) {
                if (error) *error = QStringLiteral("Invalid burst count.");
                return false;
            }
            b.label = o.value(QStringLiteral("label")).toString();
            b.showOverlay = o.value(QStringLiteral("showOverlay")).toBool(true);
        }
        loaded.taps.append(b);
    }

    if (version >= 3) {
        for (const auto &v : root.value(QStringLiteral("dpads")).toArray()) {
            if (!v.isObject()) { if (error) *error = QStringLiteral("Invalid D-pad entry."); return false; }
            const auto o = v.toObject();
            DpadControl d;
            if (!parseJsonInt(o.value(QStringLiteral("keyUp")), 1, 0x1FF, &d.keyUp) ||
                !parseJsonInt(o.value(QStringLiteral("keyDown")), 1, 0x1FF, &d.keyDown) ||
                !parseJsonInt(o.value(QStringLiteral("keyLeft")), 1, 0x1FF, &d.keyLeft) ||
                !parseJsonInt(o.value(QStringLiteral("keyRight")), 1, 0x1FF, &d.keyRight) ||
                !parseJsonInt(o.value(QStringLiteral("sprintKey")), 0, 0x1FF, &d.sprintKey) ||
                !parseJsonInt(o.value(QStringLiteral("walkKey")), 0, 0x1FF, &d.walkKey)) {
                if (error) *error = QStringLiteral("Invalid D-pad key codes.");
                return false;
            }
            d.x = o.value(QStringLiteral("x")).toDouble(0.18);
            d.y = o.value(QStringLiteral("y")).toDouble(0.74);
            d.radius = o.value(QStringLiteral("radius")).toDouble(0.10);
            d.sprintMultiplier = o.value(QStringLiteral("sprintMultiplier")).toDouble(1.45);
            d.walkMultiplier = o.value(QStringLiteral("walkMultiplier")).toDouble(0.45);
            d.label = o.value(QStringLiteral("label")).toString(QStringLiteral("Move"));
            d.showOverlay = o.value(QStringLiteral("showOverlay")).toBool(true);
            loaded.dpads.append(d);
        }
        for (const auto &v : root.value(QStringLiteral("swipes")).toArray()) {
            if (!v.isObject()) { if (error) *error = QStringLiteral("Invalid swipe entry."); return false; }
            const auto o = v.toObject();
            SwipeControl sw;
            if (!parseJsonInt(o.value(QStringLiteral("key")), 1, 0x1FF, &sw.key) ||
                !parseJsonInt(o.value(QStringLiteral("durationMs")), 30, 5000, &sw.durationMs)) {
                if (error) *error = QStringLiteral("Invalid swipe fields.");
                return false;
            }
            sw.x1 = o.value(QStringLiteral("x1")).toDouble();
            sw.y1 = o.value(QStringLiteral("y1")).toDouble();
            sw.x2 = o.value(QStringLiteral("x2")).toDouble();
            sw.y2 = o.value(QStringLiteral("y2")).toDouble();
            sw.holdAtEnd = o.value(QStringLiteral("holdAtEnd")).toBool(false);
            sw.label = o.value(QStringLiteral("label")).toString();
            sw.showOverlay = o.value(QStringLiteral("showOverlay")).toBool(true);
            loaded.swipes.append(sw);
        }
        for (const auto &v : root.value(QStringLiteral("zooms")).toArray()) {
            if (!v.isObject()) { if (error) *error = QStringLiteral("Invalid zoom entry."); return false; }
            const auto o = v.toObject();
            ZoomControl z;
            if (!parseJsonInt(o.value(QStringLiteral("keyIn")), 1, 0x1FF, &z.keyIn) ||
                !parseJsonInt(o.value(QStringLiteral("keyOut")), 1, 0x1FF, &z.keyOut) ||
                !parseJsonInt(o.value(QStringLiteral("modifierKey")), 0, 0x1FF, &z.modifierKey) ||
                !parseJsonInt(o.value(QStringLiteral("durationMs")), 40, 3000, &z.durationMs)) {
                if (error) *error = QStringLiteral("Invalid zoom fields.");
                return false;
            }
            z.x = o.value(QStringLiteral("x")).toDouble(0.5);
            z.y = o.value(QStringLiteral("y")).toDouble(0.5);
            z.radius = o.value(QStringLiteral("radius")).toDouble(0.14);
            z.label = o.value(QStringLiteral("label")).toString(QStringLiteral("Zoom"));
            z.showOverlay = o.value(QStringLiteral("showOverlay")).toBool(true);
            loaded.zooms.append(z);
        }
        for (const auto &v : root.value(QStringLiteral("mobaSkills")).toArray()) {
            if (!v.isObject()) { if (error) *error = QStringLiteral("Invalid MOBA skill entry."); return false; }
            const auto o = v.toObject();
            MobaSkillControl m;
            int mode = 1;
            if (!parseJsonInt(o.value(QStringLiteral("key")), 1, 0x1FF, &m.key) ||
                !parseJsonInt(o.value(QStringLiteral("cancelKey")), 0, 0x1FF, &m.cancelKey) ||
                !parseJsonInt(o.value(QStringLiteral("mode")), 0, 2, &mode) ||
                !parseJsonInt(o.value(QStringLiteral("quickCastHoldMs")), 20, 1000, &m.quickCastHoldMs)) {
                if (error) *error = QStringLiteral("Invalid MOBA skill fields.");
                return false;
            }
            m.mode = static_cast<MobaCastMode>(mode);
            m.x = o.value(QStringLiteral("x")).toDouble(0.78);
            m.y = o.value(QStringLiteral("y")).toDouble(0.72);
            m.radius = o.value(QStringLiteral("radius")).toDouble(0.11);
            m.originX = o.value(QStringLiteral("originX")).toDouble(0.50);
            m.originY = o.value(QStringLiteral("originY")).toDouble(0.50);
            m.cancelX = o.value(QStringLiteral("cancelX")).toDouble(0.89);
            m.cancelY = o.value(QStringLiteral("cancelY")).toDouble(0.22);
            m.cancelEnabled = o.value(QStringLiteral("cancelEnabled")).toBool(false);
            m.label = o.value(QStringLiteral("label")).toString();
            m.showOverlay = o.value(QStringLiteral("showOverlay")).toBool(true);
            loaded.mobaSkills.append(m);
        }
        for (const auto &v : root.value(QStringLiteral("scripts")).toArray()) {
            if (!v.isObject()) { if (error) *error = QStringLiteral("Invalid script entry."); return false; }
            const auto o = v.toObject();
            ScriptControl sc;
            if (!parseJsonInt(o.value(QStringLiteral("key")), 1, 0x1FF, &sc.key)) {
                if (error) *error = QStringLiteral("Invalid script key.");
                return false;
            }
            sc.x = o.value(QStringLiteral("x")).toDouble(0.5);
            sc.y = o.value(QStringLiteral("y")).toDouble(0.5);
            sc.repeatWhileHeld = o.value(QStringLiteral("repeatWhileHeld")).toBool(false);
            sc.label = o.value(QStringLiteral("label")).toString(QStringLiteral("Macro"));
            sc.scriptText = o.value(QStringLiteral("scriptText")).toString();
            sc.showOverlay = o.value(QStringLiteral("showOverlay")).toBool(true);
            loaded.scripts.append(sc);
        }
        if (root.value(QStringLiteral("aim")).isObject()) {
            const auto ao = root.value(QStringLiteral("aim")).toObject();
            auto &a = loaded.aim;
            int style = 0;
            if (!parseJsonInt(ao.value(QStringLiteral("toggleKey")), 1, 0x1FF, &a.toggleKey) ||
                !parseJsonInt(ao.value(QStringLiteral("suspendKey")), 0, 0x1FF, &a.suspendKey) ||
                !parseJsonInt(ao.value(QStringLiteral("freeLookKey")), 0, 0x1FF, &a.freeLookKey) ||
                !parseJsonInt(ao.value(QStringLiteral("crosshairStyle")), 0, 3, &style) ||
                !parseJsonInt(ao.value(QStringLiteral("crosshairSize")), 4, 64, &a.crosshairSize)) {
                if (error) *error = QStringLiteral("Invalid aim settings in profile.");
                return false;
            }
            a.enabled = ao.value(QStringLiteral("enabled")).toBool(false);
            a.anchorX = ao.value(QStringLiteral("anchorX")).toDouble(0.56);
            a.anchorY = ao.value(QStringLiteral("anchorY")).toDouble(0.48);
            a.sensitivityX = ao.value(QStringLiteral("sensitivityX")).toDouble(1.0);
            a.sensitivityY = ao.value(QStringLiteral("sensitivityY")).toDouble(1.0);
            a.fireEnabled = ao.value(QStringLiteral("fireEnabled")).toBool(true);
            a.fireX = ao.value(QStringLiteral("fireX")).toDouble(0.82);
            a.fireY = ao.value(QStringLiteral("fireY")).toDouble(0.74);
            a.adsEnabled = ao.value(QStringLiteral("adsEnabled")).toBool(false);
            a.adsHoldMode = ao.value(QStringLiteral("adsHoldMode")).toBool(false);
            a.adsX = ao.value(QStringLiteral("adsX")).toDouble(0.90);
            a.adsY = ao.value(QStringLiteral("adsY")).toDouble(0.56);
            a.freeLookEnabled = ao.value(QStringLiteral("freeLookEnabled")).toBool(false);
            a.freeLookX = ao.value(QStringLiteral("freeLookX")).toDouble(0.76);
            a.freeLookY = ao.value(QStringLiteral("freeLookY")).toDouble(0.26);
            a.showCrosshair = ao.value(QStringLiteral("showCrosshair")).toBool(true);
            a.crosshairStyle = static_cast<CrosshairStyle>(style);
            a.crosshairColor = ao.value(QStringLiteral("crosshairColor")).toString(QStringLiteral("#7cf7c4"));
            a.showOverlay = ao.value(QStringLiteral("showOverlay")).toBool(true);
        }
    }

    if (!validateKeymapProfile(loaded, error)) return false;
    *profile = loaded;
    return true;
}

bool saveKeymapProfile(const QString &path, const KeymapProfile &profile, QString *error) {
    if (!validateKeymapProfile(profile, error)) return false;
    QJsonArray tapEntries;
    for (const auto &b : profile.taps) {
        tapEntries.append(QJsonObject{
            {QStringLiteral("key"), b.key},
            {QStringLiteral("x"), b.x},
            {QStringLiteral("y"), b.y},
            {QStringLiteral("repeat"), b.repeat},
            {QStringLiteral("intervalMs"), b.intervalMs},
            {QStringLiteral("burstCount"), b.burstCount},
            {QStringLiteral("label"), b.label},
            {QStringLiteral("showOverlay"), b.showOverlay}
        });
    }
    QJsonArray dpadEntries;
    for (const auto &d : profile.dpads) {
        dpadEntries.append(QJsonObject{
            {QStringLiteral("keyUp"), d.keyUp},
            {QStringLiteral("keyDown"), d.keyDown},
            {QStringLiteral("keyLeft"), d.keyLeft},
            {QStringLiteral("keyRight"), d.keyRight},
            {QStringLiteral("sprintKey"), d.sprintKey},
            {QStringLiteral("walkKey"), d.walkKey},
            {QStringLiteral("x"), d.x},
            {QStringLiteral("y"), d.y},
            {QStringLiteral("radius"), d.radius},
            {QStringLiteral("sprintMultiplier"), d.sprintMultiplier},
            {QStringLiteral("walkMultiplier"), d.walkMultiplier},
            {QStringLiteral("label"), d.label},
            {QStringLiteral("showOverlay"), d.showOverlay}
        });
    }
    QJsonArray swipeEntries;
    for (const auto &sw : profile.swipes) {
        swipeEntries.append(QJsonObject{
            {QStringLiteral("key"), sw.key},
            {QStringLiteral("x1"), sw.x1},
            {QStringLiteral("y1"), sw.y1},
            {QStringLiteral("x2"), sw.x2},
            {QStringLiteral("y2"), sw.y2},
            {QStringLiteral("durationMs"), sw.durationMs},
            {QStringLiteral("holdAtEnd"), sw.holdAtEnd},
            {QStringLiteral("label"), sw.label},
            {QStringLiteral("showOverlay"), sw.showOverlay}
        });
    }
    QJsonArray zoomEntries;
    for (const auto &z : profile.zooms) {
        zoomEntries.append(QJsonObject{
            {QStringLiteral("keyIn"), z.keyIn},
            {QStringLiteral("keyOut"), z.keyOut},
            {QStringLiteral("modifierKey"), z.modifierKey},
            {QStringLiteral("x"), z.x},
            {QStringLiteral("y"), z.y},
            {QStringLiteral("radius"), z.radius},
            {QStringLiteral("durationMs"), z.durationMs},
            {QStringLiteral("label"), z.label},
            {QStringLiteral("showOverlay"), z.showOverlay}
        });
    }
    QJsonArray mobaEntries;
    for (const auto &m : profile.mobaSkills) {
        mobaEntries.append(QJsonObject{
            {QStringLiteral("key"), m.key},
            {QStringLiteral("cancelKey"), m.cancelKey},
            {QStringLiteral("x"), m.x},
            {QStringLiteral("y"), m.y},
            {QStringLiteral("radius"), m.radius},
            {QStringLiteral("originX"), m.originX},
            {QStringLiteral("originY"), m.originY},
            {QStringLiteral("cancelX"), m.cancelX},
            {QStringLiteral("cancelY"), m.cancelY},
            {QStringLiteral("cancelEnabled"), m.cancelEnabled},
            {QStringLiteral("mode"), static_cast<int>(m.mode)},
            {QStringLiteral("quickCastHoldMs"), m.quickCastHoldMs},
            {QStringLiteral("label"), m.label},
            {QStringLiteral("showOverlay"), m.showOverlay}
        });
    }
    QJsonArray scriptEntries;
    for (const auto &sc : profile.scripts) {
        scriptEntries.append(QJsonObject{
            {QStringLiteral("key"), sc.key},
            {QStringLiteral("x"), sc.x},
            {QStringLiteral("y"), sc.y},
            {QStringLiteral("repeatWhileHeld"), sc.repeatWhileHeld},
            {QStringLiteral("label"), sc.label},
            {QStringLiteral("scriptText"), sc.scriptText},
            {QStringLiteral("showOverlay"), sc.showOverlay}
        });
    }
    const auto &a = profile.aim;
    const QJsonObject aimObj{
        {QStringLiteral("enabled"), a.enabled},
        {QStringLiteral("toggleKey"), a.toggleKey},
        {QStringLiteral("suspendKey"), a.suspendKey},
        {QStringLiteral("anchorX"), a.anchorX},
        {QStringLiteral("anchorY"), a.anchorY},
        {QStringLiteral("sensitivityX"), a.sensitivityX},
        {QStringLiteral("sensitivityY"), a.sensitivityY},
        {QStringLiteral("fireEnabled"), a.fireEnabled},
        {QStringLiteral("fireX"), a.fireX},
        {QStringLiteral("fireY"), a.fireY},
        {QStringLiteral("adsEnabled"), a.adsEnabled},
        {QStringLiteral("adsHoldMode"), a.adsHoldMode},
        {QStringLiteral("adsX"), a.adsX},
        {QStringLiteral("adsY"), a.adsY},
        {QStringLiteral("freeLookEnabled"), a.freeLookEnabled},
        {QStringLiteral("freeLookKey"), a.freeLookKey},
        {QStringLiteral("freeLookX"), a.freeLookX},
        {QStringLiteral("freeLookY"), a.freeLookY},
        {QStringLiteral("showCrosshair"), a.showCrosshair},
        {QStringLiteral("crosshairStyle"), static_cast<int>(a.crosshairStyle)},
        {QStringLiteral("crosshairSize"), a.crosshairSize},
        {QStringLiteral("crosshairColor"), a.crosshairColor},
        {QStringLiteral("showOverlay"), a.showOverlay}
    };

    const QJsonObject root{
        {QStringLiteral("version"), 3},
        {QStringLiteral("name"), profile.name.trimmed().isEmpty() ? QStringLiteral("Default") : profile.name.trimmed()},
        {QStringLiteral("overlayOpacity"), profile.overlayOpacity},
        {QStringLiteral("bindings"), tapEntries},
        {QStringLiteral("dpads"), dpadEntries},
        {QStringLiteral("swipes"), swipeEntries},
        {QStringLiteral("zooms"), zoomEntries},
        {QStringLiteral("mobaSkills"), mobaEntries},
        {QStringLiteral("scripts"), scriptEntries},
        {QStringLiteral("aim"), aimObj}
    };

    QFileInfo info(path);
    if (info.dir().path() != QStringLiteral(".")) QDir().mkpath(info.dir().path());
    QSaveFile f(path);
    if (!f.open(QIODevice::WriteOnly)) {
        if (error) *error = f.errorString();
        return false;
    }
    const QByteArray bytes = QJsonDocument(root).toJson(QJsonDocument::Indented);
    if (f.write(bytes) != bytes.size() || !f.commit()) {
        if (error) *error = f.errorString();
        return false;
    }
    return true;
}

bool loadKeymap(const QString &path, QList<KeyBinding> *bindings, QString *error) {
    KeymapProfile profile;
    if (!loadKeymapProfile(path, &profile, error)) return false;
    *bindings = profile.taps;
    return true;
}

bool saveKeymap(const QString &path, const QList<KeyBinding> &bindings, QString *error) {
    KeymapProfile profile;
    if (QFile::exists(path)) {
        QString ignore;
        loadKeymapProfile(path, &profile, &ignore);
    }
    profile.taps = bindings;
    return saveKeymapProfile(path, profile, error);
}

KeymapProfile makePresetProfile(const QString &presetId) {
    KeymapProfile p;
    if (presetId == QStringLiteral("fps")) {
        p.name = QStringLiteral("Battle Royale / FPS");
        DpadControl move;
        move.x = 0.18; move.y = 0.74; move.radius = 0.10;
        move.sprintKey = 0x10; move.sprintMultiplier = 1.45;
        move.walkKey = 0x11; move.walkMultiplier = 0.45;
        move.label = QStringLiteral("Move");
        p.dpads.append(move);

        p.aim.enabled = true;
        p.aim.toggleKey = 0x77; // F8
        p.aim.suspendKey = 0x09; // Tab
        p.aim.anchorX = 0.56; p.aim.anchorY = 0.48;
        p.aim.sensitivityX = 1.15; p.aim.sensitivityY = 0.95;
        p.aim.fireEnabled = true; p.aim.fireX = 0.83; p.aim.fireY = 0.73;
        p.aim.adsEnabled = true; p.aim.adsHoldMode = false; p.aim.adsX = 0.91; p.aim.adsY = 0.56;
        p.aim.freeLookEnabled = true; p.aim.freeLookKey = 0x12; p.aim.freeLookX = 0.77; p.aim.freeLookY = 0.25;
        p.aim.showCrosshair = true;

        p.taps = {
            {0x20, 0.92, 0.74, false, 250, 1, QStringLiteral("Jump"), true},
            {'C',  0.86, 0.88, false, 250, 1, QStringLiteral("Crouch"), true},
            {'Z',  0.94, 0.88, false, 250, 1, QStringLiteral("Prone"), true},
            {'R',  0.75, 0.84, false, 250, 1, QStringLiteral("Reload"), true},
            {'F',  0.69, 0.60, false, 250, 1, QStringLiteral("Interact"), true},
            {'G',  0.71, 0.88, false, 250, 1, QStringLiteral("Grenade"), true},
            {'1',  0.44, 0.89, false, 250, 1, QStringLiteral("Weapon 1"), true},
            {'2',  0.53, 0.89, false, 250, 1, QStringLiteral("Weapon 2"), true},
            {'4',  0.34, 0.89, false, 250, 1, QStringLiteral("Heal"), true},
            {'M',  0.91, 0.14, false, 250, 1, QStringLiteral("Map"), true},
        };
    } else if (presetId == QStringLiteral("moba")) {
        p.name = QStringLiteral("MOBA 5v5 Arena");
        DpadControl move;
        move.x = 0.17; move.y = 0.76; move.radius = 0.11;
        move.sprintKey = 0; move.walkKey = 0;
        p.dpads.append(move);

        auto addSkill = [&p](int key, double x, double y, const QString &lbl) {
            MobaSkillControl s;
            s.key = key; s.x = x; s.y = y; s.radius = 0.11;
            s.originX = 0.50; s.originY = 0.50;
            s.cancelEnabled = true; s.cancelKey = 0x20; s.cancelX = 0.89; s.cancelY = 0.22;
            s.mode = MobaCastMode::ManualAimWhileHeld;
            s.label = lbl;
            p.mobaSkills.append(s);
        };
        addSkill('Q', 0.74, 0.84, QStringLiteral("Skill 1"));
        addSkill('E', 0.81, 0.69, QStringLiteral("Skill 2"));
        addSkill('R', 0.91, 0.60, QStringLiteral("Ultimate"));

        p.taps = {
            {MouseRightButton, 0.89, 0.83, false, 250, 1, QStringLiteral("Attack"), true},
            {'F', 0.65, 0.86, false, 250, 1, QStringLiteral("Spell"), true},
            {'B', 0.56, 0.88, false, 250, 1, QStringLiteral("Recall"), true},
            {'1', 0.86, 0.44, false, 250, 1, QStringLiteral("Active"), true},
        };
    } else if (presetId == QStringLiteral("arpg")) {
        p.name = QStringLiteral("Action RPG Combat");
        DpadControl move;
        move.x = 0.18; move.y = 0.75; move.radius = 0.10;
        move.sprintKey = 0; move.walkKey = 0;
        p.dpads.append(move);

        SwipeControl dodge;
        dodge.key = 0x10; // Shift
        dodge.x1 = 0.86; dodge.y1 = 0.82; dodge.x2 = 0.95; dodge.y2 = 0.72;
        dodge.durationMs = 110; dodge.label = QStringLiteral("Dodge");
        p.swipes.append(dodge);

        ZoomControl zoom;
        zoom.keyIn = MouseWheelUp; zoom.keyOut = MouseWheelDown; zoom.modifierKey = 0x11;
        zoom.x = 0.50; zoom.y = 0.50; zoom.radius = 0.15;
        p.zooms.append(zoom);

        ScriptControl combo;
        combo.key = 'X'; combo.x = 0.76; combo.y = 0.85;
        combo.label = QStringLiteral("Burst Combo");
        combo.scriptText = QStringLiteral("tap 88 80 45\nwait 60\ntap 88 80 45\nwait 60\ntap 81 68 65");
        p.scripts.append(combo);

        p.taps = {
            {'J', 0.88, 0.80, true, 90, 1, QStringLiteral("Auto Attack"), true},
            {'E', 0.81, 0.68, false, 250, 1, QStringLiteral("Skill"), true},
            {'Q', 0.91, 0.58, false, 250, 1, QStringLiteral("Burst"), true},
            {0x20, 0.93, 0.76, false, 250, 1, QStringLiteral("Jump"), true},
            {'1', 0.93, 0.24, false, 250, 1, QStringLiteral("Char 1"), true},
            {'2', 0.93, 0.34, false, 250, 1, QStringLiteral("Char 2"), true},
            {'3', 0.93, 0.44, false, 250, 1, QStringLiteral("Char 3"), true},
        };
    } else {
        p.name = QStringLiteral("Custom");
    }
    return p;
}

QString sanitizeSchemeName(const QString &rawName) {
    QString clean = rawName.trimmed();
    static const QRegularExpression badChars(QStringLiteral("[\\\\/:*?\"<>|.]"));
    clean.replace(badChars, QStringLiteral("_"));
    clean = clean.simplified();
    if (clean.isEmpty()) clean = QStringLiteral("default");
    if (clean.size() > 48) clean.truncate(48);
    return clean;
}

QString schemeFilePath(const QString &schemeDir, const QString &schemeName) {
    const QString safe = sanitizeSchemeName(schemeName);
    const QString file = (safe.compare(QStringLiteral("default"), Qt::CaseInsensitive) == 0)
        ? QStringLiteral("default.json")
        : safe + QStringLiteral(".json");
    return QDir(schemeDir).filePath(file);
}

QStringList listSavedSchemes(const QString &schemeDir) {
    QDir dir(schemeDir);
    QStringList names;
    if (dir.exists()) {
        const auto files = dir.entryInfoList({QStringLiteral("*.json")}, QDir::Files, QDir::Name | QDir::IgnoreCase);
        for (const auto &fi : files) names.append(fi.completeBaseName());
    }
    if (!names.contains(QStringLiteral("default"), Qt::CaseInsensitive))
        names.prepend(QStringLiteral("default"));
    return names;
}
