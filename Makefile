# TOAPIPROXY Build System
# Supports Windows (PowerShell) and Unix-like systems (bash)

ifneq (,$(findstring Windows,$(shell cmd /c echo %OS% 2>nul)))
    detected_OS := Windows
else ifneq (,$(findstring MINGW,$(shell echo $$MSYSTEM)))
    detected_OS := Windows
else ifeq ($(shell uname -s 2>/dev/null || echo ""),Darwin)
    detected_OS := Darwin
else ifneq (,$(shell echo $$OSTYPE 2>/dev/null | grep -i darwin))
    detected_OS := Darwin
else
    detected_OS := Unix
endif

.PHONY: help build clean dev install build-cli-proxy release \
	clean-win-dist clean-mac-dist \
	build-win build-mac \
	build-windows-x64 build-windows-arm64 build-windows-all \
	build-macos-arm64 build-macos-intel build-macos-universal build-macos-all

WINDOWS_X64_TARGET := x86_64-pc-windows-msvc
WINDOWS_ARM64_TARGET := aarch64-pc-windows-msvc
MACOS_ARM64_TARGET := aarch64-apple-darwin
MACOS_INTEL_TARGET := x86_64-apple-darwin
MACOS_UNIVERSAL_TARGET := universal-apple-darwin

ifndef MACOS_SIGNING_IDENTITY
    MACOS_SIGNING_ARGS := --no-sign
else
    MACOS_SIGNING_ARGS := --signing-identity "$(MACOS_SIGNING_IDENTITY)"
endif

help:
	@echo "TOAPIPROXY Build System"
	@echo ""
	@echo "Available targets:"
	@echo "  make build           - Build installer"
	@echo "  make dev             - Run in development mode"
	@echo "  make clean           - Clean build artifacts"
	@echo "  make build-cli-proxy - Build CLIProxyAPIPlus binary only"
	@echo "  make build-win             - Build both Windows installers into build/dist"
	@echo "  make build-mac             - Build both macOS DMGs into build/dist"
	@echo "  make build-windows-x64     - Build Windows NSIS installer for x64"
	@echo "  make build-windows-arm64   - Build Windows NSIS installer for ARM64"
	@echo "  make build-windows-all     - Build both Windows installers"
	@echo "  make build-macos-arm64     - Build macOS app/dmg for Apple Silicon"
	@echo "  make build-macos-intel     - Build macOS app/dmg for Intel"
	@echo "  make build-macos-universal - Build macOS universal app/dmg"
	@echo "  make build-macos-all       - Build both Apple Silicon and Intel macOS app/dmg"
	@echo "  make release         - Build and publish release to Gitee"
	@echo "                         optionally pass VERSION=x.y.z"
	@echo ""
	@echo "Optional macOS signing override:"
	@echo "  make build-macos-arm64 MACOS_SIGNING_IDENTITY=-"
	@echo "  make build-macos-intel MACOS_SIGNING_IDENTITY=\"Developer ID Application: Your Name (TEAMID)\""

ifeq ($(detected_OS),Windows)
    # Windows (PowerShell)
    BUILD_CMD = powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "scripts\\build-app.ps1"
    DEV_CMD = cargo tauri dev
    CLEAN_CMD = cargo tauri build --clean
    CLI_BUILD_CMD = powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "scripts\\build-cli-proxy.ps1"
    RELEASE_CMD = powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "scripts\\release.ps1"
    BUILD_ARGS = -Bundles nsis -CopyInstaller
else
    # Unix-like (macOS, Linux)
    BUILD_CMD = chmod +x scripts/build-app.sh 2>/dev/null; scripts/build-app.sh
    DEV_CMD = cargo tauri dev
    CLEAN_CMD = cargo tauri build --clean
    CLI_BUILD_CMD = chmod +x scripts/build-cli-proxy.sh 2>/dev/null; scripts/build-cli-proxy.sh
    RELEASE_CMD = pwsh -NoLogo -NoProfile -File scripts/release.ps1
    BUILD_ARGS =
endif

build:
	$(BUILD_CMD) $(BUILD_ARGS)

dev:
	$(DEV_CMD)

clean:
	$(CLEAN_CMD)

build-cli-proxy:
	$(CLI_BUILD_CMD)

release:
	$(RELEASE_CMD) $(if $(VERSION),-Version $(VERSION),)

install: build
	@echo "Installer is in build/ directory"

clean-win-dist:
ifeq ($(detected_OS),Windows)
	powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "$$dist = Join-Path '$(CURDIR)' 'build\\dist'; if (Test-Path $$dist) { Get-ChildItem $$dist -File -Filter '*_windows_*' | Remove-Item -Force }; $$legacy = Join-Path '$(CURDIR)' 'build\\windows'; if (Test-Path $$legacy) { Remove-Item $$legacy -Recurse -Force }"
else
	@mkdir -p build/dist
	@find build/dist -maxdepth 1 -type f -name '*_windows_*' -delete
	@rm -rf build/windows
endif

clean-mac-dist:
ifeq ($(detected_OS),Windows)
	powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "$$dist = Join-Path '$(CURDIR)' 'build\\dist'; if (Test-Path $$dist) { Get-ChildItem $$dist -File -Filter '*_macos_*' | Remove-Item -Force }; $$legacy = Join-Path '$(CURDIR)' 'build\\macos'; if (Test-Path $$legacy) { Remove-Item $$legacy -Recurse -Force }"
else
	@mkdir -p build/dist
	@find build/dist -maxdepth 1 -type f -name '*_macos_*' -delete
	@rm -rf build/macos
endif

build-win: clean-win-dist build-windows-all

build-mac: clean-mac-dist build-macos-all

build-windows-x64:
ifeq ($(detected_OS),Windows)
	$(BUILD_CMD) -Target $(WINDOWS_X64_TARGET) -Bundles nsis -CopyInstaller
else
	@echo "build-windows-x64 must be run on Windows."
	@exit 1
endif

build-windows-arm64:
ifeq ($(detected_OS),Windows)
	$(BUILD_CMD) -Target $(WINDOWS_ARM64_TARGET) -Bundles nsis -CopyInstaller
else
	@echo "build-windows-arm64 must be run on Windows."
	@exit 1
endif

build-windows-all: build-windows-x64 build-windows-arm64

build-macos-arm64:
ifeq ($(detected_OS),Darwin)
	$(BUILD_CMD) --target $(MACOS_ARM64_TARGET) --bundles app,dmg $(MACOS_SIGNING_ARGS)
else
	@echo "build-macos-arm64 must be run on macOS."
	@exit 1
endif

build-macos-intel:
ifeq ($(detected_OS),Darwin)
	$(BUILD_CMD) --target $(MACOS_INTEL_TARGET) --bundles app,dmg $(MACOS_SIGNING_ARGS)
else
	@echo "build-macos-intel must be run on macOS."
	@exit 1
endif

build-macos-universal:
ifeq ($(detected_OS),Darwin)
	$(BUILD_CMD) --target $(MACOS_UNIVERSAL_TARGET) --bundles app,dmg $(MACOS_SIGNING_ARGS)
else
	@echo "build-macos-universal must be run on macOS."
	@exit 1
endif

build-macos-all: build-macos-arm64 build-macos-intel
