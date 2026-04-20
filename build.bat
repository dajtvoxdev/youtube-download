@echo off
REM Build script that sets up VS2019 Build Tools environment for Tauri
set VSBASE=C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Tools\MSVC\14.29.30133
set SDK_VER=10.0.19041.0
set SDK_BASE=C:\Program Files (x86)\Windows Kits\10

set PATH=%VSBASE%\bin\HostX64\x64;%PATH%
set INCLUDE=%VSBASE%\include;%SDK_BASE%\Include\%SDK_VER%\ucrt;%SDK_BASE%\Include\%SDK_VER%\shared;%SDK_BASE%\Include\%SDK_VER%\um;%SDK_BASE%\Include\%SDK_VER%\winrt
set LIB=%VSBASE%\lib\x64;%SDK_BASE%\Lib\%SDK_VER%\um\x64;%SDK_BASE%\Lib\%SDK_VER%\ucrt\x64

echo [VideoDownload] Building release...
npm run tauri build
