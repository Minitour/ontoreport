/**
 * Shared platform -> package resolution for the launcher and the postinstall check.
 */
const PLATFORM_PACKAGES = {
  "linux-x64": "@ontoreport/ontoreport-linux-x64",
  "linux-arm64": "@ontoreport/ontoreport-linux-arm64",
  "darwin-x64": "@ontoreport/ontoreport-darwin-x64",
  "darwin-arm64": "@ontoreport/ontoreport-darwin-arm64",
  "win32-x64": "@ontoreport/ontoreport-win32-x64",
  "win32-arm64": "@ontoreport/ontoreport-win32-arm64",
};

const BINARY_NAME = process.platform === "win32" ? "ontoreport.exe" : "ontoreport";
const platformKey = `${process.platform}-${process.arch}`;

module.exports = {
  PLATFORM_PACKAGES,
  BINARY_NAME,
  platformKey,
  packageName: PLATFORM_PACKAGES[platformKey],
};
