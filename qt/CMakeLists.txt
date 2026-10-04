cmake_minimum_required(VERSION 3.25)
project(ArcadeLinkQt LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Core Network Test)

add_library(arcade_link_qt STATIC ArcadeLink.h ArcadeLink.cpp)
target_include_directories(arcade_link_qt PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(arcade_link_qt PUBLIC Qt6::Core Qt6::Network)

enable_testing()
foreach(test_name IN ITEMS Vectors Server)
    add_executable(tst_${test_name} tests/tst_${test_name}.cpp)
    target_link_libraries(tst_${test_name} PRIVATE arcade_link_qt Qt6::Test)
    target_compile_definitions(tst_${test_name} PRIVATE
        ARCADE_LINK_VECTORS="${CMAKE_CURRENT_SOURCE_DIR}/../spec/vectors"
        ARCADE_LINK_FIXTURES="${CMAKE_CURRENT_SOURCE_DIR}/../fixtures")
    add_test(NAME ${test_name} COMMAND tst_${test_name})
endforeach()
