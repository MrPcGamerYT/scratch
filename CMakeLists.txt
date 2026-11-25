cmake_minimum_required(VERSION 3.21)
project(ProjectNexus VERSION 0.1.0 LANGUAGES CXX)

if(NOT WIN32)
    message(FATAL_ERROR "Project Nexus currently targets Windows only (WHPX and Win32 window embedding).")
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
find_package(Qt6 6.2 REQUIRED COMPONENTS Widgets Network)
qt_standard_project_setup()

# The Qt shell remains C++; its input policy is incrementally migrating to Rust.
# Rust's MSVC static library is built once in Release and linked into the player.
if(NOT MSVC OR NOT CMAKE_SIZEOF_VOID_P EQUAL 8)
    message(FATAL_ERROR "The Rust player core currently requires an x64 MSVC build.")
endif()
find_program(CARGO_EXECUTABLE cargo REQUIRED)
set(NEXUS_RUST_DIR "${CMAKE_CURRENT_SOURCE_DIR}/rust/nexus-core")
set(NEXUS_RUST_LIB "${CMAKE_CURRENT_BINARY_DIR}/cargo/x86_64-pc-windows-msvc/release/nexus_core.lib")
add_custom_command(OUTPUT "${NEXUS_RUST_LIB}"
    COMMAND "${CARGO_EXECUTABLE}" build --locked --release --target x86_64-pc-windows-msvc
            --manifest-path "${NEXUS_RUST_DIR}/Cargo.toml" --target-dir "${CMAKE_CURRENT_BINARY_DIR}/cargo"
    DEPENDS "${NEXUS_RUST_DIR}/Cargo.toml" "${NEXUS_RUST_DIR}/Cargo.lock" "${NEXUS_RUST_DIR}/src/lib.rs" "${NEXUS_RUST_DIR}/src/runtime.rs"
    VERBATIM)
add_custom_target(nexus_rust_build DEPENDS "${NEXUS_RUST_LIB}")
add_library(nexus_rust STATIC IMPORTED GLOBAL)
set_target_properties(nexus_rust PROPERTIES IMPORTED_LOCATION "${NEXUS_RUST_LIB}")
add_dependencies(nexus_rust nexus_rust_build)

add_library(nexus_controls STATIC
    src/keymap.cpp src/keymap.h
    src/qmp.cpp src/qmp.h
    src/clicksequence.cpp src/clicksequence.h
    src/multisequence.cpp src/multisequence.h
    src/keymapengine.cpp src/keymapengine.h
    src/manageddisk.cpp src/manageddisk.h
    src/aimmodel.cpp src/aimmodel.h
)
add_dependencies(nexus_controls nexus_rust_build)
target_link_libraries(nexus_controls PUBLIC Qt6::Core nexus_rust ws2_32 userenv bcrypt ntdll advapi32 synchronization)
target_include_directories(nexus_controls PUBLIC src)

add_library(nexus_download STATIC src/imagedownload.cpp src/imagedownload.h)
target_link_libraries(nexus_download PUBLIC Qt6::Network)
target_include_directories(nexus_download PUBLIC src)

qt_add_executable(ProjectNexus WIN32
    src/main.cpp
    src/windowchrome.cpp src/windowchrome.h
    src/mainwindow.cpp
    src/mainwindow.h
    src/settingsdialog.cpp
    src/settingsdialog.h
    src/emulatorview.cpp
    src/emulatorview.h
    src/inputmapper.cpp
    src/inputmapper.h
    src/mouselock.cpp
    src/mouselock.h
    src/rawmouse.cpp
    src/rawmouse.h
    src/aimcontroller.cpp
    src/aimcontroller.h
    src/aimsettings.cpp
    src/aimsettings.h
    src/keymapdialog.cpp
    src/keymapdialog.h
    src/keymapoverlay.cpp
    src/keymapoverlay.h
)
qt_add_resources(ProjectNexus "assets" PREFIX "/" FILES assets/nexus-wallpaper.png)
target_link_libraries(ProjectNexus PRIVATE Qt6::Widgets nexus_controls user32)

qt_add_executable(NexusImageStudio WIN32 src/imagestudio.cpp src/windowchrome.cpp src/windowchrome.h)
target_link_libraries(NexusImageStudio PRIVATE Qt6::Widgets)

include(CTest)
if(BUILD_TESTING)
    add_executable(vmplan_tests tests/vmplan_tests.cpp)
    target_link_libraries(vmplan_tests PRIVATE nexus_controls)
    add_test(NAME vmplan_tests COMMAND vmplan_tests)
    add_executable(download_tests tests/download_tests.cpp)
    target_link_libraries(download_tests PRIVATE nexus_download)
    add_test(NAME download_tests COMMAND download_tests)
    add_executable(keymap_tests tests/keymap_tests.cpp)
    target_link_libraries(keymap_tests PRIVATE nexus_controls)
    add_test(NAME keymap_tests COMMAND keymap_tests)
    add_executable(rawmouse_tests tests/rawmouse_tests.cpp src/rawmouse.cpp src/rawmouse.h)
    target_include_directories(rawmouse_tests PRIVATE src)
    target_link_libraries(rawmouse_tests PRIVATE user32)
    add_test(NAME rawmouse_tests COMMAND rawmouse_tests)
    add_executable(aimmodel_tests tests/aimmodel_tests.cpp)
    target_link_libraries(aimmodel_tests PRIVATE nexus_controls)
    add_test(NAME aimmodel_tests COMMAND aimmodel_tests)
    add_executable(managed_disk_tests tests/managed_disk_tests.cpp)
    target_link_libraries(managed_disk_tests PRIVATE nexus_controls)
    add_test(NAME managed_disk_tests COMMAND managed_disk_tests)
    add_executable(keymap_ui_tests tests/keymap_ui_tests.cpp src/keymapdialog.cpp src/keymapdialog.h src/aimsettings.cpp src/aimsettings.h)
    target_link_libraries(keymap_ui_tests PRIVATE Qt6::Widgets nexus_controls)
    add_test(NAME keymap_ui_tests COMMAND keymap_ui_tests)
    set_tests_properties(keymap_ui_tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
    add_executable(studiobrowse_tests tests/studiobrowse_tests.cpp src/windowchrome.cpp src/windowchrome.h src/studiobrowse.h)
    target_include_directories(studiobrowse_tests PRIVATE src)
    target_link_libraries(studiobrowse_tests PRIVATE Qt6::Widgets)
    add_test(NAME studiobrowse_tests COMMAND studiobrowse_tests)
    set_tests_properties(studiobrowse_tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 30)
    add_executable(settings_tests tests/settings_tests.cpp src/settingsdialog.cpp src/settingsdialog.h src/windowchrome.cpp src/windowchrome.h)
    target_include_directories(settings_tests PRIVATE src)
    target_link_libraries(settings_tests PRIVATE Qt6::Widgets)
    add_test(NAME settings_tests COMMAND settings_tests)
    set_tests_properties(settings_tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
    add_test(NAME image_studio_ui_tests COMMAND NexusImageStudio --smoke-test)
    set_tests_properties(image_studio_ui_tests PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
    add_executable(qemu_stub tests/qemu_stub.cpp)
endif()
