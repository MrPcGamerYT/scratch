#include "keymapdialog.h"
#include <QComboBox>
#include <QDialogButtonBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QListWidget>
#include <QLineF>
#include <QMouseEvent>
#include <QPainter>
#include <QPushButton>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QVBoxLayout>
#include <functional>
#include <algorithm>

class KeymapCanvas : public QWidget {
public:
    QPixmap image;
    QList<KeyBinding> bindings;
    std::function<int()> selectedKey;
    std::function<bool()> selectedRepeat;
    std::function<int()> selectedInterval;
    std::function<void()> changed;
    explicit KeymapCanvas(const QPixmap &pixmap, QWidget *parent = nullptr) : QWidget(parent), image(pixmap) {
        setMinimumSize(460, 300); setMouseTracking(true);
    }
    QRectF imageRect() const {
        QSizeF size = image.isNull() ? QSizeF(16, 9) : QSizeF(image.size());
        size.scale(QSizeF(width(), height()), Qt::KeepAspectRatio);
        return QRectF((width() - size.width()) / 2, (height() - size.height()) / 2, size.width(), size.height());
    }
    void place(int index, QPointF p) {
        const auto r = imageRect();
        bindings[index].x = std::clamp((p.x() - r.left()) / r.width(), 0.0, 1.0);
        bindings[index].y = std::clamp((p.y() - r.top()) / r.height(), 0.0, 1.0);
        update(); changed();
    }

protected:
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this); painter.setRenderHint(QPainter::Antialiasing);
        painter.fillRect(rect(), QColor("#0b1120"));
        const auto r = imageRect();
        if (!image.isNull()) painter.drawPixmap(r, image, QRectF(image.rect()));
        else { painter.fillRect(r, QColor("#192437")); painter.setPen(QColor("#a5b4cc")); painter.drawText(r, Qt::AlignCenter, "Screenshot unavailable"); }
        for (const auto &binding : bindings) {
            const QPointF point(r.left() + binding.x * r.width(), r.top() + binding.y * r.height());
            painter.setBrush(QColor(43, 123, 255, 210)); painter.setPen(QPen(Qt::white, 2));
            painter.drawEllipse(point, 19, 19); painter.setPen(Qt::white);
            painter.drawText(QRectF(point.x() - 19, point.y() - 19, 38, 38), Qt::AlignCenter, keyName(binding.key));
        }
    }
    void mousePressEvent(QMouseEvent *event) override {
        if (event->button() != Qt::LeftButton || !imageRect().contains(event->position())) return;
        const auto r = imageRect();
        dragging = -1;
        for (int i = bindings.size() - 1; i >= 0; --i) {
            const auto &binding = bindings[i];
            const QPointF point(r.left() + binding.x * r.width(), r.top() + binding.y * r.height());
            if (QLineF(point, event->position()).length() <= 20) { dragging = i; break; }
        }
        if (dragging < 0 && bindings.size() < 128) {
            const int key = selectedKey();
            for (auto &binding : bindings) if (binding.key == key) {
                binding.repeat = selectedRepeat(); binding.intervalMs = selectedInterval();
            }
            bindings.append({key, 0, 0, selectedRepeat(), selectedInterval()});
            dragging = bindings.size() - 1;
        }
        if (dragging >= 0) place(dragging, event->position());
    }
    void mouseMoveEvent(QMouseEvent *event) override { if (dragging >= 0 && (event->buttons() & Qt::LeftButton)) place(dragging, event->position()); }
    void mouseReleaseEvent(QMouseEvent *) override { dragging = -1; }
private:
    int dragging = -1;
};

