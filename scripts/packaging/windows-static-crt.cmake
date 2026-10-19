# scripts/package.sh's Windows build: SDL3 built against the static C runtime (/MT), as rustc's +crt-static links.
set(CMAKE_MSVC_RUNTIME_LIBRARY MultiThreaded)
