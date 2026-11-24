#include "keymapdialog.h"
#include "aimsettings.h"
#include <QButtonGroup>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QFileDialog>
#include <QFocusEvent>
#include <QFormLayout>
#include <QFrame>
#include <QHBoxLayout>
#include <QHash>
#include <QInputDialog>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QLineF>
#include <QListWidget>
#include <QMenu>
#include <QMessageBox>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QPainter>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QScrollArea>
#include <QShortcut>
#include <QSignalBlocker>
#include <QSlider>
#include <QSpinBox>
#include <QStackedWidget>
#include <QVBoxLayout>
#include <QWheelEvent>
#include <algorithm>
#include <cmath>
#include <functional>

class KeyBindButton : public QPushButton {
public:
    std::function<void(int)> keyChanged;
    explicit KeyBindButton(bool allowUnassigned = false, QWidget *parent = nullptr)
        : QPushButton(parent), allowNone(allowUnassigned) {
        setFocusPolicy(Qt::StrongFocus);
        updateText();
        connect(this, &QPushButton::clicked, this, [this] {
            listening = !listening;
            updateText();
            if (listening) setFocus(Qt::MouseFocusReason);
        });
    }
    void setBoundKey(int k) {
        current = k;
        listening = false;
        updateText();
    }
    int boundKey() const { return current; }

protected:
    void keyPressEvent(QKeyEvent *event) override {
        if (!listening) { QPushButton::keyPressEvent(event); return; }
        if (event->isAutoRepeat()) return;
        if (event->key() == Qt::Key_Escape) {
            listening = false;
            updateText();
            event->accept();
            return;
        }
        if (allowNone && (event->key() == Qt::Key_Backspace || event->key() == Qt::Key_Delete)) {
            current = 0;
            listening = false;
            updateText();
            if (keyChanged) keyChanged(current);
            event->accept();
            return;
        }
        const int mapped = qtKeyToBindingKey(event->key(), event->modifiers());
        if (validKey(mapped)) {
            current = mapped;
            listening = false;
            updateText();
            if (keyChanged) keyChanged(current);
        }
        event->accept();
    }
    void mousePressEvent(QMouseEvent *event) override {
        if (listening && event->button() != Qt::LeftButton) {
            const int mapped = mouseButtonToBindingKey(event->button());
            if (validKey(mapped)) {
                current = mapped;
                listening = false;
                updateText();
                if (keyChanged) keyChanged(current);
                event->accept();
                return;
            }
        }
        QPushButton::mousePressEvent(event);
    }
    void wheelEvent(QWheelEvent *event) override {
        if (listening && event->angleDelta().y() != 0) {
            current = event->angleDelta().y() > 0 ? MouseWheelUp : MouseWheelDown;
            listening = false;
            updateText();
            if (keyChanged) keyChanged(current);
            event->accept();
            return;
        }
        QPushButton::wheelEvent(event);
    }
    void focusOutEvent(QFocusEvent *event) override {
        if (listening) { listening = false; updateText(); }
        QPushButton::focusOutEvent(event);
    }

private:
    void updateText() {
        if (listening) {
            setText(allowNone ? "Press key/mouse (Bksp = None)…" : "Press any key or mouse…");
            setStyleSheet("background: #47387a; border: 1px solid #c3b2ff; color: #ffffff;");
        } else {
            setText(current == 0 ? QStringLiteral("Disabled") : keyName(current));
            setStyleSheet(QString());
        }
    }
    int current = 'A';
    bool allowNone = false;
    bool listening = false;
};

namespace {
enum class ToolKind : int {
    Select = 0,
    Tap = 1,
    Dpad = 2,
    Swipe = 3,
    Zoom = 4,
    Moba = 5,
    Script = 6,
};

enum class SelType : int {
    None = 0,
    Tap,
    Dpad,
    Swipe,
    Zoom,
    Moba,
    Script,
    Aim,
};

struct Selection {
    SelType type = SelType::None;
    int index = -1;
    bool operator==(const Selection &o) const { return type == o.type && index == o.index; }
};
}

class KeymapCanvas : public QWidget {
public:
    QPixmap image;
    KeymapProfile profile;
    ToolKind activeTool = ToolKind::Tap;
    Selection sel;
    bool snapToGrid = false;
    bool pickingScriptCoord = false;
    int defaultTapKey = 'F';

    std::function<void()> beforeMutate;
    std::function<void()> onMutated;
    std::function<void()> onSelectionChanged;
    std::function<void(double, double)> onPickedCoord;

    explicit KeymapCanvas(const QPixmap &pixmap, QWidget *parent = nullptr)
        : QWidget(parent), image(pixmap) {
        setMinimumSize(540, 340);
        setMouseTracking(true);
        setFocusPolicy(Qt::ClickFocus);
    }

    QRectF imageRect() const {
        QSizeF size = image.isNull() ? QSizeF(1600, 900) : QSizeF(image.size());
        size.scale(QSizeF(width(), height()), Qt::KeepAspectRatio);
        return QRectF((width() - size.width()) / 2.0, (height() - size.height()) / 2.0,
                      size.width(), size.height());
    }

    QPointF toScreen(const QRectF &r, double ux, double uy) const {
        return QPointF(r.left() + ux * r.width(), r.top() + uy * r.height());
    }

