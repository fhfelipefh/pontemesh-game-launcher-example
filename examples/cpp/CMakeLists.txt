cmake_minimum_required(VERSION 3.20)
project(pontemesh_game_launcher_cpp VERSION 0.3.1 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

include(FetchContent)
FetchContent_Declare(
    nlohmann_json
    GIT_REPOSITORY https://github.com/nlohmann/json.git
    GIT_TAG v3.11.3
    GIT_SHALLOW TRUE
)
FetchContent_MakeAvailable(nlohmann_json)

enable_testing()
add_executable(pontemesh_cpp_tests tests.cpp)
target_link_libraries(pontemesh_cpp_tests PRIVATE nlohmann_json::nlohmann_json)
add_test(NAME pontemesh_cpp_tests COMMAND pontemesh_cpp_tests)

option(PONTEMESH_BUILD_LAUNCHER "Build the native SDK launcher" ON)
if(PONTEMESH_BUILD_LAUNCHER)
    set(PONTEMESH_SDK_INCLUDE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../pontemesh-sdk/bindings/c/include" CACHE PATH "Directory containing pontemesh_sdk.h")
    if(NOT EXISTS "${PONTEMESH_SDK_INCLUDE_DIR}/pontemesh_sdk.h")
        set(PONTEMESH_SDK_INCLUDE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../native/include")
    endif()
    if(NOT EXISTS "${PONTEMESH_SDK_INCLUDE_DIR}/pontemesh_sdk.h")
        message(FATAL_ERROR "pontemesh_sdk.h was not found. Set PONTEMESH_SDK_INCLUDE_DIR to an extracted SDK include directory.")
    endif()
    add_executable(pontemesh_game_launcher_cpp main.cpp)
    target_include_directories(pontemesh_game_launcher_cpp PRIVATE "${PONTEMESH_SDK_INCLUDE_DIR}")
    target_link_libraries(pontemesh_game_launcher_cpp PRIVATE nlohmann_json::nlohmann_json ${CMAKE_DL_LIBS})
    target_compile_definitions(pontemesh_game_launcher_cpp PRIVATE PONTEMESH_REPO_ROOT="${CMAKE_CURRENT_SOURCE_DIR}/../..")
endif()
