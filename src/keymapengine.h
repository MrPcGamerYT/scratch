#pragma once

#include "keymap.h"
#include <QElapsedTimer>
#include <QList>
#include <QObject>
#include <QPointF>
#include <QSet>
#include <QTimer>
#include <array>

class ClickSequence;

// Unified non-blocking keymap runtime supporting both 10-slot virtio multitouch
// and single-pointer USB-tablet fallback. Pure QtCore for headless testing.
class KeymapEngine : public QObject {
    Q_OBJECT
public:
    explicit KeymapEngine(QObject *parent = nullptr);

    void configure(const KeymapProfile &profile, bool multitouch);
    void setReservedTouchSlots(const QSet<int> &slotsReserved);
    bool isMultiTouch() const { return touchMode; }

    bool handleKeyPress(int key, QPointF cursorNorm = QPointF(0.5, 0.5));
    bool handleKeyRelease(int key);
    bool handleWheel(int wheelKey, QPointF cursorNorm = QPointF(0.5, 0.5));
    void handleCursorMove(QPointF cursorNorm);
    void cancelAll();
    bool active() const;

signals:
    void pointerDown(double x, double y);
    void pointerMove(double x, double y);
    void pointerUp();
    void touchDown(int slot, double x, double y);
    void touchMove(int slot, double x, double y);
    void touchUp(int slot);

private:
    static constexpr int SinglePointerContact = 100;

    int acquireContact();
    void contactDown(int contact, double x, double y);
    void contactMove(int contact, double x, double y);
    void contactUp(int contact);

    void tickAnimations();
    void updateDpad(int index);

    struct TapRunner {
        int key = 0;
        int contact = -1;
        bool contactPressed = false;
        ClickSequence *sequence = nullptr;
    };

    struct DpadState {
        bool up = false;
        bool down = false;
        bool left = false;
        bool right = false;
        bool sprint = false;
        bool walk = false;
        int contact = -1;
    };

    struct ActiveSwipe {
        int controlIndex = -1;
        int key = 0;
        int contact = -1;
        int durationMs = 150;
        bool holdAtEnd = false;
        bool keyHeld = true;
        bool reachedEnd = false;
        double x1 = 0, y1 = 0, x2 = 0, y2 = 0;
        QElapsedTimer elapsed;
    };

    struct ActiveZoom {
        int contactA = -1;
        int contactB = -1;
        double x = 0.5, y = 0.5, radius = 0.14;
        int durationMs = 180;
        bool zoomIn = true;
        QElapsedTimer elapsed;
    };

    struct ActiveMoba {
        int controlIndex = -1;
        int key = 0;
        int contact = -1;
        MobaSkillControl cfg;
        bool cancelling = false;
        bool autoReleasePending = false;
        int releaseAfterMs = 65;
        QElapsedTimer timer;
    };

    struct ActiveScript {
        int controlIndex = -1;
        int key = 0;
        bool repeatWhileHeld = false;
        bool keyHeld = true;
        QList<ScriptStep> steps;
        int stepIndex = 0;
        int primaryContact = -1;
        bool primaryDown = false;
        int secondaryContact = -1;
        bool secondaryDown = false;
        bool stepStarted = false;
        QElapsedTimer stepTimer;
        void releaseContacts(KeymapEngine *engine);
    };

    void advanceScripts();

    KeymapProfile currentProfile;
    bool touchMode = false;
    QSet<int> reservedSlots;
    std::array<bool, 10> slotInUse{};
    bool singlePointerInUse = false;

    QSet<int> heldModifiers;
    QPointF lastCursor{0.5, 0.5};

    std::array<TapRunner, 10> tapRunners;
    QList<DpadState> dpadStates;
    QList<ActiveSwipe> activeSwipes;
    QList<ActiveZoom> activeZooms;
    QList<ActiveMoba> activeMobas;
    QList<ActiveScript> activeScripts;

    QTimer frameTimer;
};