    QPointF toUnit(const QRectF &r, QPointF pos) const {
        double ux = std::clamp((pos.x() - r.left()) / std::max(1.0, r.width()), 0.0, 1.0);
        double uy = std::clamp((pos.y() - r.top()) / std::max(1.0, r.height()), 0.0, 1.0);
        if (snapToGrid) {
            ux = std::round(ux * 50.0) / 50.0; // 2% grid
            uy = std::round(uy * 50.0) / 50.0;
        }
        return QPointF(ux, uy);
    }

protected:
    void paintEvent(QPaintEvent *) override {
        QPainter p(this);
        p.setRenderHint(QPainter::Antialiasing, true);
        p.fillRect(rect(), QColor("#0b0e18"));
        const auto r = imageRect();

        if (!image.isNull()) {
            p.drawPixmap(r, image, QRectF(image.rect()));
        } else {
            p.fillRect(r, QColor("#141b2d"));
            p.setPen(QPen(QColor(255, 255, 255, 18), 1));
            for (int i = 1; i < 10; ++i) {
                p.drawLine(QPointF(r.left() + r.width() * i / 10.0, r.top()),
                           QPointF(r.left() + r.width() * i / 10.0, r.bottom()));
                p.drawLine(QPointF(r.left(), r.top() + r.height() * i / 10.0),
                           QPointF(r.right(), r.top() + r.height() * i / 10.0));
            }
            p.setPen(QColor("#8e9bb8"));
            p.drawText(r, Qt::AlignCenter,
                       "Reference 16:9 Canvas (Android offline)\nControls save as display percentages and scale automatically.");
        }

        if (snapToGrid) {
            p.setPen(QPen(QColor(157, 134, 255, 28), 1, Qt::DotLine));
            for (int i = 1; i < 20; ++i) {
                p.drawLine(QPointF(r.left() + r.width() * i / 20.0, r.top()),
                           QPointF(r.left() + r.width() * i / 20.0, r.bottom()));
                p.drawLine(QPointF(r.left(), r.top() + r.height() * i / 20.0),
                           QPointF(r.right(), r.top() + r.height() * i / 20.0));
            }
        }

        const qreal minDim = std::min(r.width(), r.height());

        // Draw sequential connectors for multi-marker same-key taps
        QHash<int, QList<QPointF>> keyChains;
        for (const auto &b : profile.taps) keyChains[b.key].append(toScreen(r, b.x, b.y));
        for (auto it = keyChains.cbegin(); it != keyChains.cend(); ++it) {
            if (it.value().size() > 1) {
                p.setPen(QPen(QColor(145, 118, 255, 170), 2.0, Qt::DashLine));
                for (int i = 0; i + 1 < it.value().size(); ++i)
                    p.drawLine(it.value()[i], it.value()[i + 1]);
            }
        }

        // 1. D-pads
        for (int i = 0; i < profile.dpads.size(); ++i) {
            const auto &d = profile.dpads[i];
            const bool isSel = (sel.type == SelType::Dpad && sel.index == i);
            const QPointF c = toScreen(r, d.x, d.y);
            const qreal rad = std::max<qreal>(34.0, d.radius * minDim);
            if (d.sprintKey != 0) {
                p.setPen(QPen(QColor(110, 231, 183, 95), 1.2, Qt::DotLine));
                p.setBrush(Qt::NoBrush);
                p.drawEllipse(c, rad * d.sprintMultiplier, rad * d.sprintMultiplier);
            }
            p.setPen(QPen(isSel ? QColor("#ffffff") : QColor("#9d86ff"), isSel ? 2.6 : 1.8));
            p.setBrush(QColor(35, 30, 68, 175));
            p.drawEllipse(c, rad, rad);
            p.setPen(Qt::white);
            p.drawText(QRectF(c.x() - 22, c.y() - rad + 2, 44, 18), Qt::AlignCenter, keyShortBadge(d.keyUp));
            p.drawText(QRectF(c.x() - 22, c.y() + rad - 20, 44, 18), Qt::AlignCenter, keyShortBadge(d.keyDown));
            p.drawText(QRectF(c.x() - rad + 2, c.y() - 9, 34, 18), Qt::AlignCenter, keyShortBadge(d.keyLeft));
            p.drawText(QRectF(c.x() + rad - 36, c.y() - 9, 34, 18), Qt::AlignCenter, keyShortBadge(d.keyRight));
            p.setPen(QColor("#c7beff"));
            p.drawText(QRectF(c.x() - 28, c.y() - 9, 56, 18), Qt::AlignCenter, QStringLiteral("✥"));
        }

        // 2. Swipes
        for (int i = 0; i < profile.swipes.size(); ++i) {
            const auto &sw = profile.swipes[i];
            const bool isSel = (sel.type == SelType::Swipe && sel.index == i);
            const QPointF p1 = toScreen(r, sw.x1, sw.y1);
            const QPointF p2 = toScreen(r, sw.x2, sw.y2);
            p.setPen(QPen(isSel ? QColor("#ffffff") : QColor("#57d9ff"), isSel ? 3.0 : 2.2));
            p.drawLine(p1, p2);
            p.setBrush(QColor(18, 82, 112, 220));
            p.drawEllipse(p1, 13, 13);
            p.setBrush(QColor(87, 217, 255, 230));
            p.drawEllipse(p2, 9, 9);
            p.setPen(Qt::white);
            p.drawText(QRectF(p1.x() - 18, p1.y() - 13, 36, 26), Qt::AlignCenter, keyShortBadge(sw.key));
        }

        // 3. Pinch Zooms
        for (int i = 0; i < profile.zooms.size(); ++i) {
            const auto &z = profile.zooms[i];
            const bool isSel = (sel.type == SelType::Zoom && sel.index == i);
            const QPointF c = toScreen(r, z.x, z.y);
            const qreal rad = std::max<qreal>(30.0, z.radius * minDim * 0.75);
            p.setPen(QPen(isSel ? QColor("#ffffff") : QColor("#ffd166"), isSel ? 2.6 : 1.8, Qt::DashLine));
            p.setBrush(QColor(92, 68, 16, 120));
            p.drawEllipse(c, rad, rad);
            p.setPen(Qt::white);
            p.drawText(QRectF(c.x() - 40, c.y() - 12, 80, 24), Qt::AlignCenter,
                       QStringLiteral("⤢ %1/%2").arg(keyShortBadge(z.keyIn), keyShortBadge(z.keyOut)));
        }

        // 4. MOBA Skills
        for (int i = 0; i < profile.mobaSkills.size(); ++i) {
            const auto &m = profile.mobaSkills[i];
            const bool isSel = (sel.type == SelType::Moba && sel.index == i);
            const QPointF c = toScreen(r, m.x, m.y);
            const qreal rad = std::max<qreal>(26.0, m.radius * minDim * 0.7);
            p.setPen(QPen(isSel ? QColor("#ffffff") : QColor("#ff7ab8"), isSel ? 2.6 : 1.8));
            p.setBrush(QColor(96, 28, 66, 165));
            p.drawEllipse(c, rad, rad);
            p.setPen(Qt::white);
            p.drawText(QRectF(c.x() - 24, c.y() - 12, 48, 24), Qt::AlignCenter,
                       QStringLiteral("⚡") + keyShortBadge(m.key));
            if (isSel) {
                const QPointF orig = toScreen(r, m.originX, m.originY);
                p.setPen(QPen(QColor("#ffadd2"), 1.4, Qt::DotLine));
                p.setBrush(QColor(255, 122, 184, 150));
                p.drawEllipse(orig, 10, 10);
                p.setPen(Qt::white);
                p.drawText(QRectF(orig.x() - 24, orig.y() - 24, 48, 16), Qt::AlignCenter, "Hero");
                if (m.cancelEnabled) {
                    const QPointF can = toScreen(r, m.cancelX, m.cancelY);
                    p.setPen(QPen(QColor("#ff7875"), 1.6));
                    p.setBrush(QColor(140, 28, 28, 190));
                    p.drawEllipse(can, 12, 12);
                    p.setPen(Qt::white);
                    p.drawText(QRectF(can.x() - 18, can.y() - 12, 36, 24), Qt::AlignCenter, "✕");
                }
            }
        }

        // 5. Macro Scripts
        for (int i = 0; i < profile.scripts.size(); ++i) {
            const auto &sc = profile.scripts[i];
            const bool isSel = (sel.type == SelType::Script && sel.index == i);
            const QPointF c = toScreen(r, sc.x, sc.y);
            p.setPen(QPen(isSel ? QColor("#ffffff") : QColor("#6ee7b7"), isSel ? 2.8 : 1.8));
            p.setBrush(QColor(18, 78, 62, 220));
            p.drawRoundedRect(QRectF(c.x() - 24, c.y() - 16, 48, 32), 9, 9);
            p.setPen(Qt::white);
            p.drawText(QRectF(c.x() - 24, c.y() - 16, 48, 32), Qt::AlignCenter,
                       QStringLiteral("⌘") + keyShortBadge(sc.key));
        }

        // 6. Tap / Burst / Repeat markers
        QHash<int, int> seqStepCounter;
        for (int i = 0; i < profile.taps.size(); ++i) {
            const auto &b = profile.taps[i];
            const int stepNum = ++seqStepCounter[b.key];
            const int totalForKey = keyChains.value(b.key).size();
            const bool isSel = (sel.type == SelType::Tap && sel.index == i);
            const QPointF pt = toScreen(r, b.x, b.y);
            p.setBrush(isSel ? QColor(139, 111, 255, 245) : QColor(43, 123, 255, 215));
            p.setPen(QPen(isSel ? QColor("#ffffff") : QColor("#d9e6ff"), isSel ? 3.0 : 1.8));
            p.drawEllipse(pt, 18, 18);
            QString text = keyShortBadge(b.key);
            if (totalForKey > 1) text += QStringLiteral("·%1").arg(stepNum);
            else if (b.repeat) text += QStringLiteral("↻");
            else if (b.burstCount > 1) text += QStringLiteral("×%1").arg(b.burstCount);
            p.setPen(Qt::white);
            p.drawText(QRectF(pt.x() - 22, pt.y() - 18, 44, 36), Qt::AlignCenter, text);
        }

        // 7. Aim / Fire / ADS / Free-Look markers
        if (profile.aim.enabled) {
            const auto &a = profile.aim;
            const bool isSel = (sel.type == SelType::Aim);
            const QPointF anc = toScreen(r, a.anchorX, a.anchorY);
            p.setPen(QPen(isSel ? QColor("#ffffff") : QColor("#7cf7c4"), isSel ? 2.8 : 2.0));
            p.setBrush(QColor(18, 72, 58, 210));
            p.drawEllipse(anc, 19, 19);
            p.setPen(Qt::white);
            p.drawText(QRectF(anc.x() - 22, anc.y() - 19, 44, 38), Qt::AlignCenter,
                       QStringLiteral("⌖") + keyShortBadge(a.toggleKey));
            if (a.fireEnabled) {
                const QPointF fp = toScreen(r, a.fireX, a.fireY);
                p.setPen(QPen(QColor("#ff7875"), 2.0));
                p.setBrush(QColor(130, 28, 34, 215));
                p.drawEllipse(fp, 15, 15);
                p.setPen(Qt::white);
                p.drawText(QRectF(fp.x() - 18, fp.y() - 15, 36, 30), Qt::AlignCenter, "LMB");
            }
            if (a.adsEnabled) {
                const QPointF ap = toScreen(r, a.adsX, a.adsY);
                p.setPen(QPen(QColor("#ffd166"), 2.0));
                p.setBrush(QColor(120, 84, 18, 215));
                p.drawEllipse(ap, 15, 15);
                p.setPen(Qt::white);
                p.drawText(QRectF(ap.x() - 18, ap.y() - 15, 36, 30), Qt::AlignCenter, "RMB");
            }
            if (a.freeLookEnabled) {
                const QPointF lp = toScreen(r, a.freeLookX, a.freeLookY);
                p.setPen(QPen(QColor("#69c0ff"), 2.0));
                p.setBrush(QColor(22, 68, 118, 215));
                p.drawEllipse(lp, 15, 15);
                p.setPen(Qt::white);
                p.drawText(QRectF(lp.x() - 18, lp.y() - 15, 36, 30), Qt::AlignCenter, "👁");
            }
        }
    }

