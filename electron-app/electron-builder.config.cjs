/** @type {import('electron-builder').Configuration} */
const config = {
  appId: "com.cli-box",
  productName: "CLI Box",
  directories: {
    output: "../dist/electron",
  },
  mac: {
    target: ["dmg"],
    category: "public.app-category.developer-tools",
  },
  files: ["out/**/*"],
  extraResources: [
    {
      from: "../target/release/cli-box-daemon",
      to: "cli-box-daemon",
    },
  ],
};

module.exports = config;
