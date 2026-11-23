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
#include <QVBoxLayout>
#include <functional>
#include <algorithm>

class KeymapCanvas : public QWidget {
public:
    QPixmap image;
    QList<KeyBinding> bindings;
    std::function<int()> selectedKey;
    std::function<void()> changed;
    explicit KeymapCanvas(const QPixmap &pixmap, QWidget *parent = nullptr) : QWidget(parent), image(pixmap) {
        setMinimumSize(460, 300); setMouseTracking(true);
    }
    QRectF imageRect() const {
        QSizeF size = image.isNull() ? QSizeF(16, 9) : QSizeF(image.size());
        size.scale(QSizeF(width(), height()), Qt::KeepAspectRatio);
        return QRectF((width() - size.width()) / 2, (height() - size.height()) / 2, size.width(), size.height());
    }
    void place(int key, QPointF p) {
        const auto r = imageRect();
        const double x = std::clamp((p.x() - r.left()) / r.width(), 0.0, 1.0);
        const double y = std::clamp((p.y() - r.top()) / r.height(), 0.0, 1.0);
        for (auto &binding : bindings) if (binding.key == key) { binding.x = x; binding.y = y; update(); changed(); return; }
        bindings.append({key, x, y}); update(); changed();
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
        dragging = 0;
        for (const auto &binding : bindings) {
            const QPointF point(r.left() + binding.x * r.width(), r.top() + binding.y * r.height());
            if (QLineF(point, event->position()).length() <= 20) { dragging = binding.key; break; }
        }
        if (!dragging) dragging = selectedKey();
        place(dragging, event->position());
    }
    void mouseMoveEvent(QMouseEvent *event) override { if (dragging && (event->buttons() & Qt::LeftButton)) place(dragging, event->position()); }
    void mouseReleaseEvent(QMouseEvent *) override { dragging = 0; }
private:
    int dragging = 0;
};

KeymapDialog::KeymapDialog(const QPixmap &screenshot, const QList<KeyBinding> &bindings, QWidget *parent) : QDialog(parent) {
    setWindowTitle("Edit keymap"); resize(1000, 660);
    auto *layout = new QVBoxLayout(this);
    auto *heading = new QLabel("Place tap controls", this); heading->setObjectName("sectionTitle"); layout->addWidget(heading);
    auto *description = new QLabel("Choose a key, then click the matching spot in the screenshot. Drag markers to adjust them. Each key holds one virtual mouse click; multitouch and joystick controls are not supported yet.", this);
    description->setWordWrap(true); layout->addWidget(description);
    auto *body = new QHBoxLayout;
    canvas = new KeymapCanvas(screenshot, this); canvas->bindings = bindings;
    body->addWidget(canvas, 1);
    auto *side = new QVBoxLayout;
    auto *keyLabel = new QLabel("Key to place", this); side->addWidget(keyLabel);
    keyChoice = new QComboBox(this);
    for (int key = 'A'; key <= 'Z'; ++key) keyChoice->addItem(keyName(key), key);
    for (int key = '0'; key <= '9'; ++key) keyChoice->addItem(keyName(key), key);
    keyChoice->addItem("Space", 0x20); side->addWidget(keyChoice);
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
    canvas->changed = [this] { updateList(); };
    updateList();
}
QList<KeyBinding> KeymapDialog::resultBindings() const { return canvas->bindings; }
void KeymapDialog::updateList() {
    const int selected = list->currentRow(); list->clear();
    for (const auto &b : canvas->bindings) list->addItem(QString("%1  ·  %2%, %3%").arg(keyName(b.key)).arg(qRound(b.x * 100)).arg(qRound(b.y * 100)));
    if (selected >= 0 && selected < list->count()) list->setCurrentRow(selected);
}
