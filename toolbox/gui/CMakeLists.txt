cmake_minimum_required(VERSION 3.24)
project(samurai-toolbox-gui VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

option(SAMURAI_GUI_DEVTOOLS "Test build: --cli, --pkexec and --grab options" OFF)

find_package(Qt6 6.7 REQUIRED COMPONENTS Core Gui Widgets Network Qml Quick QuickControls2)
qt_standard_project_setup(REQUIRES 6.7)
include(GNUInstallDirs)

# the privileged operation table: shared by the app (display, early refusal) and the root helper
add_library(samurai-operations STATIC common/operations.cpp)

qt_add_executable(samurai-toolbox src/main.cpp src/backend.cpp src/logmodel.cpp)
set(qml_files
    qml/Main.qml
    qml/components/Badge.qml
    qml/components/Card.qml
    qml/components/ConfirmDialog.qml
    qml/components/LogPanel.qml
    qml/components/Plain.qml
    qml/components/PageBody.qml
    qml/pages/OverviewPage.qml
    qml/pages/HealthPage.qml
    qml/pages/AppsPage.qml
    qml/pages/CleanupPage.qml
    qml/pages/KernelsPage.qml
    qml/pages/UpdatesPage.qml
    qml/pages/SnapshotsPage.qml
    qml/pages/DiagnosticsPage.qml
    qml/pages/HardwarePage.qml
)
# one flat module: every file can name every other one without per-directory imports
foreach(file IN LISTS qml_files)
    get_filename_component(name ${file} NAME)
    set_source_files_properties(${file} PROPERTIES QT_RESOURCE_ALIAS ${name})
endforeach()
qt_add_qml_module(samurai-toolbox
    URI org.samurai.toolbox
    VERSION 1.0
    NO_PLUGIN
    QML_FILES ${qml_files}
)
target_link_libraries(samurai-toolbox PRIVATE
    samurai-operations Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Network Qt6::Qml Qt6::Quick
    Qt6::QuickControls2)
if(SAMURAI_GUI_DEVTOOLS)
    target_compile_definitions(samurai-toolbox PRIVATE SAMURAI_GUI_DEVTOOLS)
endif()

# runs as root through pkexec: no Qt, only the standard library
add_executable(gui-helper helper/main.cpp)
target_link_libraries(gui-helper PRIVATE samurai-operations)
set_target_properties(gui-helper PROPERTIES AUTOMOC OFF)

install(TARGETS samurai-toolbox RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
# the polkit policy names this exact path
install(TARGETS gui-helper RUNTIME DESTINATION ${CMAKE_INSTALL_PREFIX}/lib/samurai)
install(FILES data/org.samurai.toolbox.policy DESTINATION ${CMAKE_INSTALL_DATADIR}/polkit-1/actions)
install(FILES data/samurai-toolbox.desktop DESTINATION ${CMAKE_INSTALL_DATADIR}/applications)
