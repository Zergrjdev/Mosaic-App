cmake_minimum_required(VERSION 3.21)
project(Mosaic VERSION 1.3.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.4 REQUIRED COMPONENTS Core Gui Network Quick WebView)
qt_standard_project_setup()

qt_add_executable(Mosaic
    src/main.cpp
    src/AssetServer.cpp
    src/AssetServer.h
)

# Embed the whole web app inside the binary (works offline, nothing to install alongside it).
file(GLOB_RECURSE WEB_FILES CONFIGURE_DEPENDS RELATIVE ${CMAKE_CURRENT_SOURCE_DIR} web/*)
qt_add_resources(Mosaic "web" PREFIX "/web" BASE web FILES ${WEB_FILES})
qt_add_resources(Mosaic "qml" PREFIX "/qml" BASE qml FILES qml/Main.qml)

target_compile_definitions(Mosaic PRIVATE APP_VERSION="${PROJECT_VERSION}")
target_link_libraries(Mosaic PRIVATE Qt6::Gui Qt6::Network Qt6::Quick Qt6::WebView)

set_target_properties(Mosaic PROPERTIES
    WIN32_EXECUTABLE TRUE
    MACOSX_BUNDLE TRUE
    MACOSX_BUNDLE_GUI_IDENTIFIER dev.zergrjdev.mosaic
    MACOSX_BUNDLE_BUNDLE_NAME Mosaic
    MACOSX_BUNDLE_SHORT_VERSION_STRING ${PROJECT_VERSION}
    MACOSX_BUNDLE_BUNDLE_VERSION ${PROJECT_VERSION}
)

if(ANDROID)
    # Custom manifest + network config: Android blocks http:// even to 127.0.0.1 unless allowed.
    set_property(TARGET Mosaic PROPERTY QT_ANDROID_PACKAGE_SOURCE_DIR
                 ${CMAKE_CURRENT_SOURCE_DIR}/platform/android)
endif()

# iOS: -DMOSAIC_IOS_UNSIGNED=ON builds a device app with no code signing at all (no Apple account needed).
# The result is wrapped into an .ipa by platform/ios/build-unsigned-ipa.sh and has to be re-signed
# (AltStore, Sideloadly, TrollStore or Xcode) before it can be installed.
option(MOSAIC_IOS_UNSIGNED "iOS: build without code signing (to be re-signed when installed)" OFF)

if(IOS)
    set_target_properties(Mosaic PROPERTIES
        MACOSX_BUNDLE_INFO_PLIST ${CMAKE_CURRENT_SOURCE_DIR}/platform/ios/Info.plist
        XCODE_ATTRIBUTE_PRODUCT_BUNDLE_IDENTIFIER dev.zergrjdev.mosaic
    )
    if(MOSAIC_IOS_UNSIGNED)
        set_target_properties(Mosaic PROPERTIES
            XCODE_ATTRIBUTE_CODE_SIGNING_ALLOWED NO
            XCODE_ATTRIBUTE_CODE_SIGNING_REQUIRED NO
            XCODE_ATTRIBUTE_CODE_SIGN_IDENTITY ""
            XCODE_ATTRIBUTE_DEVELOPMENT_TEAM ""
        )
    endif()
    qt_import_qml_plugins(Mosaic)
endif()

# On Android, Qt builds the app as a shared library (a CMake "module" target), which needs a LIBRARY destination.
install(TARGETS Mosaic
    BUNDLE DESTINATION .
    RUNTIME DESTINATION bin
    LIBRARY DESTINATION lib
)