    void mousePressEvent(QMouseEvent *event) override {
        if (event->button() != Qt::LeftButton) return;
        const auto r = imageRect();
        if (!r.contains(event->position())) return;

        if (pickingScriptCoord) {
            const QPointF u = toUnit(r, event->position());
            pickingScriptCoord = false;
            setCursor(Qt::ArrowCursor);
            if (onPickedCoord) onPickedCoord(u.x() * 100.0, u.y() * 100.0);
            return;
        }

        dragHandle = 0;
        if (hitTestExisting(r, event->position())) {
            if (beforeMutate) beforeMutate();
            dragging = true;
            update();
            if (onSelectionChanged) onSelectionChanged();
            return;
        }

        if (activeTool == ToolKind::Select) {
            sel = {SelType::None, -1};
            update();
            if (onSelectionChanged) onSelectionChanged();
            return;
        }

        const QPointF u = toUnit(r, event->position());
        if (beforeMutate) beforeMutate();

        switch (activeTool) {
        case ToolKind::Tap:
            if (profile.taps.size() < 128) {
                KeyBinding b;
                b.key = defaultTapKey;
                b.x = u.x(); b.y = u.y();
                for (const auto &existing : profile.taps) {
                    if (existing.key == b.key) {
                        b.repeat = existing.repeat;
                        b.intervalMs = existing.intervalMs;
                        b.burstCount = existing.burstCount;
                        break;
                    }
                }
                profile.taps.append(b);
                sel = {SelType::Tap, static_cast<int>(profile.taps.size()) - 1};
                dragging = true;
            }
            break;
        case ToolKind::Dpad:
            if (profile.dpads.size() < 8) {
                DpadControl d;
                d.x = u.x(); d.y = u.y();
                profile.dpads.append(d);
                sel = {SelType::Dpad, static_cast<int>(profile.dpads.size()) - 1};
                dragging = true;
            }
            break;
        case ToolKind::Swipe:
            if (profile.swipes.size() < 64) {
                SwipeControl sw;
                sw.key = 'E';
                sw.x1 = u.x(); sw.y1 = u.y();
                sw.x2 = std::clamp(u.x() + 0.12, 0.02, 0.98);
                sw.y2 = std::clamp(u.y() - 0.12, 0.02, 0.98);
                profile.swipes.append(sw);
                sel = {SelType::Swipe, static_cast<int>(profile.swipes.size()) - 1};
                dragHandle = 1; // drag end-point immediately for intuitive vector drawing!
                dragging = true;
            }
            break;
        case ToolKind::Zoom:
            if (profile.zooms.size() < 8) {
                ZoomControl z;
                z.x = u.x(); z.y = u.y();
                profile.zooms.append(z);
                sel = {SelType::Zoom, static_cast<int>(profile.zooms.size()) - 1};
                dragging = true;
            }
            break;
        case ToolKind::Moba:
            if (profile.mobaSkills.size() < 24) {
                MobaSkillControl m;
                m.x = u.x(); m.y = u.y();
                profile.mobaSkills.append(m);
                sel = {SelType::Moba, static_cast<int>(profile.mobaSkills.size()) - 1};
                dragging = true;
            }
            break;
        case ToolKind::Script:
            if (profile.scripts.size() < 32) {
                ScriptControl sc;
                sc.key = 'X';
                sc.x = u.x(); sc.y = u.y();
                sc.scriptText = QStringLiteral("tap %1 %2 50").arg(qRound(u.x() * 100.0)).arg(qRound(u.y() * 100.0));
                profile.scripts.append(sc);
                sel = {SelType::Script, static_cast<int>(profile.scripts.size()) - 1};
                dragging = true;
            }
            break;
        case ToolKind::Select:
            break;
        }
        update();
        if (onMutated) onMutated();
        if (onSelectionChanged) onSelectionChanged();
    }

    void mouseMoveEvent(QMouseEvent *event) override {
        if (!dragging || !(event->buttons() & Qt::LeftButton)) return;
        const auto r = imageRect();
        const QPointF u = toUnit(r, event->position());
        switch (sel.type) {
        case SelType::Tap:
            if (sel.index >= 0 && sel.index < profile.taps.size()) {
                profile.taps[sel.index].x = u.x();
                profile.taps[sel.index].y = u.y();
            }
            break;
        case SelType::Dpad:
            if (sel.index >= 0 && sel.index < profile.dpads.size()) {
                profile.dpads[sel.index].x = u.x();
                profile.dpads[sel.index].y = u.y();
            }
            break;
        case SelType::Swipe:
            if (sel.index >= 0 && sel.index < profile.swipes.size()) {
                if (dragHandle == 1) {
                    profile.swipes[sel.index].x2 = u.x();
                    profile.swipes[sel.index].y2 = u.y();
                } else {
                    profile.swipes[sel.index].x1 = u.x();
                    profile.swipes[sel.index].y1 = u.y();
                }
            }
            break;
        case SelType::Zoom:
            if (sel.index >= 0 && sel.index < profile.zooms.size()) {
                profile.zooms[sel.index].x = u.x();
                profile.zooms[sel.index].y = u.y();
            }
            break;
        case SelType::Moba:
            if (sel.index >= 0 && sel.index < profile.mobaSkills.size()) {
                auto &m = profile.mobaSkills[sel.index];
                if (dragHandle == 1) { m.originX = u.x(); m.originY = u.y(); }
                else if (dragHandle == 2) { m.cancelX = u.x(); m.cancelY = u.y(); }
                else { m.x = u.x(); m.y = u.y(); }
            }
            break;
        case SelType::Script:
            if (sel.index >= 0 && sel.index < profile.scripts.size()) {
                profile.scripts[sel.index].x = u.x();
                profile.scripts[sel.index].y = u.y();
            }
            break;
        case SelType::Aim: {
            auto &a = profile.aim;
            if (dragHandle == 1) { a.fireX = u.x(); a.fireY = u.y(); }
            else if (dragHandle == 2) { a.adsX = u.x(); a.adsY = u.y(); }
            else if (dragHandle == 3) {
                a.freeLookX = std::clamp(u.x(), 0.1, 0.9);
                a.freeLookY = std::clamp(u.y(), 0.1, 0.9);
            } else {
                a.anchorX = std::clamp(u.x(), 0.1, 0.9);
                a.anchorY = std::clamp(u.y(), 0.1, 0.9);
            }
            break;
        }
        case SelType::None:
            break;
        }
        update();
        if (onMutated) onMutated();
    }

    void mouseReleaseEvent(QMouseEvent *) override { dragging = false; }

private:
    bool hitTestExisting(const QRectF &r, QPointF pos) {
        if (profile.aim.enabled) {
            const auto &a = profile.aim;
            if (a.freeLookEnabled && QLineF(toScreen(r, a.freeLookX, a.freeLookY), pos).length() <= 18) {
                sel = {SelType::Aim, 0}; dragHandle = 3; return true;
            }
            if (a.adsEnabled && QLineF(toScreen(r, a.adsX, a.adsY), pos).length() <= 18) {
                sel = {SelType::Aim, 0}; dragHandle = 2; return true;
            }
            if (a.fireEnabled && QLineF(toScreen(r, a.fireX, a.fireY), pos).length() <= 18) {
                sel = {SelType::Aim, 0}; dragHandle = 1; return true;
            }
            if (QLineF(toScreen(r, a.anchorX, a.anchorY), pos).length() <= 21) {
                sel = {SelType::Aim, 0}; dragHandle = 0; return true;
            }
        }
        if (sel.type == SelType::Moba && sel.index >= 0 && sel.index < profile.mobaSkills.size()) {
            const auto &m = profile.mobaSkills[sel.index];
            if (m.cancelEnabled && QLineF(toScreen(r, m.cancelX, m.cancelY), pos).length() <= 16) {
                dragHandle = 2; return true;
            }
            if (QLineF(toScreen(r, m.originX, m.originY), pos).length() <= 15) {
                dragHandle = 1; return true;
            }
        }
        for (int i = profile.taps.size() - 1; i >= 0; --i) {
            if (QLineF(toScreen(r, profile.taps[i].x, profile.taps[i].y), pos).length() <= 20) {
                sel = {SelType::Tap, i}; return true;
            }
        }
        for (int i = profile.scripts.size() - 1; i >= 0; --i) {
            if (QLineF(toScreen(r, profile.scripts[i].x, profile.scripts[i].y), pos).length() <= 24) {
                sel = {SelType::Script, i}; return true;
            }
        }
        for (int i = profile.swipes.size() - 1; i >= 0; --i) {
            if (QLineF(toScreen(r, profile.swipes[i].x2, profile.swipes[i].y2), pos).length() <= 15) {
                sel = {SelType::Swipe, i}; dragHandle = 1; return true;
            }
            if (QLineF(toScreen(r, profile.swipes[i].x1, profile.swipes[i].y1), pos).length() <= 18) {
                sel = {SelType::Swipe, i}; dragHandle = 0; return true;
            }
        }
        for (int i = profile.mobaSkills.size() - 1; i >= 0; --i) {
            if (QLineF(toScreen(r, profile.mobaSkills[i].x, profile.mobaSkills[i].y), pos).length() <= 28) {
                sel = {SelType::Moba, i}; dragHandle = 0; return true;
            }
        }
        for (int i = profile.zooms.size() - 1; i >= 0; --i) {
            if (QLineF(toScreen(r, profile.zooms[i].x, profile.zooms[i].y), pos).length() <= 30) {
                sel = {SelType::Zoom, i}; return true;
            }
        }
        for (int i = profile.dpads.size() - 1; i >= 0; --i) {
            if (QLineF(toScreen(r, profile.dpads[i].x, profile.dpads[i].y), pos).length() <= 36) {
                sel = {SelType::Dpad, i}; return true;
            }
        }
        return false;
    }

    bool dragging = false;
    int dragHandle = 0;
};

