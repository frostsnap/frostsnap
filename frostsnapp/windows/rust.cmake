# We include Corrosion inline here, but ideally in a project with
# many dependencies we would need to install Corrosion on the system.
# See instructions on https://github.com/AndrewGaspar/corrosion#cmake-install
# Once done, uncomment this line:
# find_package(Corrosion REQUIRED)

include(FetchContent)

FetchContent_Declare(
    Corrosion
    GIT_REPOSITORY https://github.com/AndrewGaspar/corrosion.git
    GIT_TAG origin/master # Optionally specify a version tag or branch here
)

FetchContent_MakeAvailable(Corrosion)

# note llfourn edited this file to add "CRATES native" below so it wouldn't get all the crates in the workspace
corrosion_import_crate(MANIFEST_PATH ../native/Cargo.toml IMPORTED_CRATES imported_crates CRATES native)
target_link_libraries(${BINARY_NAME} PRIVATE ${imported_crates})
foreach(imported_crate ${imported_crates})
  list(APPEND PLUGIN_BUNDLED_LIBRARIES $<TARGET_FILE:${imported_crate}-shared>)
endforeach()
