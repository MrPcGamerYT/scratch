#include "keymap.h"
#include "clicksequence.h"
#include "keymapengine.h"
#include "multisequence.h"
#include "qmp.h"
#include <QCoreApplication>
#include <QEventLoop>
#include <QFile>
#include <QJsonArray>
#include <QJsonObject>
#include <QPointF>
#include <QTemporaryDir>
#include <QTimer>
#include <cmath>
#include <iostream>
#include <limits>

#define TEST_FAIL(code) do { std::cerr << "keymap_tests failed at line " << __LINE__ << " (code " << code << ")\n"; return code; } while (false)

static void wait(int ms) {
    QEventLoop loop;
    QTimer::singleShot(ms, &loop, &QEventLoop::quit);
    loop.exec();
}

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    QTemporaryDir tmp;
    if (!tmp.isValid()) TEST_FAIL(1);
    const QString path = tmp.filePath("profile.json");
    QString error;

    // 1. Legacy v1/v2 compatibility and validation
    QList<KeyBinding> bindings{{'W', 0.25, 0.75, true, 200}, {'W', 0.9, 0.1, true, 200}, {0x20, 1.0, 0.0}}, result;
    if (!saveKeymap(path, bindings, &error) || !loadKeymap(path, &result, &error) || result.size() != 3 || !result[0].repeat || result[1].key != 'W') TEST_FAIL(2);
    if (saveKeymap(path, {{'W', 0.5, 0.5, true, 200}, {'W', 0.1, 0.2, false, 200}}, &error)) TEST_FAIL(3);
    if (!loadKeymap(path, &result, &error) || result.size() != 3) TEST_FAIL(4);
    if (saveKeymap(path, {{'A', -0.1, 0.5}}, &error) || saveKeymap(path, {{10, 0.5, 0.5}}, &error) || saveKeymap(path, {{'A', 0.1, 0.5, true, 1}}, &error)) TEST_FAIL(5);
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) TEST_FAIL(6);
    file.write("{\"version\":1,\"bindings\":[{\"key\":65,\"x\":0.2,\"y\":0.3}]}\n");
    file.close();
    if (!loadKeymap(path, &result, &error) || result.size() != 1 || result[0].repeat || result[0].intervalMs != 250) TEST_FAIL(7);

    auto press = Qmp::pointerEvents(0.5, 1.0, true);
    auto release = Qmp::pointerEvents(0, 0, false);
    if (press.size() != 3 || release.size() != 1 || press[0].toObject().value("data").toObject().value("value").toInt() != 16384) TEST_FAIL(8);

    // 2. ClickSequence: hold, multi-marker sequential, burst N×, repeat-while-held, cancel
    ClickSequence seq;
    QString events;
    QObject::connect(&seq, &ClickSequence::pointerDown, &app, [&](double, double) { events += 'D'; });
    QObject::connect(&seq, &ClickSequence::pointerUp, &app, [&] { events += 'U'; });
    if (!seq.press('A', {{'A', 0.2, 0.3}})) TEST_FAIL(9);
    seq.release('A');
    if (events != "DU" || seq.active()) TEST_FAIL(10);

    events.clear();
    if (!seq.press('A', {{'A', 0.2, 0.3}, {'A', 0.8, 0.9}})) TEST_FAIL(11);
    seq.release('A');
    wait(260);
    if (events != "DUDU" || seq.active()) TEST_FAIL(12);

    events.clear();
    if (!seq.press('A', {{'A', 0.2, 0.3, true, 100}})) TEST_FAIL(13);
    wait(560);
    seq.release('A');
    wait(160);
    if (events.count('D') < 2 || events.count('D') != events.count('U') || seq.active()) TEST_FAIL(14);

    events.clear();
    seq.press('A', {{'A', 0.2, 0.3, true, 100}});
    seq.cancel();
    wait(120);
    if (events != "DU" || seq.active()) TEST_FAIL(15);

    // Burst 3 taps on quick press
    events.clear();
    KeyBinding burstBind{'B', 0.4, 0.6, false, 40, 3};
    if (!seq.press('B', {burstBind})) TEST_FAIL(16);
    seq.release('B');
    wait(240);
    if (events != "DUDUDU" || seq.active()) TEST_FAIL(17);

    if (saveKeymap(path, {{'A', std::numeric_limits<double>::quiet_NaN(), .5}}, &error)) TEST_FAIL(18);
    QList<KeyBinding> tooMany;
    for (int i = 0; i < 129; ++i) tooMany.append({'A', .5, .5});
    if (saveKeymap(path, tooMany, &error)) TEST_FAIL(19);

    // Extended key support (Enter, F8, MouseRight, WheelUp allowed; Escape, LWin, 0 rejected)
    if (!validKey('Z') || !validKey('0') || !validKey(0x0D) || !validKey(0x77) ||
        !validKey(MouseRightButton) || !validKey(MouseWheelUp) ||
        validKey(0x1B) || validKey(0x5B) || validKey(0))
        TEST_FAIL(20);

    // 3. MultiSequence & QMP touch framing
    MultiSequence multi;
    QList<int> downSlots, upSlots;
    QObject::connect(&multi, &MultiSequence::touchDown, &app, [&](int slot, double, double) { downSlots.append(slot); });
    QObject::connect(&multi, &MultiSequence::touchUp, &app, [&](int slot) { upSlots.append(slot); });
    QList<KeyBinding> simultaneous{{'A', .2, .3}, {'B', .6, .7}};
    if (!multi.press('A', simultaneous) || !multi.press('B', simultaneous) || downSlots != QList<int>{2, 3}) TEST_FAIL(21);
    multi.release('A');
    if (upSlots != QList<int>{2} || !multi.active()) TEST_FAIL(22);
    multi.cancel();
    if (upSlots != QList<int>{2, 3} || multi.active()) TEST_FAIL(23);
    const auto firstTouch = Qmp::touchEvents("begin", 0, .5, .25, true);
    if (firstTouch.size() != 4 || firstTouch[0].toObject().value("data").toObject().value("button").toString() != "touch" ||
        firstTouch[1].toObject().value("data").toObject().value("tracking-id").toInt() != 0) TEST_FAIL(24);
    const auto lastTouch = Qmp::touchEvents("end", 3, 0, 0, true);
    if (lastTouch.size() != 2 || lastTouch[0].toObject().value("data").toObject().value("tracking-id").toInt() != -1) TEST_FAIL(25);

    // 4. Full v3 Profile round-trip, presets, script parser, and conflict detection
    for (const QString &presetId : {QStringLiteral("fps"), QStringLiteral("moba"), QStringLiteral("arpg")}) {
        const KeymapProfile preset = makePresetProfile(presetId);
        const QString presetPath = tmp.filePath(presetId + ".json");
        KeymapProfile loaded;
        if (!saveKeymapProfile(presetPath, preset, &error) || !loadKeymapProfile(presetPath, &loaded, &error)) {
            std::cerr << "Preset failed (" << presetId.toStdString() << "): " << error.toStdString() << "\n";
            TEST_FAIL(26);
        }
        if (loaded.dpads.size() != preset.dpads.size() || loaded.mobaSkills.size() != preset.mobaSkills.size() ||
            loaded.scripts.size() != preset.scripts.size() || loaded.aim.enabled != preset.aim.enabled)
            TEST_FAIL(27);
        if (!detectKeyConflicts(preset).isEmpty()) TEST_FAIL(28);
    }

    QList<ScriptStep> parsedSteps;
    if (!parseScriptText("# comment\ntap 50 50 40\nwait 30\nswipe 20 80 20 30 80\nmultipress 20 70 80 70 40", &parsedSteps, &error) ||
        parsedSteps.size() != 4)
        TEST_FAIL(29);
    if (parseScriptText("unknowncmd 10 20", &parsedSteps, &error) || parseScriptText("tap 150 50", &parsedSteps, &error))
        TEST_FAIL(30);

    // 5. KeymapEngine integration: 8-way D-pad slide + sprint, Swipe-and-Hold, Pinch Zoom, MOBA Cancel, Script
    KeymapProfile rich;
    DpadControl dpad;
    dpad.x = 0.20; dpad.y = 0.80; dpad.radius = 0.10;
    dpad.sprintKey = 0x10; dpad.sprintMultiplier = 1.5;
    rich.dpads.append(dpad);

    SwipeControl holdSwipe;
    holdSwipe.key = 'E';
    holdSwipe.x1 = 0.50; holdSwipe.y1 = 0.70; holdSwipe.x2 = 0.50; holdSwipe.y2 = 0.30;
    holdSwipe.durationMs = 60; holdSwipe.holdAtEnd = true;
    rich.swipes.append(holdSwipe);

    ZoomControl zoom;
    zoom.keyIn = MouseWheelUp; zoom.keyOut = MouseWheelDown; zoom.modifierKey = 0;
    zoom.x = 0.50; zoom.y = 0.50; zoom.radius = 0.16; zoom.durationMs = 60;
    rich.zooms.append(zoom);

    MobaSkillControl moba;
    moba.key = 'Q'; moba.x = 0.80; moba.y = 0.75; moba.radius = 0.10;
    moba.originX = 0.50; moba.originY = 0.50;
    moba.cancelEnabled = true; moba.cancelKey = 0x20; moba.cancelX = 0.90; moba.cancelY = 0.20;
    moba.mode = MobaCastMode::ManualAimWhileHeld;
    rich.mobaSkills.append(moba);

    ScriptControl macro;
    macro.key = 'X'; macro.x = 0.70; macro.y = 0.80;
    macro.scriptText = QStringLiteral("tap 60 60 30\nwait 20\ntap 70 70 30");
    rich.scripts.append(macro);

    KeymapEngine engine;
    engine.configure(rich, true);

    int touchBegins = 0, touchMoves = 0, touchEnds = 0;
    QPointF lastMovePos;
    QObject::connect(&engine, &KeymapEngine::touchDown, &app, [&](int, double, double) { ++touchBegins; });
    QObject::connect(&engine, &KeymapEngine::touchMove, &app, [&](int, double x, double y) {
        ++touchMoves;
        lastMovePos = QPointF(x, y);
    });
    QObject::connect(&engine, &KeymapEngine::touchUp, &app, [&](int) { ++touchEnds; });

    // D-pad W -> W+D -> Shift sprint -> release all without lifting mid-transition!
    if (!engine.handleKeyPress('W')) TEST_FAIL(31);
    if (touchBegins != 1 || touchEnds != 0 || std::abs(lastMovePos.y() - 0.70) > 0.001) TEST_FAIL(32);
    if (!engine.handleKeyPress('D')) TEST_FAIL(33);
    if (touchBegins != 1 || touchEnds != 0) TEST_FAIL(34); // smooth slide, same contact!
    engine.handleKeyRelease('W');
    if (touchBegins != 1 || touchEnds != 0 || std::abs(lastMovePos.x() - 0.30) > 0.001) TEST_FAIL(35);
    engine.handleKeyPress(0x10); // Shift sprint expands reach to 0.20 + 0.10*1.5 = 0.35
    if (std::abs(lastMovePos.x() - 0.35) > 0.001) TEST_FAIL(36);
    engine.handleKeyRelease(0x10);
    engine.handleKeyRelease('D');
    if (touchEnds != 1 || engine.active()) TEST_FAIL(37);

    // Swipe & Hold stays pressed at endpoint until key release
    touchBegins = touchMoves = touchEnds = 0;
    if (!engine.handleKeyPress('E')) TEST_FAIL(38);
    wait(110);
    if (touchBegins != 1 || touchEnds != 0 || std::abs(lastMovePos.y() - 0.30) > 0.001) TEST_FAIL(39);
    engine.handleKeyRelease('E');
    if (touchEnds != 1 || engine.active()) TEST_FAIL(40);

    // Two-finger Pinch Zoom allocates 2 simultaneous touch slots
    touchBegins = touchMoves = touchEnds = 0;
    if (!engine.handleWheel(MouseWheelUp)) TEST_FAIL(41);
    if (touchBegins != 2) TEST_FAIL(42);
    wait(120);
    if (touchEnds != 2 || engine.active()) TEST_FAIL(43);

    // MOBA Manual Aim + live cursor update + Space Cancel moves to cancel target
    touchBegins = touchMoves = touchEnds = 0;
    if (!engine.handleKeyPress('Q', QPointF(0.90, 0.50))) TEST_FAIL(44);
    if (std::abs(lastMovePos.x() - 0.90) > 0.001) TEST_FAIL(45);
    engine.handleCursorMove(QPointF(0.50, 0.10));
    if (std::abs(lastMovePos.y() - 0.65) > 0.001) TEST_FAIL(46);
    if (!engine.handleKeyPress(0x20)) TEST_FAIL(47); // Cancel skill!
    if (std::abs(lastMovePos.x() - 0.90) > 0.001 || std::abs(lastMovePos.y() - 0.20) > 0.001) TEST_FAIL(48);
    wait(80);
    engine.handleKeyRelease(0x20);
    engine.handleKeyRelease('Q');
    if (touchEnds != 1 || engine.active()) TEST_FAIL(49);

    // Macro Script executes two sequential taps
    touchBegins = touchMoves = touchEnds = 0;
    if (!engine.handleKeyPress('X')) TEST_FAIL(50);
    engine.handleKeyRelease('X');
    wait(160);
    if (touchBegins != 2 || touchEnds != 2 || engine.active()) TEST_FAIL(51);

    std::cout << "Keymap v3 schemes, 8-way D-pad, swipe-hold, pinch-zoom, MOBA aim/cancel, burst and macro scripts passed.\n";
    return 0;
}