KeymapDialog::KeymapDialog(const QPixmap &screenshot, const QList<KeyBinding> &bindings, QWidget *parent)
    : KeymapDialog(screenshot, [&] { KeymapProfile p; p.taps = bindings; return p; }(),
                   QString(), QStringLiteral("default"), parent) {}

KeymapDialog::KeymapDialog(const QPixmap &screenshot, const KeymapProfile &profile,
                           const QString &schemeDir, const QString &activeScheme, QWidget *parent)
    : QDialog(parent), schemesDirectory(schemeDir), currentSchemeName(sanitizeSchemeName(activeScheme)) {
    setWindowTitle("Nexus Keymap Studio");
    resize(1220, 760);
    setMinimumSize(980, 620);

    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(16, 14, 16, 14);
    root->setSpacing(10);

    // Top Studio Header Bar
    auto *topBar = new QHBoxLayout;
    topBar->setSpacing(8);
    auto *title = new QLabel("Keymap Studio", this);
    title->setObjectName("sectionTitle");
    topBar->addWidget(title);
    topBar->addSpacing(8);

    topBar->addWidget(new QLabel("Scheme:", this));
    schemeCombo = new QComboBox(this);
    schemeCombo->setMinimumWidth(155);
    const QStringList schemes = schemesDirectory.isEmpty() ? QStringList{currentSchemeName} : listSavedSchemes(schemesDirectory);
    schemeCombo->addItems(schemes);
    const int sIdx = schemeCombo->findText(currentSchemeName, Qt::MatchFixedString);
    if (sIdx >= 0) schemeCombo->setCurrentIndex(sIdx);
    topBar->addWidget(schemeCombo);

    auto *newSchemeBtn = new QPushButton("+ New", this);
    topBar->addWidget(newSchemeBtn);

    auto *presetBtn = new QPushButton("Templates ▾", this);
    auto *presetMenu = new QMenu(presetBtn);
    presetMenu->addAction("Battle Royale / FPS (WASD + Aim + ADS + Free-Look)", this, [this] { applyPreset("fps"); });
    presetMenu->addAction("MOBA 5v5 Arena (WASD + 3 Aimed Skills + Cancel)", this, [this] { applyPreset("moba"); });
    presetMenu->addAction("Action RPG Combat (WASD + Dodge Swipe + Zoom + Combo)", this, [this] { applyPreset("arpg"); });
    presetMenu->addSeparator();
    presetMenu->addAction("Clear All Controls", this, [this] { applyPreset("blank"); });
    presetBtn->setMenu(presetMenu);
    topBar->addWidget(presetBtn);

    auto *importBtn = new QPushButton("Import…", this);
    auto *exportBtn = new QPushButton("Export…", this);
    topBar->addWidget(importBtn);
    topBar->addWidget(exportBtn);

    topBar->addStretch();

    undoBtn = new QPushButton("↶ Undo", this);
    redoBtn = new QPushButton("↷ Redo", this);
    undoBtn->setEnabled(false);
    redoBtn->setEnabled(false);
    topBar->addWidget(undoBtn);
    topBar->addWidget(redoBtn);

    snapCheck = new QCheckBox("Snap grid", this);
    topBar->addWidget(snapCheck);

    topBar->addWidget(new QLabel("Overlay:", this));
    opacitySlider = new QSlider(Qt::Horizontal, this);
    opacitySlider->setRange(0, 100);
    opacitySlider->setFixedWidth(90);
    opacitySlider->setValue(profile.overlayOpacity);
    opacityValueLabel = new QLabel(QStringLiteral("%1%").arg(profile.overlayOpacity), this);
    opacityValueLabel->setFixedWidth(36);
    topBar->addWidget(opacitySlider);
    topBar->addWidget(opacityValueLabel);
    root->addLayout(topBar);

    // Tool Palette Row
    auto *paletteRow = new QHBoxLayout;
    paletteRow->setSpacing(6);
    auto *toolGroup = new QButtonGroup(this);
    toolGroup->setExclusive(true);
    auto makeTool = [&](const QString &label, ToolKind kind, bool checked = false) {
        auto *b = new QPushButton(label, this);
        b->setCheckable(true);
        b->setChecked(checked);
        toolGroup->addButton(b, static_cast<int>(kind));
        paletteRow->addWidget(b);
        return b;
    };
    makeTool("↖ Select / Drag", ToolKind::Select);
    makeTool("● Tap / Burst / Repeat", ToolKind::Tap, true);
    makeTool("✥ WASD D-Pad", ToolKind::Dpad);
    makeTool("↝ Swipe", ToolKind::Swipe);
    makeTool("⤢ Pinch Zoom", ToolKind::Zoom);
    makeTool("⚡ MOBA Skill", ToolKind::Moba);
    makeTool("⌘ Macro Script", ToolKind::Script);
    paletteRow->addStretch();
    auto *aimDialogBtn = new QPushButton("⌖ Aim, Shoot & ADS…", this);
    paletteRow->addWidget(aimDialogBtn);
    root->addLayout(paletteRow);

    // Body: Canvas + Right Sidebar (Placed List + Context Inspector)
    auto *body = new QHBoxLayout;
    body->setSpacing(12);

    canvas = new KeymapCanvas(screenshot, this);
    canvas->profile = profile;
    body->addWidget(canvas, 1);

    auto *sidePanel = new QWidget(this);
    sidePanel->setFixedWidth(315);
    auto *side = new QVBoxLayout(sidePanel);
    side->setContentsMargins(0, 0, 0, 0);
    side->setSpacing(8);

    side->addWidget(new QLabel("Placed Controls", sidePanel));
    controlList = new QListWidget(sidePanel);
    controlList->setMaximumHeight(165);
    side->addWidget(controlList);

    auto *removeBtn = new QPushButton("Remove Selected Control", sidePanel);
    side->addWidget(removeBtn);

    inspectorStack = new QStackedWidget(sidePanel);

    // Page 0: Empty / Help
    {
        auto *w = new QWidget;
        auto *l = new QVBoxLayout(w);
        auto *hint = new QLabel(
            "Select a control tool above and click the canvas to place it, or click any existing marker to inspect and rebind.\n\n"
            "• Click any Key button to capture keyboard keys, F-keys, arrows, Shift/Ctrl/Alt, Right/Middle/Side mouse buttons, or Scroll Wheel.\n"
            "• Same-key Tap markers run in sequence; Burst fires N rapid taps on one press.\n"
            "• Pinch Zoom & concurrent multi-key touch require Multi-Touch enabled in Settings → Preferences.",
            w);
        hint->setWordWrap(true);
        hint->setObjectName("muted");
        l->addWidget(hint);
        l->addStretch();
        inspectorStack->addWidget(w);
    }

    // Page 1: Tap / Burst / Repeat Inspector
    {
        auto *w = new QWidget;
        auto *f = new QFormLayout(w);
        tapKeyBtn = new KeyBindButton(false, w);
        tapModeCombo = new QComboBox(w);
        tapModeCombo->addItem("Hold / Sequential combo", 0);
        tapModeCombo->addItem("Repeat while held (Turbo)", 1);
        tapModeCombo->addItem("Burst N taps on single press", 2);
        tapIntervalSpin = new QSpinBox(w);
        tapIntervalSpin->setRange(25, 5000);
        tapIntervalSpin->setSingleStep(25);
        tapIntervalSpin->setSuffix(" ms");
        tapBurstSpin = new QSpinBox(w);
        tapBurstSpin->setRange(2, 50);
        tapBurstSpin->setSuffix(" taps");
        tapXSpin = new QDoubleSpinBox(w); tapXSpin->setRange(0, 100); tapXSpin->setDecimals(1); tapXSpin->setSuffix(" %");
        tapYSpin = new QDoubleSpinBox(w); tapYSpin->setRange(0, 100); tapYSpin->setDecimals(1); tapYSpin->setSuffix(" %");
        tapLabelEdit = new QLineEdit(w); tapLabelEdit->setPlaceholderText("e.g. Jump, Reload");
        f->addRow("Bound input", tapKeyBtn);
        f->addRow("Behavior", tapModeCombo);
        f->addRow("Interval", tapIntervalSpin);
        f->addRow("Burst count", tapBurstSpin);
        f->addRow("Position X", tapXSpin);
        f->addRow("Position Y", tapYSpin);
        f->addRow("Label", tapLabelEdit);
        inspectorStack->addWidget(w);
    }

    // Page 2: D-Pad Inspector
    {
        auto *w = new QWidget;
        auto *f = new QFormLayout(w);
        dpadUpBtn = new KeyBindButton(false, w);
        dpadDownBtn = new KeyBindButton(false, w);
        dpadLeftBtn = new KeyBindButton(false, w);
        dpadRightBtn = new KeyBindButton(false, w);
        dpadSprintBtn = new KeyBindButton(true, w);
        dpadWalkBtn = new KeyBindButton(true, w);
        dpadRadiusSpin = new QDoubleSpinBox(w); dpadRadiusSpin->setRange(3.0, 40.0); dpadRadiusSpin->setSuffix(" %");
        dpadSprintMultSpin = new QDoubleSpinBox(w); dpadSprintMultSpin->setRange(1.05, 2.40); dpadSprintMultSpin->setSingleStep(0.05); dpadSprintMultSpin->setSuffix(" ×");
        f->addRow("Move Up", dpadUpBtn);
        f->addRow("Move Down", dpadDownBtn);
        f->addRow("Move Left", dpadLeftBtn);
        f->addRow("Move Right", dpadRightBtn);
        f->addRow("Sprint key", dpadSprintBtn);
        f->addRow("Sprint reach", dpadSprintMultSpin);
        f->addRow("Walk key", dpadWalkBtn);
        f->addRow("Stick radius", dpadRadiusSpin);
        inspectorStack->addWidget(w);
    }

    // Page 3: Swipe Inspector
    {
        auto *w = new QWidget;
        auto *f = new QFormLayout(w);
        swipeKeyBtn = new KeyBindButton(false, w);
        swipeDurationSpin = new QSpinBox(w); swipeDurationSpin->setRange(30, 5000); swipeDurationSpin->setSingleStep(20); swipeDurationSpin->setSuffix(" ms");
        swipeHoldCheck = new QCheckBox("Hold finger at end until key release", w);
        swipeLabelEdit = new QLineEdit(w); swipeLabelEdit->setPlaceholderText("e.g. Dodge, Peek");
        f->addRow("Trigger input", swipeKeyBtn);
        f->addRow("Swipe duration", swipeDurationSpin);
        f->addRow(swipeHoldCheck);
        f->addRow("Label", swipeLabelEdit);
        auto *note = new QLabel("Drag both start (dark) and end (cyan) handles on the canvas.", w);
        note->setWordWrap(true); note->setObjectName("muted");
        f->addRow(note);
        inspectorStack->addWidget(w);
    }

    // Page 4: Pinch Zoom Inspector
    {
        auto *w = new QWidget;
        auto *f = new QFormLayout(w);
        zoomInBtn = new KeyBindButton(false, w);
        zoomOutBtn = new KeyBindButton(false, w);
        zoomModBtn = new KeyBindButton(true, w);
        zoomRadiusSpin = new QDoubleSpinBox(w); zoomRadiusSpin->setRange(4.0, 40.0); zoomRadiusSpin->setSuffix(" %");
        zoomDurationSpin = new QSpinBox(w); zoomDurationSpin->setRange(40, 2000); zoomDurationSpin->setSuffix(" ms");
        f->addRow("Zoom In", zoomInBtn);
        f->addRow("Zoom Out", zoomOutBtn);
        f->addRow("Modifier", zoomModBtn);
        f->addRow("Spread radius", zoomRadiusSpin);
        f->addRow("Duration", zoomDurationSpin);
        inspectorStack->addWidget(w);
    }

    // Page 5: MOBA Skill Inspector
    {
        auto *w = new QWidget;
        auto *f = new QFormLayout(w);
        mobaKeyBtn = new KeyBindButton(false, w);
        mobaModeCombo = new QComboBox(w);
        mobaModeCombo->addItem("Quick Cast toward cursor", static_cast<int>(MobaCastMode::QuickCast));
        mobaModeCombo->addItem("Manual Aim while held", static_cast<int>(MobaCastMode::ManualAimWhileHeld));
        mobaModeCombo->addItem("Auto-Cast center tap", static_cast<int>(MobaCastMode::AutoCastTap));
        mobaRadiusSpin = new QDoubleSpinBox(w); mobaRadiusSpin->setRange(3.0, 32.0); mobaRadiusSpin->setSuffix(" %");
        mobaCancelCheck = new QCheckBox("Enable Cancel-Skill target", w);
        mobaCancelKeyBtn = new KeyBindButton(false, w);
        mobaLabelEdit = new QLineEdit(w); mobaLabelEdit->setPlaceholderText("e.g. Skill 1, Ultimate");
        f->addRow("Skill key", mobaKeyBtn);
        f->addRow("Cast mode", mobaModeCombo);
        f->addRow("Cast radius", mobaRadiusSpin);
        f->addRow(mobaCancelCheck);
        f->addRow("Cancel key", mobaCancelKeyBtn);
        f->addRow("Label", mobaLabelEdit);
        auto *note = new QLabel("When selected, drag 'Hero' to your character center and '✕' to the game's cancel-cast icon.", w);
        note->setWordWrap(true); note->setObjectName("muted");
        f->addRow(note);
        inspectorStack->addWidget(w);
    }

    // Page 6: Macro Script Inspector
    {
        auto *w = new QWidget;
        auto *v = new QVBoxLayout(w);
        v->setContentsMargins(0, 0, 0, 0);
        auto *f = new QFormLayout;
        scriptKeyBtn = new KeyBindButton(false, w);
        scriptLabelEdit = new QLineEdit(w);
        scriptRepeatCheck = new QCheckBox("Loop script while key held", w);
        f->addRow("Trigger key", scriptKeyBtn);
        f->addRow("Macro name", scriptLabelEdit);
        f->addRow(scriptRepeatCheck);
        v->addLayout(f);
        pickCoordBtn = new QPushButton("＋ Click Canvas to Insert 'tap X Y'", w);
        v->addWidget(pickCoordBtn);
        scriptEditor = new QPlainTextEdit(w);
        scriptEditor->setPlaceholderText("tap 80 75 50\nwait 80\nswipe 50 70 50 30 160\nmultipress 20 80 85 75 60");
        v->addWidget(scriptEditor, 1);
        scriptStatusLabel = new QLabel(w);
        scriptStatusLabel->setWordWrap(true);
        scriptStatusLabel->setObjectName("muted");
        v->addWidget(scriptStatusLabel);
        inspectorStack->addWidget(w);
    }

    // Page 7: Aim Quick Summary
    {
        auto *w = new QWidget;
        auto *v = new QVBoxLayout(w);
        auto *lbl = new QLabel("Drag ⌖ (Camera Anchor), LMB (Fire), RMB (ADS), and 👁 (Free-Look) directly on the canvas, or open full Aim settings.", w);
        lbl->setWordWrap(true);
        v->addWidget(lbl);
        auto *openBtn = new QPushButton("Open Full Aim, ADS & Crosshair Settings…", w);
        connect(openBtn, &QPushButton::clicked, aimDialogBtn, &QPushButton::click);
        v->addWidget(openBtn);
        v->addStretch();
        inspectorStack->addWidget(w);
    }

    side->addWidget(inspectorStack, 1);
    body->addWidget(sidePanel);
    root->addLayout(body, 1);

    conflictBanner = new QLabel(this);
    conflictBanner->setWordWrap(true);
    root->addWidget(conflictBanner);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Save | QDialogButtonBox::Cancel, this);
    root->addWidget(buttons);

    // Wire Shortcuts & Actions
    new QShortcut(QKeySequence::Undo, this, [this] { undo(); });
    new QShortcut(QKeySequence::Redo, this, [this] { redo(); });

    connect(undoBtn, &QPushButton::clicked, this, [this] { undo(); });
    connect(redoBtn, &QPushButton::clicked, this, [this] { redo(); });
    connect(snapCheck, &QCheckBox::toggled, this, [this](bool on) { canvas->snapToGrid = on; canvas->update(); });
    connect(opacitySlider, &QSlider::valueChanged, this, [this](int val) {
        canvas->profile.overlayOpacity = val;
        opacityValueLabel->setText(QStringLiteral("%1%").arg(val));
    });
    connect(toolGroup, &QButtonGroup::idClicked, this, [this](int id) {
        canvas->activeTool = static_cast<ToolKind>(id);
    });
    connect(aimDialogBtn, &QPushButton::clicked, this, [this] {
        AimConfig init = canvas->profile.aim;
        init.enabled = true;
        AimSettingsDialog dlg(init, this);
        if (dlg.exec() == QDialog::Accepted) {
            pushUndoSnapshot();
            canvas->profile.aim = dlg.resultConfig();
            if (canvas->profile.aim.enabled) canvas->sel = {SelType::Aim, 0};
            canvas->update();
            refreshControlList();
            refreshInspector();
            refreshConflictBanner();
        }
    });

    connect(newSchemeBtn, &QPushButton::clicked, this, [this] { createNewScheme(); });
    connect(importBtn, &QPushButton::clicked, this, [this] { importSchemeFile(); });
    connect(exportBtn, &QPushButton::clicked, this, [this] { exportSchemeFile(); });
    connect(schemeCombo, &QComboBox::currentTextChanged, this, [this](const QString &name) {
        if (name.isEmpty() || schemesDirectory.isEmpty()) return;
        const QString safe = sanitizeSchemeName(name);
        if (safe == currentSchemeName) return;
        KeymapProfile loaded;
        QString err;
        if (loadKeymapProfile(schemeFilePath(schemesDirectory, safe), &loaded, &err)) {
            currentSchemeName = safe;
            undoStack.clear(); redoStack.clear();
            undoBtn->setEnabled(false); redoBtn->setEnabled(false);
            canvas->profile = loaded;
            canvas->sel = {SelType::None, -1};
            opacitySlider->setValue(loaded.overlayOpacity);
            canvas->update();
            refreshControlList();
            refreshInspector();
            refreshConflictBanner();
        }
    });

    canvas->beforeMutate = [this] { pushUndoSnapshot(); };
    canvas->onMutated = [this] {
        refreshControlList();
        refreshInspector();
        refreshConflictBanner();
    };
    canvas->onSelectionChanged = [this] {
        refreshControlList();
        refreshInspector();
    };
    canvas->onPickedCoord = [this](double px, double py) {
        pickCoordBtn->setText("＋ Click Canvas to Insert 'tap X Y'");
        if (canvas->sel.type == SelType::Script && canvas->sel.index >= 0 &&
            canvas->sel.index < canvas->profile.scripts.size()) {
            const QString line = QStringLiteral("tap %1 %2 50").arg(qRound(px)).arg(qRound(py));
            QString cur = scriptEditor->toPlainText();
            if (!cur.isEmpty() && !cur.endsWith('\n')) cur += '\n';
            cur += line;
            scriptEditor->setPlainText(cur);
        }
    };

    connect(controlList, &QListWidget::currentRowChanged, this, [this](int row) {
        if (row < 0) return;
        const auto data = controlList->item(row)->data(Qt::UserRole).toPoint();
        canvas->sel = {static_cast<SelType>(data.x()), data.y()};
        canvas->update();
        refreshInspector();
    });

    connect(removeBtn, &QPushButton::clicked, this, [this] {
        const auto s = canvas->sel;
        if (s.type == SelType::None) return;
        pushUndoSnapshot();
        switch (s.type) {
        case SelType::Tap: if (s.index >= 0 && s.index < canvas->profile.taps.size()) canvas->profile.taps.removeAt(s.index); break;
        case SelType::Dpad: if (s.index >= 0 && s.index < canvas->profile.dpads.size()) canvas->profile.dpads.removeAt(s.index); break;
        case SelType::Swipe: if (s.index >= 0 && s.index < canvas->profile.swipes.size()) canvas->profile.swipes.removeAt(s.index); break;
        case SelType::Zoom: if (s.index >= 0 && s.index < canvas->profile.zooms.size()) canvas->profile.zooms.removeAt(s.index); break;
        case SelType::Moba: if (s.index >= 0 && s.index < canvas->profile.mobaSkills.size()) canvas->profile.mobaSkills.removeAt(s.index); break;
        case SelType::Script: if (s.index >= 0 && s.index < canvas->profile.scripts.size()) canvas->profile.scripts.removeAt(s.index); break;
        case SelType::Aim: canvas->profile.aim.enabled = false; break;
        case SelType::None: break;
        }
        canvas->sel = {SelType::None, -1};
        canvas->update();
        refreshControlList();
        refreshInspector();
        refreshConflictBanner();
    });

    // Wire Inspector Edits
    auto updateTapMode = [this] {
        if (canvas->sel.type != SelType::Tap || canvas->sel.index < 0 || canvas->sel.index >= canvas->profile.taps.size()) return;
        pushUndoSnapshot();
        auto &cur = canvas->profile.taps[canvas->sel.index];
        const int mode = tapModeCombo->currentData().toInt();
        const bool rep = (mode == 1);
        const int burst = (mode == 2) ? std::max(2, tapBurstSpin->value()) : 1;
        const int iv = tapIntervalSpin->value();
        for (auto &b : canvas->profile.taps) {
            if (b.key == cur.key) { b.repeat = rep; b.burstCount = burst; b.intervalMs = iv; }
        }
        canvas->update(); refreshControlList(); refreshInspector(); refreshConflictBanner();
    };
    tapKeyBtn->keyChanged = [this](int k) {
        canvas->defaultTapKey = k;
        if (canvas->sel.type != SelType::Tap || canvas->sel.index < 0 || canvas->sel.index >= canvas->profile.taps.size()) return;
        pushUndoSnapshot();
        auto &cur = canvas->profile.taps[canvas->sel.index];
        cur.key = k;
        for (const auto &other : canvas->profile.taps) {
            if (&other != &cur && other.key == k) {
                cur.repeat = other.repeat; cur.burstCount = other.burstCount; cur.intervalMs = other.intervalMs;
                break;
            }
        }
        canvas->update(); refreshControlList(); refreshInspector(); refreshConflictBanner();
    };
    connect(tapModeCombo, &QComboBox::currentIndexChanged, this, [updateTapMode](int) { updateTapMode(); });
    connect(tapIntervalSpin, &QSpinBox::valueChanged, this, [updateTapMode](int) { updateTapMode(); });
    connect(tapBurstSpin, &QSpinBox::valueChanged, this, [updateTapMode](int) { updateTapMode(); });
    connect(tapXSpin, &QDoubleSpinBox::valueChanged, this, [this](double v) {
        if (canvas->sel.type == SelType::Tap && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.taps.size()) {
            canvas->profile.taps[canvas->sel.index].x = v / 100.0; canvas->update(); refreshControlList();
        }
    });
    connect(tapYSpin, &QDoubleSpinBox::valueChanged, this, [this](double v) {
        if (canvas->sel.type == SelType::Tap && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.taps.size()) {
            canvas->profile.taps[canvas->sel.index].y = v / 100.0; canvas->update(); refreshControlList();
        }
    });
    connect(tapLabelEdit, &QLineEdit::textEdited, this, [this](const QString &t) {
        if (canvas->sel.type == SelType::Tap && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.taps.size()) {
            canvas->profile.taps[canvas->sel.index].label = t; refreshControlList(); refreshConflictBanner();
        }
    });

    auto mutDpad = [this](std::function<void(DpadControl &)> fn) {
        if (canvas->sel.type != SelType::Dpad || canvas->sel.index < 0 || canvas->sel.index >= canvas->profile.dpads.size()) return;
        pushUndoSnapshot(); fn(canvas->profile.dpads[canvas->sel.index]);
        canvas->update(); refreshControlList(); refreshConflictBanner();
    };
    dpadUpBtn->keyChanged = [mutDpad](int k) { mutDpad([k](DpadControl &d) { d.keyUp = k; }); };
    dpadDownBtn->keyChanged = [mutDpad](int k) { mutDpad([k](DpadControl &d) { d.keyDown = k; }); };
    dpadLeftBtn->keyChanged = [mutDpad](int k) { mutDpad([k](DpadControl &d) { d.keyLeft = k; }); };
    dpadRightBtn->keyChanged = [mutDpad](int k) { mutDpad([k](DpadControl &d) { d.keyRight = k; }); };
    dpadSprintBtn->keyChanged = [mutDpad](int k) { mutDpad([k](DpadControl &d) { d.sprintKey = k; }); };
    dpadWalkBtn->keyChanged = [mutDpad](int k) { mutDpad([k](DpadControl &d) { d.walkKey = k; }); };
    connect(dpadRadiusSpin, &QDoubleSpinBox::valueChanged, this, [this](double v) {
        if (canvas->sel.type == SelType::Dpad && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.dpads.size()) {
            canvas->profile.dpads[canvas->sel.index].radius = v / 100.0; canvas->update();
        }
    });
    connect(dpadSprintMultSpin, &QDoubleSpinBox::valueChanged, this, [this](double v) {
        if (canvas->sel.type == SelType::Dpad && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.dpads.size()) {
            canvas->profile.dpads[canvas->sel.index].sprintMultiplier = v; canvas->update();
        }
    });

    swipeKeyBtn->keyChanged = [this](int k) {
        if (canvas->sel.type == SelType::Swipe && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.swipes.size()) {
            pushUndoSnapshot(); canvas->profile.swipes[canvas->sel.index].key = k;
            canvas->update(); refreshControlList(); refreshConflictBanner();
        }
    };
    connect(swipeDurationSpin, &QSpinBox::valueChanged, this, [this](int v) {
        if (canvas->sel.type == SelType::Swipe && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.swipes.size())
            canvas->profile.swipes[canvas->sel.index].durationMs = v;
    });
    connect(swipeHoldCheck, &QCheckBox::toggled, this, [this](bool on) {
        if (canvas->sel.type == SelType::Swipe && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.swipes.size())
            canvas->profile.swipes[canvas->sel.index].holdAtEnd = on;
    });
    connect(swipeLabelEdit, &QLineEdit::textEdited, this, [this](const QString &t) {
        if (canvas->sel.type == SelType::Swipe && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.swipes.size()) {
            canvas->profile.swipes[canvas->sel.index].label = t; refreshControlList();
        }
    });

    zoomInBtn->keyChanged = [this](int k) {
        if (canvas->sel.type == SelType::Zoom && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.zooms.size()) {
            pushUndoSnapshot(); canvas->profile.zooms[canvas->sel.index].keyIn = k;
            canvas->update(); refreshControlList(); refreshConflictBanner();
        }
    };
    zoomOutBtn->keyChanged = [this](int k) {
        if (canvas->sel.type == SelType::Zoom && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.zooms.size()) {
            pushUndoSnapshot(); canvas->profile.zooms[canvas->sel.index].keyOut = k;
            canvas->update(); refreshControlList(); refreshConflictBanner();
        }
    };
    zoomModBtn->keyChanged = [this](int k) {
        if (canvas->sel.type == SelType::Zoom && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.zooms.size()) {
            pushUndoSnapshot(); canvas->profile.zooms[canvas->sel.index].modifierKey = k;
            canvas->update(); refreshControlList(); refreshConflictBanner();
        }
    };
    connect(zoomRadiusSpin, &QDoubleSpinBox::valueChanged, this, [this](double v) {
        if (canvas->sel.type == SelType::Zoom && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.zooms.size()) {
            canvas->profile.zooms[canvas->sel.index].radius = v / 100.0; canvas->update();
        }
    });
    connect(zoomDurationSpin, &QSpinBox::valueChanged, this, [this](int v) {
        if (canvas->sel.type == SelType::Zoom && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.zooms.size())
            canvas->profile.zooms[canvas->sel.index].durationMs = v;
    });

    mobaKeyBtn->keyChanged = [this](int k) {
        if (canvas->sel.type == SelType::Moba && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.mobaSkills.size()) {
            pushUndoSnapshot(); canvas->profile.mobaSkills[canvas->sel.index].key = k;
            canvas->update(); refreshControlList(); refreshConflictBanner();
        }
    };
    mobaCancelKeyBtn->keyChanged = [this](int k) {
        if (canvas->sel.type == SelType::Moba && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.mobaSkills.size()) {
            pushUndoSnapshot(); canvas->profile.mobaSkills[canvas->sel.index].cancelKey = k;
            canvas->update();
        }
    };
    connect(mobaModeCombo, &QComboBox::currentIndexChanged, this, [this](int) {
        if (canvas->sel.type == SelType::Moba && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.mobaSkills.size())
            canvas->profile.mobaSkills[canvas->sel.index].mode = static_cast<MobaCastMode>(mobaModeCombo->currentData().toInt());
    });
    connect(mobaRadiusSpin, &QDoubleSpinBox::valueChanged, this, [this](double v) {
        if (canvas->sel.type == SelType::Moba && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.mobaSkills.size()) {
            canvas->profile.mobaSkills[canvas->sel.index].radius = v / 100.0; canvas->update();
        }
    });
    connect(mobaCancelCheck, &QCheckBox::toggled, this, [this](bool on) {
        if (canvas->sel.type == SelType::Moba && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.mobaSkills.size()) {
            canvas->profile.mobaSkills[canvas->sel.index].cancelEnabled = on;
            mobaCancelKeyBtn->setEnabled(on);
            canvas->update();
        }
    });
    connect(mobaLabelEdit, &QLineEdit::textEdited, this, [this](const QString &t) {
        if (canvas->sel.type == SelType::Moba && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.mobaSkills.size()) {
            canvas->profile.mobaSkills[canvas->sel.index].label = t; refreshControlList();
        }
    });

    scriptKeyBtn->keyChanged = [this](int k) {
        if (canvas->sel.type == SelType::Script && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.scripts.size()) {
            pushUndoSnapshot(); canvas->profile.scripts[canvas->sel.index].key = k;
            canvas->update(); refreshControlList(); refreshConflictBanner();
        }
    };
    connect(scriptLabelEdit, &QLineEdit::textEdited, this, [this](const QString &t) {
        if (canvas->sel.type == SelType::Script && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.scripts.size()) {
            canvas->profile.scripts[canvas->sel.index].label = t; refreshControlList();
        }
    });
    connect(scriptRepeatCheck, &QCheckBox::toggled, this, [this](bool on) {
        if (canvas->sel.type == SelType::Script && canvas->sel.index >= 0 && canvas->sel.index < canvas->profile.scripts.size())
            canvas->profile.scripts[canvas->sel.index].repeatWhileHeld = on;
    });
    connect(pickCoordBtn, &QPushButton::clicked, this, [this] {
        canvas->pickingScriptCoord = !canvas->pickingScriptCoord;
        canvas->setCursor(canvas->pickingScriptCoord ? Qt::CrossCursor : Qt::ArrowCursor);
        pickCoordBtn->setText(canvas->pickingScriptCoord ? "Click anywhere on canvas now…" : "＋ Click Canvas to Insert 'tap X Y'");
    });
    connect(scriptEditor, &QPlainTextEdit::textChanged, this, [this] {
        if (canvas->sel.type != SelType::Script || canvas->sel.index < 0 || canvas->sel.index >= canvas->profile.scripts.size()) return;
        const QString txt = scriptEditor->toPlainText();
        canvas->profile.scripts[canvas->sel.index].scriptText = txt;
        QList<ScriptStep> steps; QString err;
        if (parseScriptText(txt, &steps, &err))
            scriptStatusLabel->setText(QStringLiteral("✓ Valid macro (%1 steps).").arg(steps.size()));
        else
            scriptStatusLabel->setText(QStringLiteral("⚠ %1").arg(err));
    });

    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] {
        QString err;
        if (!validateKeymapProfile(canvas->profile, &err)) {
            QMessageBox::warning(this, "Invalid Control Configuration", err);
            return;
        }
        accept();
    });

    refreshControlList();
    refreshInspector();
    refreshConflictBanner();
}

