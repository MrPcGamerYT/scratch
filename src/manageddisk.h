#pragma once
#include <QString>
// Creates a per-user copy-on-write disk; never modifies or overwrites the base.
bool prepareManagedDisk(const QString &qemuImg, const QString &base, const QString &overlay, QString *error);
