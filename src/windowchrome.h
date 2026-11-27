#pragma once
#include <QObject>

// Application-wide chrome for native top-level dialogs and player windows.
class WindowChrome : public QObject {
public:
    using QObject::QObject;
protected:
    bool eventFilter(QObject *object, QEvent *event) override;
};
