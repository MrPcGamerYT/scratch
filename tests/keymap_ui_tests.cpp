#include "keymapdialog.h"
#include <QApplication>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QTemporaryDir>
#include <iostream>
static void click(QWidget *canvas, QPointF point, Qt::MouseButton button = Qt::LeftButton) {
    QMouseEvent press(QEvent::MouseButtonPress, point, button, button, Qt::NoModifier);
    QApplication::sendEvent(canvas, &press);
    QMouseEvent release(QEvent::MouseButtonRelease, point, button, Qt::NoButton, Qt::NoModifier);
    QApplication::sendEvent(canvas, &release);
}
static void key(QWidget *canvas, int code) {
    QKeyEvent event(QEvent::KeyPress, code, Qt::NoModifier);
    QApplication::sendEvent(canvas, &event);
}
int main(int argc, char **argv) {
    QApplication app(argc, argv); QTemporaryDir directory;
    KeymapDialog dialog(QPixmap(), KeymapProfile(), directory.path(), "test");
    dialog.show(); app.processEvents();
    auto *canvas = dialog.findChild<QWidget *>("keymapCanvas");
    if (!canvas) return 1;
    const QPointF a(canvas->width() * .3, canvas->height() * .4);
    const QPointF b(canvas->width() * .7, canvas->height() * .6);
    click(canvas, a);
    if (!dialog.resultBindings().isEmpty()) return 2;
    key(canvas, Qt::Key_B);
    if (dialog.resultBindings().size() != 1 || dialog.resultBindings()[0].key != 'B') return 3;
    click(canvas, b); key(canvas, Qt::Key_Escape);
    if (dialog.resultBindings().size() != 1) return 4;
    click(canvas, b); click(canvas, b, Qt::RightButton);
    if (dialog.resultBindings().size() != 2 || dialog.resultBindings()[1].key != MouseRightButton) return 5;
    click(canvas, QPointF(canvas->width()*.5, canvas->height()*.7));
    key(canvas, Qt::Key_Tab);
    if (dialog.resultBindings().size()!=3 || dialog.resultBindings()[2].key!=0x09) return 6;
    std::cout << "HUD click-to-bind, pending cancellation, mouse and Tab capture passed.\n";
    return 0;
}
