# We include Corrosion inline here, but ideally in a project with
# many dependencies we would need to install Corrosion on the system.
# See instructions on https://github.com/AndrewGaspar/corrosion#cmake-install
# Once done, uncomment this line:
# find_package(Corrosion REQUIRED)

include(FetchContent)

FetchContent_Declare(
    Corrosion
    GIT_REPOSITORY https://github.com/AndrewGaspar/corrosion.git
    # llfourn fixed this so it wouldn't got to the internet all the time
    GIT_TAG b1fab721655c5c4b1b08a083d3cd29f163af75d0
)

FetchContent_MakeAvailable(Corrosion)

# note llfourn edited this file to add "CRATES native" below so it wouldn't get all the crates in the workspace
corrosion_import_crate(MANIFEST_PATH ../native/Cargo.toml IMPORTED_CRATES imported_crates CRATES native)
target_link_libraries(${BINARY_NAME} PRIVATE ${imported_crates})
foreach(imported_crate ${imported_crates})
  list(APPEND PLUGIN_BUNDLED_LIBRARIES $<TARGET_FILE:${imported_crate}-shared>)
endforeach()