void KeymapDialog::pushUndoSnapshot() {
    undoStack.append(canvas->profile);
    if (undoStack.size() > 40) undoStack.removeFirst();
    redoStack.clear();
    undoBtn->setEnabled(!undoStack.isEmpty());
    redoBtn->setEnabled(false);
}

void KeymapDialog::undo() {
    if (undoStack.isEmpty()) return;
    redoStack.append(canvas->profile);
    canvas->profile = undoStack.takeLast();
    canvas->sel = {SelType::None, -1};
    undoBtn->setEnabled(!undoStack.isEmpty());
    redoBtn->setEnabled(!redoStack.isEmpty());
    opacitySlider->setValue(canvas->profile.overlayOpacity);
    canvas->update();
    refreshControlList();
    refreshInspector();
    refreshConflictBanner();
}

void KeymapDialog::redo() {
    if (redoStack.isEmpty()) return;
    undoStack.append(canvas->profile);
    canvas->profile = redoStack.takeLast();
    canvas->sel = {SelType::None, -1};
    undoBtn->setEnabled(!undoStack.isEmpty());
    redoBtn->setEnabled(!redoStack.isEmpty());
    opacitySlider->setValue(canvas->profile.overlayOpacity);
    canvas->update();
    refreshControlList();
    refreshInspector();
    refreshConflictBanner();
}

