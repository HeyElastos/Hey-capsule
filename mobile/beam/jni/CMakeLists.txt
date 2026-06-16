cmake_minimum_required(VERSION 3.22)
project(heybeam CXX)

# Hey's BEAM JNI shim -> libbeam.so. Driven by build-beam.sh, which passes:
#   BEAM_SRC      = the pinned BeamMW/beam checkout (for headers)
#   BEAM_BUILD    = the per-ABI BEAM build dir (for the built static wallet-client libs)
#   BOOST_ANDROID = the reshaped Boost root (include/ + libs/<abi>/libboost_*-..-1_68.a, Boost 1.82)
#   OPENSSL_ANDROID = the reshaped OpenSSL root (include/ + libs/<abi>/lib{crypto,ssl}.a)
# ANDROID_ABI is provided by the NDK toolchain.
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT DEFINED BEAM_SRC OR NOT DEFINED BEAM_BUILD OR NOT DEFINED BOOST_ANDROID OR NOT DEFINED OPENSSL_ANDROID)
  message(FATAL_ERROR "pass -DBEAM_SRC -DBEAM_BUILD -DBOOST_ANDROID -DOPENSSL_ANDROID (use build-beam.sh)")
endif()

add_library(beam SHARED hey_beam_jni.cpp)

target_include_directories(beam PRIVATE
  ${BEAM_SRC}                      # beam root (wallet/..., core/..., utility/...)
  ${BEAM_SRC}/wallet
  ${BOOST_ANDROID}/include
  ${OPENSSL_ANDROID}/include
  ${BEAM_INCLUDES})                # BEAM's own include dirs (incl. bundled 3rdparty) from its build

# All BEAM static libs from the per-ABI build dir (order resolved by --start-group).
# ADJUST per tag if a needed lib lives outside these dirs (inspect: find ${BEAM_BUILD} -name '*.a').
file(GLOB_RECURSE BEAM_STATIC_LIBS CONFIGURE_DEPENDS ${BEAM_BUILD}/*.a)
# Our reshaped Boost libs for THIS abi (the 1_68-named symlinks to Boost 1.82).
file(GLOB BOOST_LIBS ${BOOST_ANDROID}/libs/${ANDROID_ABI}/libboost_*.a)

find_library(log-lib log)

target_link_libraries(beam
  -Wl,--start-group ${BEAM_STATIC_LIBS} ${BOOST_LIBS} -Wl,--end-group
  ${OPENSSL_ANDROID}/libs/${ANDROID_ABI}/libssl.a
  ${OPENSSL_ANDROID}/libs/${ANDROID_ABI}/libcrypto.a
  ${log-lib})
