/** @type {import('electron-builder').Configuration} */
const config = {
  appId: "com.system-test-sandbox",
  productName: "System Test Sandbox",
  directories: {
    output: "../../dist/electron",
  },
  mac: {
    target: ["dmg"],
    category: "public.app-category.developer-tools",
  },
  files: ["dist/**/*"],
  extraResources: [
    {
      from: "../../target/release/sandbox-daemon",
      to: "sandbox-daemon",
    },
  ],
};

module.exports = config;