void KeymapDialog::applyPreset(const QString &presetId) {
    pushUndoSnapshot();
    const QString keepName = canvas->profile.name;
    canvas->profile = makePresetProfile(presetId);
    if (presetId == QStringLiteral("blank")) canvas->profile.name = keepName;
    canvas->sel = {SelType::None, -1};
    opacitySlider->setValue(canvas->profile.overlayOpacity);
    canvas->update();
    refreshControlList();
    refreshInspector();
    refreshConflictBanner();
}

void KeymapDialog::createNewScheme() {
    bool ok = false;
    const QString raw = QInputDialog::getText(this, "New Control Scheme",
                                              "Scheme name:", QLineEdit::Normal,
                                              "My Game Controls", &ok);
    if (!ok || raw.trimmed().isEmpty()) return;
    const QString safe = sanitizeSchemeName(raw);
    currentSchemeName = safe;
    canvas->profile.name = raw.trimmed();
    if (!schemesDirectory.isEmpty()) {
        QString err;
        saveKeymapProfile(schemeFilePath(schemesDirectory, safe), canvas->profile, &err);
    }
    QSignalBlocker block(schemeCombo);
    if (schemeCombo->findText(safe, Qt::MatchFixedString) < 0) schemeCombo->addItem(safe);
    schemeCombo->setCurrentText(safe);
}

