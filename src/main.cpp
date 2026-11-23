#include "mainwindow.h"
#include <QApplication>

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    QApplication::setOrganizationName("ProjectNexus");
    QApplication::setApplicationName("Project Nexus");
    MainWindow window;
    window.show();
    return app.exec();
}
