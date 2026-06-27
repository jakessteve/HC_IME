find_package(Fcitx5Core REQUIRED)

include(GNUInstallDirs)
find_program(CARGO_EXECUTABLE cargo REQUIRED)

set(HC_CORE_MANIFEST ${CMAKE_CURRENT_SOURCE_DIR}/../hc_core/Cargo.toml)
set(HC_CORE_TARGET_DIR ${CMAKE_CURRENT_BINARY_DIR}/cargo-target)
set(HC_CORE_SHARED ${HC_CORE_TARGET_DIR}/release/libhc_core.so)
set(HC_CORE_RUST_SOURCES
    ${CMAKE_CURRENT_SOURCE_DIR}/../hc_core/Cargo.toml
    ${CMAKE_CURRENT_SOURCE_DIR}/../hc_core/src/lib.rs
)

add_custom_command(
    OUTPUT ${HC_CORE_SHARED}
    COMMAND ${CARGO_EXECUTABLE} build --manifest-path ${HC_CORE_MANIFEST} --release --target-dir ${HC_CORE_TARGET_DIR}
    DEPENDS ${HC_CORE_RUST_SOURCES}
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/..
    VERBATIM
)

add_custom_target(hc_core_rust ALL DEPENDS ${HC_CORE_SHARED})

add_fcitx5_addon(hcime src/hcime.cpp)
add_dependencies(hcime hc_core_rust)
target_include_directories(hcime PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_link_directories(hcime PRIVATE ${HC_CORE_TARGET_DIR}/release)
target_link_libraries(hcime PRIVATE Fcitx5::Core Fcitx5::Config hc_core)
target_compile_features(hcime PRIVATE cxx_std_20)
set_target_properties(hcime PROPERTIES BUILD_RPATH "${HC_CORE_TARGET_DIR}/release;$ORIGIN" INSTALL_RPATH "$ORIGIN")

install(TARGETS hcime DESTINATION ${FCITX_INSTALL_ADDONDIR})
install(FILES ${HC_CORE_SHARED} DESTINATION ${FCITX_INSTALL_ADDONDIR})
install(FILES fcitx/hcime.conf DESTINATION ${FCITX_INSTALL_PKGDATADIR}/addon)
install(FILES fcitx/hcime-inputmethod.conf DESTINATION ${FCITX_INSTALL_PKGDATADIR}/inputmethod RENAME hcime.conf)