void KeymapDialog::importSchemeFile() {
    const QString path = QFileDialog::getOpenFileName(this, "Import Keymap Scheme", QString(), "Keymap JSON (*.json)");
    if (path.isEmpty()) return;
    KeymapProfile imported;
    QString err;
    if (!loadKeymapProfile(path, &imported, &err)) {
        QMessageBox::warning(this, "Import Failed", err);
        return;
    }
    pushUndoSnapshot();
    canvas->profile = imported;
    canvas->sel = {SelType::None, -1};
    opacitySlider->setValue(imported.overlayOpacity);
    canvas->update();
    refreshControlList();
    refreshInspector();
    refreshConflictBanner();
}

void KeymapDialog::exportSchemeFile() {
    QString err;
    if (!validateKeymapProfile(canvas->profile, &err)) {
        QMessageBox::warning(this, "Cannot Export", err);
        return;
    }
    const QString path = QFileDialog::getSaveFileName(this, "Export Keymap Scheme",
                                                      currentSchemeName + ".json", "Keymap JSON (*.json)");
    if (path.isEmpty()) return;
    if (!saveKeymapProfile(path, canvas->profile, &err)) {
        QMessageBox::warning(this, "Export Failed", err);
    }
}

void KeymapDialog::refreshControlList() {
    QSignalBlocker block(controlList);
    controlList->clear();
    int selectedRow = -1;
    auto addEntry = [&](SelType t, int idx, const QString &text) {
        auto *item = new QListWidgetItem(text, controlList);
        item->setData(Qt::UserRole, QPoint(static_cast<int>(t), idx));
        if (canvas->sel.type == t && canvas->sel.index == idx) selectedRow = controlList->count() - 1;
    };

    if (canvas->profile.aim.enabled) {
        addEntry(SelType::Aim, 0, QStringLiteral("⌖ Aim & Shoot (%1)%2")
            .arg(keyName(canvas->profile.aim.toggleKey),
                 canvas->profile.aim.adsEnabled ? QStringLiteral(" + ADS") : QString()));
    }
    for (int i = 0; i < canvas->profile.dpads.size(); ++i) {
        const auto &d = canvas->profile.dpads[i];
        addEntry(SelType::Dpad, i, QStringLiteral("✥ D-Pad (%1/%2/%3/%4) · %5%, %6%")
            .arg(keyShortBadge(d.keyUp), keyShortBadge(d.keyLeft), keyShortBadge(d.keyDown), keyShortBadge(d.keyRight))
            .arg(qRound(d.x * 100)).arg(qRound(d.y * 100)));
    }
    for (int i = 0; i < canvas->profile.swipes.size(); ++i) {
        const auto &sw = canvas->profile.swipes[i];
        addEntry(SelType::Swipe, i, QStringLiteral("↝ Swipe %1%2 · %3ms")
            .arg(keyName(sw.key), sw.label.isEmpty() ? QString() : (" [" + sw.label + "]"))
            .arg(sw.durationMs));
    }
    for (int i = 0; i < canvas->profile.zooms.size(); ++i) {
        const auto &z = canvas->profile.zooms[i];
        addEntry(SelType::Zoom, i, QStringLiteral("⤢ Zoom %1 / %2")
            .arg(keyName(z.keyIn), keyName(z.keyOut)));
    }
    for (int i = 0; i < canvas->profile.mobaSkills.size(); ++i) {
        const auto &m = canvas->profile.mobaSkills[i];
        addEntry(SelType::Moba, i, QStringLiteral("⚡ MOBA %1%2 · %3%, %4%")
            .arg(keyName(m.key), m.label.isEmpty() ? QString() : (" [" + m.label + "]"))
            .arg(qRound(m.x * 100)).arg(qRound(m.y * 100)));
    }
    for (int i = 0; i < canvas->profile.scripts.size(); ++i) {
        const auto &sc = canvas->profile.scripts[i];
        addEntry(SelType::Script, i, QStringLiteral("⌘ Macro %1 [%2]")
            .arg(keyName(sc.key), sc.label));
    }
    for (int i = 0; i < canvas->profile.taps.size(); ++i) {
        const auto &b = canvas->profile.taps[i];
        QString modeTag = b.repeat ? QStringLiteral(" ↻") : (b.burstCount > 1 ? QStringLiteral(" ×%1").arg(b.burstCount) : QString());
        QString lbl = b.label.isEmpty() ? QString() : (" [" + b.label + "]");
        addEntry(SelType::Tap, i, QStringLiteral("● %1%2%3 · %4%, %5%")
            .arg(keyName(b.key), modeTag, lbl).arg(qRound(b.x * 100)).arg(qRound(b.y * 100)));
    }

    if (selectedRow >= 0) controlList->setCurrentRow(selectedRow);
}

