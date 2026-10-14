# BoringSSL prefers cl on Windows unless the compiler is explicit. Keep the
# native GNU build reproducible without relying on an existing CMake cache.
find_program(ONECREW_GCC NAMES gcc REQUIRED)
find_program(ONECREW_GXX NAMES g++ REQUIRED)
set(CMAKE_C_COMPILER "${ONECREW_GCC}" CACHE FILEPATH "GNU C compiler" FORCE)
set(CMAKE_CXX_COMPILER "${ONECREW_GXX}" CACHE FILEPATH "GNU C++ compiler" FORCE)
