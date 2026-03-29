cmake_minimum_required(VERSION 3.7)

# Create the project.
project(ted-experiments)
set(CMAKE_CXX_STANDARD 17)
# Compiler flags.
# MUST be declared after project().
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
  set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++17 -Wall -O0 -g")
else ()
  set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++17 -Wall -O3 -flto")
endif ()


# echo out build type
message(STATUS "Build type: ${CMAKE_BUILD_TYPE}")

# Verify which timing implementation to use.
# NOTE: Comes from common-code.
include(CheckSymbolExists)
CHECK_SYMBOL_EXISTS(clock_gettime "time.h" HAVE_CLOCK_GETTIME)
if(NOT HAVE_CLOCK_GETTIME)
  add_definitions(-DGETRUSAGE)
endif()

# Moravec LB experiments
add_executable(query_validate
        src/lb/query_validate.cpp
)


target_include_directories(
        query_validate
        PUBLIC external/common-code/timings
)


# Create header-only library 'TreeSimilarity' with our algorithms.
add_library(
  TreeSimilarity INTERFACE
)

target_include_directories(
  TreeSimilarity INTERFACE
  external/tree-similarity/src/cost_model
  external/tree-similarity/src/data_structures
  external/tree-similarity/src/join
  external/tree-similarity/src/join/tjoin
  external/tree-similarity/src/join/tang
  external/tree-similarity/src/join/naive
  external/tree-similarity/src/join/binary_branches
  external/tree-similarity/src/join/label_histogram
  external/tree-similarity/src/join/degree_histogram
  external/tree-similarity/src/join/leaf_dist_histogram
  external/tree-similarity/src/join/histogram
  external/tree-similarity/src/label
  external/tree-similarity/src/node
  external/tree-similarity/src/parser
  external/tree-similarity/src/ted
  external/tree-similarity/src/ted_ub
  external/tree-similarity/src/ted_lb
  external/tree-similarity/src/join/guha
)


target_link_libraries(query_validate TreeSimilarity)