void KeymapDialog::refreshInspector() {
    const auto s = canvas->sel;
    switch (s.type) {
    case SelType::Tap:
        if (s.index >= 0 && s.index < canvas->profile.taps.size()) {
            const auto &b = canvas->profile.taps[s.index];
            QSignalBlocker b1(tapModeCombo), b2(tapIntervalSpin), b3(tapBurstSpin),
                           b4(tapXSpin), b5(tapYSpin), b6(tapLabelEdit);
            tapKeyBtn->setBoundKey(b.key);
            const int mode = b.repeat ? 1 : (b.burstCount > 1 ? 2 : 0);
            tapModeCombo->setCurrentIndex(mode);
            tapIntervalSpin->setValue(b.intervalMs);
            tapIntervalSpin->setEnabled(mode != 0);
            tapBurstSpin->setValue(std::max(2, b.burstCount));
            tapBurstSpin->setEnabled(mode == 2);
            tapXSpin->setValue(b.x * 100.0);
            tapYSpin->setValue(b.y * 100.0);
            tapLabelEdit->setText(b.label);
            inspectorStack->setCurrentIndex(1);
            return;
        }
        break;
    case SelType::Dpad:
        if (s.index >= 0 && s.index < canvas->profile.dpads.size()) {
            const auto &d = canvas->profile.dpads[s.index];
            QSignalBlocker b1(dpadRadiusSpin), b2(dpadSprintMultSpin);
            dpadUpBtn->setBoundKey(d.keyUp);
            dpadDownBtn->setBoundKey(d.keyDown);
            dpadLeftBtn->setBoundKey(d.keyLeft);
            dpadRightBtn->setBoundKey(d.keyRight);
            dpadSprintBtn->setBoundKey(d.sprintKey);
            dpadWalkBtn->setBoundKey(d.walkKey);
            dpadRadiusSpin->setValue(d.radius * 100.0);
            dpadSprintMultSpin->setValue(d.sprintMultiplier);
            inspectorStack->setCurrentIndex(2);
            return;
        }
        break;
    case SelType::Swipe:
        if (s.index >= 0 && s.index < canvas->profile.swipes.size()) {
            const auto &sw = canvas->profile.swipes[s.index];
            QSignalBlocker b1(swipeDurationSpin), b2(swipeHoldCheck), b3(swipeLabelEdit);
            swipeKeyBtn->setBoundKey(sw.key);
            swipeDurationSpin->setValue(sw.durationMs);
            swipeHoldCheck->setChecked(sw.holdAtEnd);
            swipeLabelEdit->setText(sw.label);
            inspectorStack->setCurrentIndex(3);
            return;
        }
        break;
    case SelType::Zoom:
        if (s.index >= 0 && s.index < canvas->profile.zooms.size()) {
            const auto &z = canvas->profile.zooms[s.index];
            QSignalBlocker b1(zoomRadiusSpin), b2(zoomDurationSpin);
            zoomInBtn->setBoundKey(z.keyIn);
            zoomOutBtn->setBoundKey(z.keyOut);
            zoomModBtn->setBoundKey(z.modifierKey);
            zoomRadiusSpin->setValue(z.radius * 100.0);
            zoomDurationSpin->setValue(z.durationMs);
            inspectorStack->setCurrentIndex(4);
            return;
        }
        break;
    case SelType::Moba:
        if (s.index >= 0 && s.index < canvas->profile.mobaSkills.size()) {
            const auto &m = canvas->profile.mobaSkills[s.index];
            QSignalBlocker b1(mobaModeCombo), b2(mobaRadiusSpin), b3(mobaCancelCheck), b4(mobaLabelEdit);
            mobaKeyBtn->setBoundKey(m.key);
            mobaModeCombo->setCurrentIndex(mobaModeCombo->findData(static_cast<int>(m.mode)));
            mobaRadiusSpin->setValue(m.radius * 100.0);
            mobaCancelCheck->setChecked(m.cancelEnabled);
            mobaCancelKeyBtn->setBoundKey(m.cancelKey == 0 ? 0x20 : m.cancelKey);
            mobaCancelKeyBtn->setEnabled(m.cancelEnabled);
            mobaLabelEdit->setText(m.label);
            inspectorStack->setCurrentIndex(5);
            return;
        }
        break;
    case SelType::Script:
        if (s.index >= 0 && s.index < canvas->profile.scripts.size()) {
            const auto &sc = canvas->profile.scripts[s.index];
            QSignalBlocker b1(scriptLabelEdit), b2(scriptRepeatCheck), b3(scriptEditor);
            scriptKeyBtn->setBoundKey(sc.key);
            scriptLabelEdit->setText(sc.label);
            scriptRepeatCheck->setChecked(sc.repeatWhileHeld);
            scriptEditor->setPlainText(sc.scriptText);
            QList<ScriptStep> steps; QString err;
            if (parseScriptText(sc.scriptText, &steps, &err))
                scriptStatusLabel->setText(QStringLiteral("✓ Valid macro (%1 steps).").arg(steps.size()));
            else
                scriptStatusLabel->setText(QStringLiteral("⚠ %1").arg(err));
            inspectorStack->setCurrentIndex(6);
            return;
        }
        break;
    case SelType::Aim:
        if (canvas->profile.aim.enabled) {
            inspectorStack->setCurrentIndex(7);
            return;
        }
        break;
    case SelType::None:
        break;
    }
    inspectorStack->setCurrentIndex(0);
}

void KeymapDialog::refreshConflictBanner() {
    const QStringList warnings = detectKeyConflicts(canvas->profile);
    if (warnings.isEmpty()) {
        conflictBanner->setStyleSheet("color: #7cf7c4; font-size: 9pt;");
        conflictBanner->setText("✓ Zero key conflicts across active scheme controls.");
    } else {
        conflictBanner->setStyleSheet("color: #ffd166; font-size: 9pt;");
        conflictBanner->setText("⚠ Key overlap: " + warnings.join("   ·   "));
    }
}

KeymapProfile KeymapDialog::resultProfile() const { return canvas->profile; }
QList<KeyBinding> KeymapDialog::resultBindings() const { return canvas->profile.taps; }
