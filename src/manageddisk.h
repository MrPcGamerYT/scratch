#pragma once
#include <QString>
// Creates a per-user copy-on-write disk; never modifies or overwrites the base.
bool prepareManagedDisk(const QString &qemuImg, const QString &base, const QString &overlay, QString *error);
// Preserve an initialized per-user UEFI variable store, tied to its template.
bool prepareManagedFirmware(const QString &firmwareTemplate, const QString &userVars, QString *error);
