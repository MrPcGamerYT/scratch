#include "windowchrome.h"
#include <QDialog>
#include <QEvent>
#include <QHBoxLayout>
#include <QLabel>
#include <QLayout>
#include <QMainWindow>
#include <QMouseEvent>
#include <QPushButton>
#include <QSizeGrip>
#include <QVBoxLayout>
#include <QVariant>
#include <QWidget>
#include <QWindow>

namespace {
class TitleBar : public QWidget {
public:
    TitleBar(QWidget *window) : QWidget(window), target(window) {
        setObjectName("nativeTitleBar"); setFixedHeight(window->property("nexusPlayer").toBool() ? 56 : 38);
        setStyleSheet("#nativeTitleBar{background:#090c12;} #nativeTitleBar QLabel{color:#bdc7da;background:transparent;font-size:12px;} #nativeTitleBar QPushButton{background:transparent;border:0;border-radius:4px;padding:4px;color:#cbd5e1;} #nativeTitleBar QPushButton:hover{background:#253149;}");
        auto *row = new QHBoxLayout(this); row->setContentsMargins(12, 3, 6, 3);
        auto *label = new QLabel(window->windowTitle(), this); label->setAttribute(Qt::WA_TransparentForMouseEvents); row->addWidget(label); row->addStretch();
        connect(window, &QWidget::windowTitleChanged, label, &QLabel::setText);
        if (window->property("nexusPlayer").toBool()) {
            label->setText("NEXUS"); label->setStyleSheet("font-weight:800;font-size:15px;letter-spacing:2px;color:#60a5fa;");
            if (auto *dock = window->findChild<QWidget *>("topDock")) { dock->setParent(this); row->addWidget(dock); dock->show(); }
            row->addStretch();
        }
        auto button = [&](const QString &text, const QString &description) {
            auto *b = new QPushButton(text, this); b->setFixedSize(32, 28); b->setAccessibleName(description); b->setToolTip(description); row->addWidget(b); return b;
        };
        connect(button("−", "Minimize"), &QPushButton::clicked, window, &QWidget::showMinimized);
        connect(button("□", "Maximize or restore"), &QPushButton::clicked, window, [window] { window->isMaximized() ? window->showNormal() : window->showMaximized(); });
        auto *close = button("×", "Close window"); close->setStyleSheet("QPushButton:hover{background:#922b40;}");
        connect(close, &QPushButton::clicked, window, &QWidget::close);
    }
protected:
    void mousePressEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton && target->windowHandle()) target->windowHandle()->startSystemMove();
    }
    void mouseDoubleClickEvent(QMouseEvent *event) override {
        if (event->button() == Qt::LeftButton) target->isMaximized() ? target->showNormal() : target->showMaximized();
    }
private:
    QWidget *target;
};
}
bool WindowChrome::eventFilter(QObject *object, QEvent *event) {
    auto *widget = qobject_cast<QWidget *>(object);
    if (event->type() != QEvent::Show || !widget || !widget->isWindow() || widget->property("nexusChrome").toBool() ||
        (!qobject_cast<QMainWindow *>(widget) && !qobject_cast<QDialog *>(widget)) || widget->windowFlags().testFlag(Qt::FramelessWindowHint))
        return QObject::eventFilter(object, event);
    widget->setProperty("nexusChrome", true);
    const auto geometry = widget->geometry();
    const auto state = widget->windowState();
    widget->setWindowFlag(Qt::FramelessWindowHint, true);
    auto *bar = new TitleBar(widget);
    if (auto *main = qobject_cast<QMainWindow *>(widget)) {
        main->setMenuWidget(bar);
    } else if (auto *layout = qobject_cast<QVBoxLayout *>(widget->layout())) {
        layout->insertWidget(0, bar);
        auto *grip = new QSizeGrip(widget); layout->addWidget(grip, 0, Qt::AlignRight);
    } else {
        // Standard message/file dialogs can use grid layouts; retain their native
        // frame rather than hiding the only close/move affordance.
        widget->setWindowFlag(Qt::FramelessWindowHint, false); delete bar;
    }
    widget->setGeometry(geometry); widget->setWindowState(state); widget->show();
    return QObject::eventFilter(object, event);
}
