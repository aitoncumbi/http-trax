import type { NextConfig } from "next";

// The UI is a static single-page app: Tauri serves the exported files from
// disk, so there is no Node server at runtime.
const nextConfig: NextConfig = {
  output: "export",
  images: { unoptimized: true },
  devIndicators: false,
  reactStrictMode: true,
};

export default nextConfig;
