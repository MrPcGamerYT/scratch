#pragma once
#include "keymap.h"
#include <QDialog>
#include <QPixmap>

class KeymapCanvas;
class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QLabel;
class QLineEdit;
class QListWidget;
class QPlainTextEdit;
class QPushButton;
class QSlider;
class QSpinBox;
class QStackedWidget;
class KeyBindButton;

class KeymapDialog : public QDialog {
public:
    KeymapDialog(const QPixmap &screenshot, const KeymapProfile &profile,
                 const QString &schemeDir, const QString &activeScheme,
                 QWidget *parent = nullptr);
    KeymapDialog(const QPixmap &screenshot, const QList<KeyBinding> &bindings,
                 QWidget *parent = nullptr);

    KeymapProfile resultProfile() const;
    QString resultSchemeName() const { return currentSchemeName; }
    QList<KeyBinding> resultBindings() const;

private:
    void pushUndoSnapshot();
    void undo();
    void redo();
    void refreshControlList();
    void refreshInspector();
    void refreshConflictBanner();
    void applyPreset(const QString &presetId);
    void importSchemeFile();
    void exportSchemeFile();
    void createNewScheme();

    QString schemesDirectory;
    QString currentSchemeName = QStringLiteral("default");
    QList<KeymapProfile> undoStack;
    QList<KeymapProfile> redoStack;

    KeymapCanvas *canvas = nullptr;
    QComboBox *schemeCombo = nullptr;
    QPushButton *undoBtn = nullptr;
    QPushButton *redoBtn = nullptr;
    QCheckBox *snapCheck = nullptr;
    QSlider *opacitySlider = nullptr;
    QLabel *opacityValueLabel = nullptr;
    QListWidget *controlList = nullptr;
    QStackedWidget *inspectorStack = nullptr;
    QLabel *conflictBanner = nullptr;

    // Tap inspector widgets
    KeyBindButton *tapKeyBtn = nullptr;
    QComboBox *tapModeCombo = nullptr;
    QSpinBox *tapIntervalSpin = nullptr;
    QSpinBox *tapBurstSpin = nullptr;
    QLineEdit *tapLabelEdit = nullptr;
    QDoubleSpinBox *tapXSpin = nullptr;
    QDoubleSpinBox *tapYSpin = nullptr;

    // D-pad inspector widgets
    KeyBindButton *dpadUpBtn = nullptr;
    KeyBindButton *dpadDownBtn = nullptr;
    KeyBindButton *dpadLeftBtn = nullptr;
    KeyBindButton *dpadRightBtn = nullptr;
    KeyBindButton *dpadSprintBtn = nullptr;
    KeyBindButton *dpadWalkBtn = nullptr;
    QDoubleSpinBox *dpadRadiusSpin = nullptr;
    QDoubleSpinBox *dpadSprintMultSpin = nullptr;

    // Swipe inspector widgets
    KeyBindButton *swipeKeyBtn = nullptr;
    QSpinBox *swipeDurationSpin = nullptr;
    QCheckBox *swipeHoldCheck = nullptr;
    QLineEdit *swipeLabelEdit = nullptr;

    // Zoom inspector widgets
    KeyBindButton *zoomInBtn = nullptr;
    KeyBindButton *zoomOutBtn = nullptr;
    KeyBindButton *zoomModBtn = nullptr;
    QDoubleSpinBox *zoomRadiusSpin = nullptr;
    QSpinBox *zoomDurationSpin = nullptr;

    // MOBA inspector widgets
    KeyBindButton *mobaKeyBtn = nullptr;
    QComboBox *mobaModeCombo = nullptr;
    QDoubleSpinBox *mobaRadiusSpin = nullptr;
    QCheckBox *mobaCancelCheck = nullptr;
    KeyBindButton *mobaCancelKeyBtn = nullptr;
    QLineEdit *mobaLabelEdit = nullptr;

    // Script inspector widgets
    KeyBindButton *scriptKeyBtn = nullptr;
    QLineEdit *scriptLabelEdit = nullptr;
    QCheckBox *scriptRepeatCheck = nullptr;
    QPlainTextEdit *scriptEditor = nullptr;
    QPushButton *pickCoordBtn = nullptr;
    QLabel *scriptStatusLabel = nullptr;
};
