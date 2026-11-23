#include "aimmodel.h"
#include "qmp.h"
#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonObject>
#include <cmath>
#include <iostream>
int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    AimModel a(QPointF(.5,.5), 1.0);
    if (a.move(100, -50, 1000, 1000) || std::abs(a.position().x()-.6) > 0.0001 || std::abs(a.position().y()-.45) > 0.0001) return 1;
    if (a.move(1000, 0, 1000, 1000) != true || a.position() != a.anchor()) return 2;
    if (a.move(100, 100, 0, 0) || a.position() != a.anchor()) return 3;
    AimModel b(QPointF(0, 1), 10); if (b.anchor() != QPointF(.1,.9)) return 4;
    auto axes = Qmp::moveEvents(.25, .75);
    if (axes.size() != 2 || axes[0].toObject().value("type").toString() != "abs" ||
        axes[1].toObject().value("data").toObject().value("value").toInt() != 24575) return 5;
    std::cout << "Aim gesture arithmetic and pointer motion passed; no Windows hook or Android test.\n";
    return 0;
}