KeymapDialog::KeymapDialog(const QPixmap &screenshot, const QList<KeyBinding> &bindings, QWidget *parent) : QDialog(parent) {
    setWindowTitle("Edit keymap"); resize(1000, 660);
    auto *layout = new QVBoxLayout(this);
    auto *heading = new QLabel("Place tap controls", this); heading->setObjectName("sectionTitle"); layout->addWidget(heading);
    auto *description = new QLabel("Choose a key, then click to add one or more markers. Drag markers to adjust them. Multiple markers run sequentially; repeat mode loops while the key is held. Multitouch is not supported.", this);
    description->setWordWrap(true); layout->addWidget(description);
    auto *body = new QHBoxLayout;
    canvas = new KeymapCanvas(screenshot, this); canvas->bindings = bindings;
    body->addWidget(canvas, 1);
    auto *side = new QVBoxLayout;
    auto *aimNote = new QLabel("Aim & fire: not available yet. Simultaneous mouse-look and fire require guest multitouch support.", this);
    aimNote->setWordWrap(true); aimNote->setObjectName("muted"); side->addWidget(aimNote);
    auto *keyLabel = new QLabel("Key to place", this); side->addWidget(keyLabel);
    keyChoice = new QComboBox(this);
    for (int key = 'A'; key <= 'Z'; ++key) keyChoice->addItem(keyName(key), key);
    for (int key = '0'; key <= '9'; ++key) keyChoice->addItem(keyName(key), key);
    keyChoice->addItem("Space", 0x20); side->addWidget(keyChoice);
    side->addWidget(new QLabel("Action", this));
    modeChoice = new QComboBox(this); modeChoice->addItem("Hold / tap sequence", false); modeChoice->addItem("Repeat while held", true); side->addWidget(modeChoice);
    side->addWidget(new QLabel("Repeat delay", this));
    interval = new QSpinBox(this); interval->setRange(100, 5000); interval->setSingleStep(50); interval->setValue(250); interval->setSuffix(" ms"); side->addWidget(interval);
    list = new QListWidget(this); list->setMinimumWidth(160); side->addWidget(list, 1);
    auto *remove = new QPushButton("Remove selected", this); side->addWidget(remove);
    body->addLayout(side); layout->addLayout(body, 1);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Save | QDialogButtonBox::Cancel, this); layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(remove, &QPushButton::clicked, this, [this] {
        const int row = list->currentRow();
        if (row >= 0 && row < canvas->bindings.size()) { canvas->bindings.removeAt(row); canvas->update(); updateList(); }
    });
    canvas->selectedKey = [this] { return keyChoice->currentData().toInt(); };
    canvas->selectedRepeat = [this] { return modeChoice->currentData().toBool(); };
    canvas->selectedInterval = [this] { return interval->value(); };
    auto updateMode = [this] {
        const int key = keyChoice->currentData().toInt();
        for (auto &binding : canvas->bindings) if (binding.key == key) {
            binding.repeat = modeChoice->currentData().toBool(); binding.intervalMs = interval->value();
        }
        interval->setEnabled(modeChoice->currentData().toBool()); updateList();
    };
    connect(modeChoice, &QComboBox::currentIndexChanged, this, [updateMode](int) { updateMode(); });
    connect(interval, &QSpinBox::valueChanged, this, [updateMode](int) { updateMode(); });
    connect(keyChoice, &QComboBox::currentIndexChanged, this, [this](int) {
        QSignalBlocker blockMode(modeChoice), blockInterval(interval);
        bool repeat = false; int delay = 250;
        for (const auto &binding : canvas->bindings) if (binding.key == keyChoice->currentData().toInt()) { repeat = binding.repeat; delay = binding.intervalMs; break; }
        modeChoice->setCurrentIndex(repeat ? 1 : 0); interval->setValue(delay); interval->setEnabled(repeat);
    });
    interval->setEnabled(false);
    canvas->changed = [this] { updateList(); };
    updateList();
}
QList<KeyBinding> KeymapDialog::resultBindings() const { return canvas->bindings; }
void KeymapDialog::updateList() {
    const int selected = list->currentRow(); list->clear();
    for (const auto &b : canvas->bindings) list->addItem(QString("%1 %2  ·  %3%, %4%").arg(keyName(b.key), b.repeat ? "↻" : "").arg(qRound(b.x * 100)).arg(qRound(b.y * 100)));
    if (selected >= 0 && selected < list->count()) list->setCurrentRow(selected);
}
