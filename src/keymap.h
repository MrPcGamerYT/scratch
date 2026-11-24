#pragma once

#include <QList>
#include <QPair>
#include <QString>
#include <QStringList>
#include <Qt>

enum SpecialInputCode : int {
    MouseLeftButton = 0x107,
    MouseRightButton  = 0x101,
    MouseMiddleButton = 0x102,
    MouseX1Button     = 0x103,
    MouseX2Button     = 0x104,
    MouseWheelUp      = 0x105,
    MouseWheelDown    = 0x106,
};

struct KeyBinding {
    int key = 0;          // Win32 virtual key or SpecialInputCode.
    double x = 0;
    double y = 0;
    bool repeat = false;
    int intervalMs = 250; // 25..5000 ms
    int burstCount = 1;   // 1 = hold / sequential, 2..50 = burst taps on single press
    QString label;
    bool showOverlay = true;
};

struct DpadControl {
    int keyUp = 'W';
    int keyDown = 'S';
    int keyLeft = 'A';
    int keyRight = 'D';
    int sprintKey = 0x10; // VK_SHIFT (0 = disabled)
    int walkKey = 0;      // 0 = disabled
    double x = 0.18;
    double y = 0.74;
    double radius = 0.10;
    double sprintMultiplier = 1.45;
    double walkMultiplier = 0.45;
    QString label = QStringLiteral("Move");
    bool showOverlay = true;
};

struct SwipeControl {
    int key = 0;
    double x1 = 0.50;
    double y1 = 0.70;
    double x2 = 0.50;
    double y2 = 0.30;
    int durationMs = 150;
    bool holdAtEnd = false;
    QString label;
    bool showOverlay = true;
};

struct ZoomControl {
    int keyIn = MouseWheelUp;
    int keyOut = MouseWheelDown;
    int modifierKey = 0x11; // VK_CONTROL (0 = none)
    double x = 0.50;
    double y = 0.50;
    double radius = 0.14;
    int durationMs = 180;
    QString label = QStringLiteral("Zoom");
    bool showOverlay = true;
};

enum class MobaCastMode : int {
    QuickCast = 0,
    ManualAimWhileHeld = 1,
    AutoCastTap = 2,
};

struct MobaSkillControl {
    int key = 'Q';
    int cancelKey = 0x20; // Space (0 = disabled)
    double x = 0.78;
    double y = 0.72;
    double radius = 0.11;
    double originX = 0.50;
    double originY = 0.50;
    double cancelX = 0.89;
    double cancelY = 0.22;
    bool cancelEnabled = false;
    MobaCastMode mode = MobaCastMode::ManualAimWhileHeld;
    int quickCastHoldMs = 65;
    QString label;
    bool showOverlay = true;
};

enum class ScriptOp : int {
    Tap = 0,
    Wait = 1,
    Swipe = 2,
    Down = 3,
    Move = 4,
    Up = 5,
    MultiPress = 6,
};

struct ScriptStep {
    ScriptOp op = ScriptOp::Tap;
    int durationMs = 50;
    double x1 = 0.50;
    double y1 = 0.50;
    double x2 = 0.50;
    double y2 = 0.50;
};

struct ScriptControl {
    int key = 0;
    double x = 0.50;
    double y = 0.50;
    bool repeatWhileHeld = false;
    QString label = QStringLiteral("Macro");
    QString scriptText = QStringLiteral("tap 80 75 50\nwait 90\ntap 85 60 50");
    bool showOverlay = true;
};

enum class CrosshairStyle : int {
    CrossDot = 0,
    Cross = 1,
    Dot = 2,
    Circle = 3,
};

struct AimConfig {
    bool enabled = false;
    int toggleKey = 0x77;   // VK_F8
    int suspendKey = 0x09;  // VK_TAB (0 = disabled; hold to temporarily unlock cursor)
    double anchorX = 0.56;
    double anchorY = 0.48;
    double sensitivityX = 1.0;
    double sensitivityY = 1.0;
    bool fireEnabled = true;
    double fireX = 0.82;
    double fireY = 0.74;
    bool adsEnabled = false;
    bool adsHoldMode = false;
    double adsX = 0.90;
    double adsY = 0.56;
    bool freeLookEnabled = false;
    int freeLookKey = 0x12; // VK_MENU (Alt)
    double freeLookX = 0.76;
    double freeLookY = 0.26;
    bool showCrosshair = true;
    CrosshairStyle crosshairStyle = CrosshairStyle::CrossDot;
    int crosshairSize = 14;
    QString crosshairColor = QStringLiteral("#7cf7c4");
    bool showOverlay = true;
};

struct KeymapProfile {
    QString name = QStringLiteral("Default");
    int overlayOpacity = 72; // 0..100
    QList<KeyBinding> taps;
    QList<DpadControl> dpads;
    QList<SwipeControl> swipes;
    QList<ZoomControl> zooms;
    QList<MobaSkillControl> mobaSkills;
    QList<ScriptControl> scripts;
    AimConfig aim;

    bool hasAnyInputBindings() const {
        return !taps.isEmpty() || !dpads.isEmpty() || !swipes.isEmpty() ||
               !zooms.isEmpty() || !mobaSkills.isEmpty() || !scripts.isEmpty();
    }
    bool hasAnyControls() const {
        return hasAnyInputBindings() || aim.enabled;
    }
};

QString keyName(int key);
QString keyShortBadge(int key);
bool validKey(int key);
bool isMouseBindingKey(int key);
bool keysMatch(int boundKey, int eventKey);
int qtKeyToBindingKey(int qtKey, Qt::KeyboardModifiers modifiers = Qt::NoModifier);
int mouseButtonToBindingKey(Qt::MouseButton button);
QList<QPair<int, QString>> allBindableKeys();

bool parseScriptText(const QString &text, QList<ScriptStep> *steps, QString *error);
bool validateKeymapProfile(const KeymapProfile &profile, QString *error);
QStringList detectKeyConflicts(const KeymapProfile &profile);

bool loadKeymap(const QString &path, QList<KeyBinding> *bindings, QString *error);
bool saveKeymap(const QString &path, const QList<KeyBinding> &bindings, QString *error);
bool loadKeymapProfile(const QString &path, KeymapProfile *profile, QString *error);
bool saveKeymapProfile(const QString &path, const KeymapProfile &profile, QString *error);

KeymapProfile makePresetProfile(const QString &presetId);
QString sanitizeSchemeName(const QString &rawName);
QString schemeFilePath(const QString &schemeDir, const QString &schemeName);
QStringList listSavedSchemes(const QString &schemeDir);
