cmake_minimum_required(VERSION 3.21)
project(ProjectNexus VERSION 0.1.0 LANGUAGES CXX)

if(NOT WIN32)
    message(FATAL_ERROR "Project Nexus currently targets Windows only (WHPX and Win32 window embedding).")
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
find_package(Qt6 6.2 REQUIRED COMPONENTS Widgets Network)
qt_standard_project_setup()

add_library(nexus_controls STATIC src/keymap.cpp src/keymap.h src/qmp.cpp src/qmp.h src/clicksequence.cpp src/clicksequence.h src/manageddisk.cpp src/manageddisk.h)
target_link_libraries(nexus_controls PUBLIC Qt6::Core)
target_include_directories(nexus_controls PUBLIC src)

add_library(nexus_download STATIC src/imagedownload.cpp src/imagedownload.h)
target_link_libraries(nexus_download PUBLIC Qt6::Network)
target_include_directories(nexus_download PUBLIC src)

qt_add_executable(ProjectNexus WIN32
    src/main.cpp
    src/mainwindow.cpp
    src/mainwindow.h
    src/emulatorview.cpp
    src/emulatorview.h
    src/inputmapper.cpp
    src/inputmapper.h
    src/mouselock.cpp
    src/mouselock.h
    src/keymapdialog.cpp
    src/keymapdialog.h
)
target_link_libraries(ProjectNexus PRIVATE Qt6::Widgets nexus_download nexus_controls user32)

include(CTest)
if(BUILD_TESTING)
    add_executable(download_tests tests/download_tests.cpp)
    target_link_libraries(download_tests PRIVATE nexus_download)
    add_test(NAME download_tests COMMAND download_tests)
    add_executable(keymap_tests tests/keymap_tests.cpp)
    target_link_libraries(keymap_tests PRIVATE nexus_controls)
    add_test(NAME keymap_tests COMMAND keymap_tests)
    add_executable(managed_disk_tests tests/managed_disk_tests.cpp)
    target_link_libraries(managed_disk_tests PRIVATE nexus_controls)
    add_executable(qemu_stub tests/qemu_stub.cpp)
endif()
